//! Shared selection helpers used by both mutation and read commands to
//! resolve a target list from one of:
//!
//! - one or more positional message/thread IDs
//! - a `--search` query (resolved against the daemon's index)
//! - IDs piped on stdin (whitespace-separated)
//!
//! Daemon-native semantics: `--search` is an IPC call against the live
//! index, so resolution happens with the same view the daemon mutators
//! would see — no client-side filtering, no time-of-check vs
//! time-of-use drift beyond the natural inter-call gap. `--first` and
//! `--limit` are presentation modifiers applied after the daemon
//! returns its sorted (date-desc) results.

use crate::commands::progress::format_thousands;
use crate::ipc_client::IpcClient;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{SearchMode, SortOrder};
use mxr_protocol::*;
use serde::Serialize;
use std::io::{IsTerminal, Read, Write};

/// Maximum number of search-resolved IDs to fan out across, no matter
/// what the user typed. Saves the operator from `--search '*'` typos
/// turning into 50k command invocations. Only applies to `--search`;
/// piped/positional IDs are passed through verbatim.
pub const SEARCH_HARD_CAP: u32 = 1000;

/// A `--search` that matched more than `SEARCH_HARD_CAP` messages, so the
/// selection holds only the newest ones. Callers say so before they act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CappedSelection {
    /// Every message the search matched, as the daemon counted them.
    pub matched_total: u32,
    /// How many of them the selection holds.
    pub selected: usize,
}

/// The fields a capped selection adds to JSON and JSONL output. They appear
/// only when the cap applied, and never replace an existing field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CappedFields {
    pub matched_total: u32,
    pub capped: bool,
}

impl CappedSelection {
    /// The one sentence every surface uses: dry-run preview, confirmation
    /// prompt, `--yes` stderr notice and read-only commands.
    pub fn notice(&self) -> String {
        format!(
            "Selected {} of {} matching messages (the newest {}); narrow the search to reach the rest.",
            format_thousands(self.selected),
            format_thousands(self.matched_total),
            format_thousands(self.selected),
        )
    }

    pub fn fields(&self) -> CappedFields {
        CappedFields {
            matched_total: self.matched_total,
            capped: true,
        }
    }

    /// Adds the capped fields to an output object that is serialised from a
    /// type this crate does not own.
    pub fn merge_into(&self, value: &mut serde_json::Value) {
        if let serde_json::Value::Object(object) = value {
            object.insert("matched_total".to_owned(), self.matched_total.into());
            object.insert("capped".to_owned(), true.into());
        }
    }

    /// Writes the notice as one line. Commands call this before any mutation
    /// request goes out when no prompt will show it.
    pub fn announce(&self, out: &mut dyn Write) -> std::io::Result<()> {
        writeln!(out, "{}", self.notice())
    }
}

/// Message IDs a `--search` resolved to, plus whether the hard cap cut them.
#[derive(Debug)]
pub struct MessageSelection {
    pub ids: Vec<MessageId>,
    pub capped: Option<CappedSelection>,
}

/// Thread IDs a `--search` resolved to, plus whether the hard cap cut the
/// messages they were drawn from.
#[derive(Debug)]
pub struct ThreadSelection {
    pub ids: Vec<ThreadId>,
    pub capped: Option<CappedSelection>,
}

/// The search every `--search` selection sends. It is always lexical and
/// newest first, whatever `search.default_mode` is: hybrid and semantic modes
/// rank by relevance and ignore the sort, so past the cap a bulk command would
/// act on a relevance-picked slice instead of the newest mail. Lexical is also
/// deterministic, which is what a mutation wants from "the messages this query
/// matches".
pub(crate) fn selection_search_request(
    query: String,
    limit: u32,
    account_id: Option<&AccountId>,
) -> Request {
    Request::Search {
        query,
        limit,
        offset: 0,
        account_id: account_id.cloned(),
        mode: Some(SearchMode::Lexical),
        sort: Some(SortOrder::DateDesc),
        explain: false,
        limit_from_config: false,
    }
}

/// Parse a single message ID string into a typed `MessageId`. Surfaces
/// the offending input verbatim so users get an actionable error.
pub fn parse_message_id(id_str: &str) -> anyhow::Result<MessageId> {
    let uuid = uuid::Uuid::parse_str(id_str)
        .map_err(|e| anyhow::anyhow!("Invalid message ID '{id_str}': {e}"))?;
    Ok(MessageId::from_uuid(uuid))
}

