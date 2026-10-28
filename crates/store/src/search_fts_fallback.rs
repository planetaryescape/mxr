//! Search over the `messages_fts` table, for when the lexical index is down.
//!
//! The table holds subject, sender and snippet only, so this answers a
//! narrower question than the lexical index. Callers translate the queries
//! they can honour into [`FtsFallbackQuery`] and refuse the rest; nothing here
//! guesses. Unchecked queries, like the other internal queries, so the table
//! needs no `.sqlx` cache entry.

use crate::{decode_id, message::future_date_cutoff_timestamp, trace_query};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{system_labels, MessageFlags};
use sqlx::{QueryBuilder, Row, Sqlite};

/// Which `messages_fts` columns a clause looks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsColumns {
    /// Subject, sender and snippet.
    Any,
    Subject,
    /// Sender address and display name.
    Sender,
}

impl FtsColumns {
    /// A fixed FTS5 column filter. Never built from user text.
    fn filter(self) -> &'static str {
        match self {
            Self::Any => "",
            Self::Subject => "{subject} : ",
            Self::Sender => "{from_email from_name} : ",
        }
    }
}

/// One word or phrase that must appear in `columns`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsClause {
    pub columns: FtsColumns,
    pub text: String,
}

/// An AND of clauses over the accounts the caller may show.
#[derive(Debug, Clone)]
pub struct FtsFallbackQuery {
    pub clauses: Vec<FtsClause>,
    /// The accounts whose mail is readable now. The caller resolves this the
    /// same way the lexical path does, so a disabled account never leaks.
    pub visible_accounts: Vec<AccountId>,
    /// Narrows to one account, within `visible_accounts`.
    pub account_id: Option<AccountId>,
    pub hide_trash: bool,
    pub hide_spam: bool,
    pub oldest_first: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsFallbackHit {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone)]
pub struct FtsFallbackPage {
    pub hits: Vec<FtsFallbackHit>,
    pub total: usize,
    pub has_more: bool,
}

