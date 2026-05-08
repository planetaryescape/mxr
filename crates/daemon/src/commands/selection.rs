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

use crate::ipc_client::IpcClient;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::SortOrder;
use mxr_protocol::*;
use std::io::{IsTerminal, Read};

/// Maximum number of search-resolved IDs to fan out across, no matter
/// what the user typed. Saves the operator from `--search '*'` typos
/// turning into 50k command invocations. Only applies to `--search`;
/// piped/positional IDs are passed through verbatim.
pub const SEARCH_HARD_CAP: u32 = 1000;

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
}

/// Resolve a list of message IDs from positional args / `--search` /
/// piped stdin, in that priority order. Returns an error when the
/// caller hasn't provided any input source.
///
/// `limit` only applies to the `--search` path; positional and piped
/// IDs are returned verbatim because the operator already typed them
/// and we shouldn't second-guess the count.
pub async fn resolve_message_ids(
    client: &mut IpcClient,
    positional: Vec<String>,
    search: Option<String>,
    limit: SelectionLimit,
) -> anyhow::Result<Vec<MessageId>> {
    match (positional.is_empty(), search) {
        (false, None) => parse_message_ids(&positional),
        (false, Some(_)) => anyhow::bail!("Provide message ID(s) or --search, not both"),
        (true, Some(query)) => {
            let resp = client
                .request(Request::Search {
                    query,
                    limit: limit.cap(),
                    offset: 0,
                    mode: None,
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::SearchResults { results, .. },
                } => Ok(results.into_iter().map(|r| r.message_id).collect()),
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response from search"),
            }
        }
        (true, None) => match read_piped_ids()? {
            Some(ids) if ids.is_empty() => anyhow::bail!("No message IDs provided on stdin"),
            Some(ids) => parse_message_ids(&ids),
            None => anyhow::bail!("Provide message ID(s), pipe IDs on stdin, or pass --search"),
        },
    }
}

/// Resolve a list of unique thread IDs from positional args /
/// `--search` / piped stdin. When `--search` is used, message-level
/// search results are deduplicated to thread IDs while preserving the
/// daemon's date-desc ordering — important for `--first` to mean
/// "most recent matching thread".
pub async fn resolve_thread_ids(
    client: &mut IpcClient,
    positional: Vec<String>,
    search: Option<String>,
    limit: SelectionLimit,
) -> anyhow::Result<Vec<ThreadId>> {
    match (positional.is_empty(), search) {
        (false, None) => positional.iter().map(|id| parse_thread_id(id)).collect(),
        (false, Some(_)) => anyhow::bail!("Provide thread ID(s) or --search, not both"),
        (true, Some(query)) => {
            // Pull a margin: dedup-by-thread shrinks the result set, so
            // a `--limit 10` on threads needs us to fetch more messages
            // up front. The cap protects against runaway over-fetch.
            let raw_cap = limit.cap();
            let fetch_cap = raw_cap.saturating_mul(4).min(SEARCH_HARD_CAP);
            let resp = client
                .request(Request::Search {
                    query,
                    limit: fetch_cap,
                    offset: 0,
                    mode: None,
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                })
                .await?;
            let results = match resp {
                Response::Ok {
                    data: ResponseData::SearchResults { results, .. },
                } => results,
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response from search"),
            };
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
            Ok(threads)
        }
        (true, None) => match read_piped_ids()? {
            Some(ids) if ids.is_empty() => anyhow::bail!("No thread IDs provided on stdin"),
            Some(ids) => ids.iter().map(|id| parse_thread_id(id)).collect(),
            None => anyhow::bail!("Provide thread ID(s), pipe IDs on stdin, or pass --search"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