/// Parse a single thread ID string into a typed `ThreadId`. Same shape
/// as `parse_message_id` — kept distinct so callers can't accidentally
/// pass a message ID where a thread ID is expected.
pub fn parse_thread_id(id_str: &str) -> anyhow::Result<ThreadId> {
    let uuid = uuid::Uuid::parse_str(id_str)
        .map_err(|e| anyhow::anyhow!("Invalid thread ID '{id_str}': {e}"))?;
    Ok(ThreadId::from_uuid(uuid))
}

fn parse_message_ids(id_strs: &[String]) -> anyhow::Result<Vec<MessageId>> {
    id_strs.iter().map(|id| parse_message_id(id)).collect()
}

/// Drain stdin if it's piped (not a tty) and return any whitespace-
/// separated IDs found. Returns `Ok(None)` when stdin is a tty so
/// callers can treat tty + missing-arg as "user didn't say what to
/// operate on" and emit a helpful error.
pub fn read_piped_ids() -> anyhow::Result<Option<Vec<String>>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut input = String::new();
    stdin.read_to_string(&mut input)?;
    let ids = input
        .split_whitespace()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Some(ids))
}

/// Refuses a `--search` selection the daemon answered from the limited SQLite
/// search. It covers less than the index does, so acting on it would quietly
/// skip mail the user's query matches. Reading search results keeps working;
/// changing mail from them does not.
pub fn require_indexed_selection(degraded: Option<&SearchDegraded>) -> anyhow::Result<()> {
    match degraded {
        Some(degraded) => anyhow::bail!(
            "{} A --search selection could miss mail, so nothing was selected; pass message IDs instead.",
            degraded.notice()
        ),
        None => Ok(()),
    }
}

/// Modifier applied to a `--search` resolution. `--first` and
/// `--limit N` are mutually exclusive at the clap layer; this enum
/// exists so the resolver doesn't have to know which flag fired —
/// only how many IDs to keep.
#[derive(Debug, Clone, Copy)]
pub enum SelectionLimit {
    /// Keep at most one — the most recent match. Maps to `--first`.
    First,
    /// Keep up to N. Maps to `--limit N`.
    AtMost(u32),
    /// No client-side cap. Daemon's `SEARCH_HARD_CAP` still applies.
    Unbounded,
}

impl SelectionLimit {
    pub fn from_flags(first: bool, limit: Option<u32>) -> Self {
        match (first, limit) {
            (true, _) => Self::First,
            (false, Some(n)) => Self::AtMost(n),
            (false, None) => Self::Unbounded,
        }
    }

    fn cap(self) -> u32 {
        match self {
            Self::First => 1,
            Self::AtMost(n) => n.min(SEARCH_HARD_CAP),
            Self::Unbounded => SEARCH_HARD_CAP,
        }
    }

    /// Whether the hard cap, not the user's `--first` / `--limit`, decides
    /// how much a search may return.
    fn bound_by_hard_cap(self) -> bool {
        match self {
            Self::First => false,
            Self::AtMost(n) => n > SEARCH_HARD_CAP,
            Self::Unbounded => true,
        }
    }

    /// `Some` when the hard cap cut off matches the search counted.
    fn capped(self, matched_total: u32, selected: usize) -> Option<CappedSelection> {
        (self.bound_by_hard_cap() && matched_total as usize > selected).then_some(CappedSelection {
            matched_total,
            selected,
        })
    }
}

/// Runs the selection search and returns its results with the daemon's match
/// count. The search is refused when the daemon answered from the limited
/// SQLite fallback.
async fn run_selection_search(
    client: &mut IpcClient,
    query: String,
    fetch_limit: u32,
    account_id: Option<&AccountId>,
) -> anyhow::Result<(Vec<SearchResultItem>, u32)> {
    let resp = client
        .request(selection_search_request(query, fetch_limit, account_id))
        .await?;
    match resp {
        Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    total,
                    degraded,
                    ..
                },
        } => {
            require_indexed_selection(degraded.as_ref())?;
            Ok((results, total))
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from search"),
    }
}