/// Quotes `text` as one FTS5 string, so FTS5 syntax in it (`*`, `-`, `NEAR`,
/// `OR`, column filters, unbalanced quotes) is matched as words, not run.
fn quote_phrase(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// The FTS5 `MATCH` expression for an AND of clauses. `None` when there are
/// none, which would otherwise match nothing or everything.
fn match_expression(clauses: &[FtsClause]) -> Option<String> {
    if clauses.is_empty() {
        return None;
    }
    Some(
        clauses
            .iter()
            .map(|clause| format!("{}{}", clause.columns.filter(), quote_phrase(&clause.text)))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

fn push_filters(
    builder: &mut QueryBuilder<Sqlite>,
    query: &FtsFallbackQuery,
    expression: &str,
    visible_accounts_json: &str,
) {
    builder
        .push(" WHERE messages_fts MATCH ")
        .push_bind(expression.to_string())
        .push(" AND m.account_id IN (SELECT value FROM json_each(")
        .push_bind(visible_accounts_json.to_string())
        .push("))");
    if let Some(account_id) = &query.account_id {
        builder
            .push(" AND m.account_id = ")
            .push_bind(account_id.as_str());
    }
    // Both the flag and the label, as the lexical exclusion does: a label
    // change from delta sync does not always set the flag.
    for (hidden, flag, label) in [
        (query.hide_trash, MessageFlags::TRASH, system_labels::TRASH),
        (query.hide_spam, MessageFlags::SPAM, system_labels::SPAM),
    ] {
        if hidden {
            builder
                .push(" AND (m.flags & ")
                .push_bind(i64::from(flag.bits()))
                .push(
                    ") = 0 AND NOT EXISTS (SELECT 1 FROM message_labels ml \
                     JOIN labels l ON l.id = ml.label_id \
                     WHERE ml.message_id = m.id AND UPPER(l.provider_id) = ",
                )
                .push_bind(label.to_string())
                .push(")");
        }
    }
}

impl super::Store {
    /// Messages whose subject, sender or snippet match every clause, newest
    /// first (or oldest first), one page at a time.
    pub async fn search_fts_fallback(
        &self,
        query: &FtsFallbackQuery,
        limit: usize,
        offset: usize,
    ) -> Result<FtsFallbackPage, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let (Some(expression), false) = (
            match_expression(&query.clauses),
            query.visible_accounts.is_empty(),
        ) else {
            return Ok(FtsFallbackPage {
                hits: Vec::new(),
                total: 0,
                has_more: false,
            });
        };
        let visible_accounts_json = serde_json::to_string(
            &query
                .visible_accounts
                .iter()
                .map(AccountId::as_str)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;

        let mut count = QueryBuilder::<Sqlite>::new(
            "SELECT COUNT(*) FROM messages_fts JOIN messages m ON m.rowid = messages_fts.rowid",
        );
        push_filters(&mut count, query, &expression, &visible_accounts_json);
        let total: i64 = count.build_query_scalar().fetch_one(self.reader()).await?;

        let direction = if query.oldest_first { "ASC" } else { "DESC" };
        let mut page = QueryBuilder::<Sqlite>::new(
            "SELECT m.id, m.account_id, m.thread_id \
             FROM messages_fts JOIN messages m ON m.rowid = messages_fts.rowid",
        );
        push_filters(&mut page, query, &expression, &visible_accounts_json);
        page.push(" ORDER BY CASE WHEN m.date > ")
            .push_bind(future_date_cutoff_timestamp())
            .push(format!(
                " THEN 0 ELSE m.date END {direction}, m.id {direction} LIMIT "
            ))
            .push_bind(i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX))
            .push(" OFFSET ")
            .push_bind(i64::try_from(offset).unwrap_or(i64::MAX));
        let rows = page.build().fetch_all(self.reader()).await?;
        trace_query("search.search_fts_fallback", started_at, rows.len());

        let has_more = rows.len() > limit;
        let hits = rows
            .into_iter()
            .take(limit)
            .map(|row| {
                Ok(FtsFallbackHit {
                    message_id: decode_id(row.get::<String, _>("id").as_str())?,
                    account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
                    thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok(FtsFallbackPage {
            hits,
            total: usize::try_from(total).unwrap_or(0),
            has_more,
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use mxr_core::types::{EventSource, Label, LabelKind};
    use mxr_core::{Account, Envelope};

    fn clause(columns: FtsColumns, text: &str) -> FtsClause {
        FtsClause {
            columns,
            text: text.to_string(),
        }
    }

    fn query(clauses: Vec<FtsClause>, accounts: &[&Account]) -> FtsFallbackQuery {
        FtsFallbackQuery {
            clauses,
            visible_accounts: accounts.iter().map(|account| account.id.clone()).collect(),
            account_id: None,
            hide_trash: false,
            hide_spam: false,
            oldest_first: false,
        }
    }

    async fn store_with(accounts: &[&Account], envelopes: &[&Envelope]) -> Store {
        let store = Store::in_memory().await.unwrap();
        for account in accounts {
            store.insert_account(account).await.unwrap();
        }
        for envelope in envelopes {
            store.upsert_envelope(envelope).await.unwrap();
        }
        store
    }

    fn envelope(
        account: &Account,
        provider_id: &str,
        subject: &str,
        from_email: &str,
        snippet: &str,
        days_ago: i64,
    ) -> Envelope {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        envelope.provider_id = provider_id.to_string();
        envelope.subject = subject.to_string();
        envelope.from.email = from_email.to_string();
        envelope.from.name = Some("Sender Person".to_string());
        envelope.snippet = snippet.to_string();
        envelope.date = chrono::Utc::now() - chrono::Duration::days(days_ago);
        envelope
    }

    async fn ids(store: &Store, query: &FtsFallbackQuery) -> Vec<MessageId> {
        store
            .search_fts_fallback(query, 50, 0)
            .await
            .unwrap()
            .hits
            .into_iter()
            .map(|hit| hit.message_id)
            .collect()
    }

    #[tokio::test]
    async fn words_match_subject_sender_and_snippet_newest_first() {
        let account = test_account();
        let in_subject = envelope(&account, "a", "Invoice 42", "x@one.test", "hello", 3);
        let in_snippet = envelope(
            &account,
            "b",
            "Hello",
            "y@two.test",
            "your invoice is due",
            1,
        );
        let in_sender = envelope(&account, "c", "Hi", "invoice@three.test", "nothing", 2);
        let other = envelope(&account, "d", "Lunch", "z@four.test", "sandwiches", 0);
        let store = store_with(&[&account], &[&in_subject, &in_snippet, &in_sender, &other]).await;

        let found = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "invoice")], &[&account]),
        )
        .await;

        assert_eq!(found, vec![in_snippet.id, in_sender.id, in_subject.id]);
    }

    #[tokio::test]
    async fn subject_and_sender_clauses_only_look_in_their_own_columns() {
        let account = test_account();
        let subject_hit = envelope(&account, "a", "Quarterly plan", "x@one.test", "", 1);
        let sender_hit = envelope(&account, "b", "Other", "plan@two.test", "", 2);
        let store = store_with(&[&account], &[&subject_hit, &sender_hit]).await;

        let by_subject = ids(
            &store,
            &query(vec![clause(FtsColumns::Subject, "plan")], &[&account]),
        )
        .await;
        let by_sender = ids(
            &store,
            &query(vec![clause(FtsColumns::Sender, "plan")], &[&account]),
        )
        .await;

        assert_eq!(by_subject, vec![subject_hit.id]);
        assert_eq!(by_sender, vec![sender_hit.id]);
    }

    #[tokio::test]
    async fn clauses_are_anded() {
        let account = test_account();
        let both = envelope(&account, "a", "Invoice", "billing@one.test", "", 1);
        let only_subject = envelope(&account, "b", "Invoice", "x@two.test", "", 2);
        let store = store_with(&[&account], &[&both, &only_subject]).await;

        let found = ids(
            &store,
            &query(
                vec![
                    clause(FtsColumns::Subject, "invoice"),
                    clause(FtsColumns::Sender, "billing"),
                ],
                &[&account],
            ),
        )
        .await;

        assert_eq!(found, vec![both.id]);
    }

    #[tokio::test]
    async fn fts5_syntax_in_user_text_is_matched_as_words() {
        let account = test_account();
        let plain = envelope(&account, "a", "alpha beta", "x@one.test", "", 1);
        let other = envelope(&account, "b", "gamma", "y@two.test", "", 2);
        let quoted = envelope(&account, "c", "she said hello", "z@three.test", "", 3);
        let store = store_with(&[&account], &[&plain, &other, &quoted]).await;

        // Each of these is FTS5 syntax. Run as syntax, `alpha NEAR beta`,
        // `alpha OR gamma` and `-alpha` would match different messages or
        // fail to parse; quoted, they are phrases of literal words.
        for text in [
            "alpha*",
            "-alpha",
            "alpha NEAR beta",
            "alpha OR gamma",
            "subject : alpha",
            "\"unbalanced",
            "unbalanced\"",
            "(alpha",
            "alpha)",
            "^alpha",
            "NOT",
            "AND",
            "a\"\"b",
            "{subject}:alpha",
            "col:alpha",
        ] {
            let result = store
                .search_fts_fallback(
                    &query(vec![clause(FtsColumns::Any, text)], &[&account]),
                    50,
                    0,
                )
                .await;
            assert!(result.is_ok(), "{text:?} must not be a syntax error");
        }

        let star = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "alpha*")], &[&account]),
        )
        .await;
        assert_eq!(star, vec![plain.id.clone()], "`*` must not widen the match");

        let near = ids(
            &store,
            &query(
                vec![clause(FtsColumns::Any, "alpha NEAR beta")],
                &[&account],
            ),
        )
        .await;
        assert!(near.is_empty(), "`NEAR` is a word here, not an operator");

        let or = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "alpha OR gamma")], &[&account]),
        )
        .await;
        assert!(or.is_empty(), "`OR` is a word here, not an operator");

        let phrase = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "said \"hello")], &[&account]),
        )
        .await;
        assert_eq!(phrase, vec![quoted.id], "a stray quote is one literal char");
    }

    #[tokio::test]
    async fn a_clause_with_no_words_matches_nothing_instead_of_failing() {
        let account = test_account();
        let message = envelope(&account, "a", "hello", "x@one.test", "", 1);
        let store = store_with(&[&account], &[&message]).await;

        let found = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "***")], &[&account]),
        )
        .await;

        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn no_clauses_means_no_results_not_everything() {
        let account = test_account();
        let message = envelope(&account, "a", "hello", "x@one.test", "", 1);
        let store = store_with(&[&account], &[&message]).await;

        assert!(ids(&store, &query(Vec::new(), &[&account]))
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn only_visible_accounts_are_searched_and_one_can_be_picked() {
        let first = test_account();
        let second = test_account();
        let disabled = test_account();
        let in_first = envelope(&first, "a", "shared word", "x@one.test", "", 1);
        let in_second = envelope(&second, "b", "shared word", "y@two.test", "", 2);
        let in_disabled = envelope(&disabled, "c", "shared word", "z@three.test", "", 3);
        let store = store_with(
            &[&first, &second, &disabled],
            &[&in_first, &in_second, &in_disabled],
        )
        .await;
        store
            .set_account_enabled(&disabled.id, false)
            .await
            .unwrap();
        let visible = store.list_accounts().await.unwrap();
        let clauses = vec![clause(FtsColumns::Any, "shared")];
        let mut everyone = query(clauses.clone(), &[]);
        everyone.visible_accounts = visible.iter().map(|account| account.id.clone()).collect();

        let all_visible = ids(&store, &everyone).await;
        assert_eq!(all_visible, vec![in_first.id.clone(), in_second.id.clone()]);

        let mut one = everyone.clone();
        one.account_id = Some(second.id.clone());
        assert_eq!(ids(&store, &one).await, vec![in_second.id.clone()]);

        // Naming a disabled account must not reach around the visible set.
        let mut hidden = everyone.clone();
        hidden.account_id = Some(disabled.id.clone());
        assert!(ids(&store, &hidden).await.is_empty());

        let mut nobody = everyone;
        nobody.visible_accounts.clear();
        assert!(ids(&store, &nobody).await.is_empty());
    }

    #[tokio::test]
    async fn trash_and_spam_are_left_out_by_flag_and_by_label() {
        let account = test_account();
        let kept = envelope(&account, "a", "report", "x@one.test", "", 1);
        let mut flagged_trash = envelope(&account, "b", "report", "y@two.test", "", 2);
        flagged_trash.flags = MessageFlags::TRASH;
        let mut flagged_spam = envelope(&account, "c", "report", "z@three.test", "", 3);
        flagged_spam.flags = MessageFlags::SPAM;
        let label_only_trash = envelope(&account, "d", "report", "w@four.test", "", 4);
        let store = store_with(
            &[&account],
            &[&kept, &flagged_trash, &flagged_spam, &label_only_trash],
        )
        .await;
        let trash = Label {
            id: mxr_core::id::LabelId::new(),
            account_id: account.id.clone(),
            name: "Trash".into(),
            kind: LabelKind::System,
            color: None,
            provider_id: "TRASH".into(),
            unread_count: 0,
            total_count: 0,
            role: None,
        };
        store.upsert_label(&trash).await.unwrap();
        store
            .set_message_labels(
                &label_only_trash.id,
                std::slice::from_ref(&trash.id),
                EventSource::Sync,
            )
            .await
            .unwrap();
        let mut query = query(vec![clause(FtsColumns::Any, "report")], &[&account]);

        assert_eq!(
            ids(&store, &query).await.len(),
            4,
            "nothing is hidden by default"
        );

        query.hide_trash = true;
        assert_eq!(
            ids(&store, &query).await,
            vec![kept.id.clone(), flagged_spam.id.clone()]
        );

        query.hide_spam = true;
        assert_eq!(ids(&store, &query).await, vec![kept.id]);
    }

    #[tokio::test]
    async fn pages_total_and_direction_stay_in_one_order() {
        let account = test_account();
        let messages = (0..5)
            .map(|day| {
                envelope(
                    &account,
                    &format!("m{day}"),
                    "digest",
                    "x@one.test",
                    "",
                    day,
                )
            })
            .collect::<Vec<_>>();
        let store = store_with(&[&account], &messages.iter().collect::<Vec<_>>()).await;
        let mut query = query(vec![clause(FtsColumns::Any, "digest")], &[&account]);

        let first = store.search_fts_fallback(&query, 2, 0).await.unwrap();
        let second = store.search_fts_fallback(&query, 2, 2).await.unwrap();
        let last = store.search_fts_fallback(&query, 2, 4).await.unwrap();

        assert_eq!(first.total, 5);
        assert!(first.has_more && second.has_more && !last.has_more);
        let newest_first = first
            .hits
            .iter()
            .chain(&second.hits)
            .chain(&last.hits)
            .map(|hit| hit.message_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            newest_first,
            messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
        );

        query.oldest_first = true;
        let oldest = store.search_fts_fallback(&query, 1, 0).await.unwrap();
        assert_eq!(oldest.hits[0].message_id, messages[4].id);
    }

    #[tokio::test]
    async fn an_updated_message_is_found_by_its_new_words_only() {
        let account = test_account();
        let mut message = envelope(&account, "a", "oldword", "x@one.test", "", 1);
        let store = store_with(&[&account], &[&message]).await;
        message.subject = "newword".to_string();
        store.upsert_envelope(&message).await.unwrap();

        let old = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "oldword")], &[&account]),
        )
        .await;
        let new = ids(
            &store,
            &query(vec![clause(FtsColumns::Any, "newword")], &[&account]),
        )
        .await;

        assert!(old.is_empty());
        assert_eq!(new, vec![message.id]);
    }
}