/// Resolve a list of message IDs from positional args / `--search` /
/// piped stdin, in that priority order. Returns an error when the
/// caller hasn't provided any input source.
///
/// `limit` only applies to the `--search` path; positional and piped
/// IDs are returned verbatim because the operator already typed them
/// and we shouldn't second-guess the count.
///
/// Mutations use this form: the selection says when the hard cap cut it, so
/// the command can tell the user before it acts.
pub async fn resolve_message_selection(
    client: &mut IpcClient,
    positional: Vec<String>,
    search: Option<String>,
    account_id: Option<&AccountId>,
    limit: SelectionLimit,
) -> anyhow::Result<MessageSelection> {
    match (positional.is_empty(), search) {
        (false, None) => Ok(MessageSelection {
            ids: parse_message_ids(&positional)?,
            capped: None,
        }),
        (false, Some(_)) => anyhow::bail!("Provide message ID(s) or --search, not both"),
        (true, Some(query)) => {
            let (results, total) =
                run_selection_search(client, query, limit.cap(), account_id).await?;
            let capped = limit.capped(total, results.len());
            Ok(MessageSelection {
                ids: results.into_iter().map(|r| r.message_id).collect(),
                capped,
            })
        }
        (true, None) => match read_piped_ids()? {
            Some(ids) if ids.is_empty() => anyhow::bail!("No message IDs provided on stdin"),
            Some(ids) => Ok(MessageSelection {
                ids: parse_message_ids(&ids)?,
                capped: None,
            }),
            None => anyhow::bail!("Provide message ID(s), pipe IDs on stdin, or pass --search"),
        },
    }
}

/// [`resolve_message_selection`] for commands that only read. A capped
/// selection is reported on stderr, so stdout stays the command's output.
pub async fn resolve_message_ids(
    client: &mut IpcClient,
    positional: Vec<String>,
    search: Option<String>,
    account_id: Option<&AccountId>,
    limit: SelectionLimit,
) -> anyhow::Result<Vec<MessageId>> {
    let selection =
        resolve_message_selection(client, positional, search, account_id, limit).await?;
    if let Some(capped) = selection.capped {
        capped.announce(&mut std::io::stderr())?;
    }
    Ok(selection.ids)
}

/// Resolve a list of unique thread IDs from positional args /
/// `--search` / piped stdin. When `--search` is used, message-level
/// search results are deduplicated to thread IDs while preserving the
/// daemon's date-desc ordering — important for `--first` to mean
/// "most recent matching thread".
pub async fn resolve_thread_selection(
    client: &mut IpcClient,
    positional: Vec<String>,
    search: Option<String>,
    account_id: Option<&AccountId>,
    limit: SelectionLimit,
) -> anyhow::Result<ThreadSelection> {
    match (positional.is_empty(), search) {
        (false, None) => Ok(ThreadSelection {
            ids: positional
                .iter()
                .map(|id| parse_thread_id(id))
                .collect::<anyhow::Result<_>>()?,
            capped: None,
        }),
        (false, Some(_)) => anyhow::bail!("Provide thread ID(s) or --search, not both"),
        (true, Some(query)) => {
            // Pull a margin: dedup-by-thread shrinks the result set, so
            // a `--limit 10` on threads needs us to fetch more messages
            // up front. The cap protects against runaway over-fetch.
            let raw_cap = limit.cap();
            let fetch_cap = raw_cap.saturating_mul(4).min(SEARCH_HARD_CAP);
            let (results, total) =
                run_selection_search(client, query, fetch_cap, account_id).await?;
            let capped = limit.capped(total, results.len());
            // Dedup-preserving-order: the first occurrence of each
            // thread_id wins, which (because results are date-desc) is
            // the most recent message in that thread.
            let mut seen = std::collections::HashSet::new();
            let mut threads: Vec<ThreadId> = Vec::new();
            for result in results {
                if seen.insert(result.thread_id.clone()) {
                    threads.push(result.thread_id);
                    if threads.len() as u32 >= raw_cap {
                        break;
                    }
                }
            }
            Ok(ThreadSelection {
                ids: threads,
                capped,
            })
        }
        (true, None) => match read_piped_ids()? {
            Some(ids) if ids.is_empty() => anyhow::bail!("No thread IDs provided on stdin"),
            Some(ids) => Ok(ThreadSelection {
                ids: ids
                    .iter()
                    .map(|id| parse_thread_id(id))
                    .collect::<anyhow::Result<_>>()?,
                capped: None,
            }),
            None => anyhow::bail!("Provide thread ID(s), pipe IDs on stdin, or pass --search"),
        },
    }
}

/// [`resolve_thread_selection`] for commands that only read; the notice goes
/// to stderr.
pub async fn resolve_thread_ids(
    client: &mut IpcClient,
    positional: Vec<String>,
    search: Option<String>,
    account_id: Option<&AccountId>,
    limit: SelectionLimit,
) -> anyhow::Result<Vec<ThreadId>> {
    let selection = resolve_thread_selection(client, positional, search, account_id, limit).await?;
    if let Some(capped) = selection.capped {
        capped.announce(&mut std::io::stderr())?;
    }
    Ok(selection.ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::FakeBulkDaemon;

    fn search_shape(request: &Request) -> (&str, u32, Option<SearchMode>, Option<SortOrder>) {
        let Request::Search {
            query,
            limit,
            mode,
            sort,
            ..
        } = request
        else {
            unreachable!("expected a search request, got {request:?}");
        };
        (query, *limit, *mode, sort.clone())
    }

    #[tokio::test]
    async fn a_search_selection_asks_for_the_newest_matches_in_lexical_mode() {
        let daemon = FakeBulkDaemon::start(3);
        let mut client = daemon.client().await;

        resolve_message_selection(
            &mut client,
            Vec::new(),
            Some("label:news".into()),
            None,
            SelectionLimit::Unbounded,
        )
        .await
        .unwrap();

        let searches = daemon.search_requests();
        assert_eq!(searches.len(), 1);
        // `mode: None` would follow `search.default_mode`, and hybrid ranks by
        // relevance and ignores the sort.
        assert_eq!(
            search_shape(&searches[0]),
            (
                "label:news",
                SEARCH_HARD_CAP,
                Some(SearchMode::Lexical),
                Some(SortOrder::DateDesc)
            )
        );
    }

    #[tokio::test]
    async fn a_thread_selection_searches_the_same_way() {
        let daemon = FakeBulkDaemon::start(3);
        let mut client = daemon.client().await;

        resolve_thread_selection(
            &mut client,
            Vec::new(),
            Some("label:news".into()),
            None,
            SelectionLimit::AtMost(10),
        )
        .await
        .unwrap();

        // 10 threads fetch 40 messages as a margin for dedup.
        assert_eq!(
            search_shape(&daemon.search_requests()[0]),
            (
                "label:news",
                40,
                Some(SearchMode::Lexical),
                Some(SortOrder::DateDesc)
            )
        );
    }

    #[tokio::test]
    async fn more_matches_than_the_cap_keep_the_newest_and_report_the_total() {
        let daemon = FakeBulkDaemon::start(1_523);
        let mut client = daemon.client().await;

        let selection = resolve_message_selection(
            &mut client,
            Vec::new(),
            Some("*".into()),
            None,
            SelectionLimit::Unbounded,
        )
        .await
        .unwrap();

        assert_eq!(selection.ids, daemon.message_ids[..1_000]);
        assert_eq!(
            selection.capped,
            Some(CappedSelection {
                matched_total: 1_523,
                selected: 1_000
            })
        );
    }

    #[tokio::test]
    async fn matches_within_the_cap_report_nothing() {
        let daemon = FakeBulkDaemon::start(1_000);
        let mut client = daemon.client().await;

        let selection = resolve_message_selection(
            &mut client,
            Vec::new(),
            Some("*".into()),
            None,
            SelectionLimit::Unbounded,
        )
        .await
        .unwrap();

        assert_eq!(selection.ids.len(), 1_000);
        assert_eq!(selection.capped, None);
    }

    #[tokio::test]
    async fn a_limit_the_user_asked_for_is_not_reported_as_a_cap() {
        let daemon = FakeBulkDaemon::start(1_523);
        let mut client = daemon.client().await;
        for limit in [SelectionLimit::First, SelectionLimit::AtMost(50)] {
            let selection =
                resolve_message_selection(&mut client, Vec::new(), Some("*".into()), None, limit)
                    .await
                    .unwrap();
            assert_eq!(selection.capped, None, "{limit:?}");
        }
    }

    #[tokio::test]
    async fn a_limit_above_the_cap_is_cut_by_the_cap_and_says_so() {
        let daemon = FakeBulkDaemon::start(1_523);
        let mut client = daemon.client().await;

        let selection = resolve_message_selection(
            &mut client,
            Vec::new(),
            Some("*".into()),
            None,
            SelectionLimit::AtMost(5_000),
        )
        .await
        .unwrap();

        assert_eq!(selection.ids.len(), 1_000);
        assert!(selection.capped.is_some());
    }

    #[tokio::test]
    async fn thread_selection_reports_the_cap_on_the_messages_it_searched() {
        let daemon = FakeBulkDaemon::start(1_523);
        let mut client = daemon.client().await;

        let selection = resolve_thread_selection(
            &mut client,
            Vec::new(),
            Some("*".into()),
            None,
            SelectionLimit::Unbounded,
        )
        .await
        .unwrap();

        assert_eq!(
            selection.capped,
            Some(CappedSelection {
                matched_total: 1_523,
                selected: 1_000
            })
        );
    }

    #[tokio::test]
    async fn explicit_ids_skip_the_search_and_the_cap() {
        let daemon = FakeBulkDaemon::start(5_000);
        let mut client = daemon.client().await;
        let ids: Vec<String> = daemon.message_ids[..1_200]
            .iter()
            .map(MessageId::as_str)
            .collect();

        let selection =
            resolve_message_selection(&mut client, ids, None, None, SelectionLimit::Unbounded)
                .await
                .unwrap();

        assert_eq!(selection.ids.len(), 1_200);
        assert_eq!(selection.capped, None);
        assert!(daemon.search_requests().is_empty());
    }

    #[test]
    fn the_notice_names_how_many_were_selected_out_of_how_many_matched() {
        let capped = CappedSelection {
            matched_total: 1_523,
            selected: 1_000,
        };

        assert_eq!(
            capped.notice(),
            "Selected 1,000 of 1,523 matching messages (the newest 1,000); narrow the search to reach the rest."
        );
        let mut out = Vec::new();
        capped.announce(&mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            format!("{}\n", capped.notice())
        );
    }

    #[test]
    fn the_capped_fields_are_added_to_an_existing_object() {
        let capped = CappedSelection {
            matched_total: 1_523,
            selected: 1_000,
        };
        let mut value = serde_json::json!({ "identity": "kept" });

        capped.merge_into(&mut value);

        assert_eq!(
            value,
            serde_json::json!({ "identity": "kept", "matched_total": 1_523, "capped": true })
        );
        assert_eq!(
            serde_json::to_value(capped.fields()).unwrap(),
            serde_json::json!({ "matched_total": 1_523, "capped": true })
        );
    }

    #[test]
    fn a_selection_from_the_limited_search_is_refused() {
        let degraded = SearchDegraded::new(SearchDegradedReason::IndexStopped);

        let error = require_indexed_selection(Some(&degraded))
            .expect_err("a limited selection must be refused")
            .to_string();

        assert!(error.contains("Search is limited"), "{error}");
        assert!(error.contains("pass message IDs instead"), "{error}");
        assert!(require_indexed_selection(None).is_ok());
    }

    #[test]
    fn selection_limit_first_caps_at_one() {
        assert_eq!(SelectionLimit::First.cap(), 1);
    }

    #[test]
    fn selection_limit_at_most_respects_user_value_below_hard_cap() {
        assert_eq!(SelectionLimit::AtMost(50).cap(), 50);
    }

    #[test]
    fn selection_limit_at_most_clamps_to_hard_cap() {
        // The hard cap saves users from `--search '*' --limit 100000`.
        // Any value at or above SEARCH_HARD_CAP should clamp.
        assert_eq!(
            SelectionLimit::AtMost(SEARCH_HARD_CAP + 5).cap(),
            SEARCH_HARD_CAP
        );
    }

    #[test]
    fn selection_limit_unbounded_uses_hard_cap() {
        // Unbounded still has a ceiling — the hard cap. This is the
        // operator-protection guarantee.
        assert_eq!(SelectionLimit::Unbounded.cap(), SEARCH_HARD_CAP);
    }

    #[test]
    fn from_flags_prefers_first_over_limit() {
        // Clap should make `--first` and `--limit` mutually exclusive,
        // but defence-in-depth: if both arrive, `--first` wins.
        let limit = SelectionLimit::from_flags(true, Some(50));
        assert!(matches!(limit, SelectionLimit::First));
    }

    #[test]
    fn parse_message_id_rejects_non_uuid_with_input_echoed() {
        let err = parse_message_id("not-a-uuid").unwrap_err().to_string();
        assert!(err.contains("not-a-uuid"));
    }

    #[test]
    fn parse_thread_id_rejects_non_uuid_with_input_echoed() {
        let err = parse_thread_id("xyz").unwrap_err().to_string();
        assert!(err.contains("xyz"));
    }
}
