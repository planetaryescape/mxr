//! Slice 5.3 of docs/reference/ai-email.md
//!
//! "Maybe include" recipient suggestions. Build a query from the
//! draft, hybrid-retrieve prior threads (lexical for the slice;
//! semantic deferred), count participants who appear repeatedly
//! across topic-similar threads, exclude self addresses and any
//! addresses already on the draft, and never reveal Bcc'd
//! recipients unless the current user is the original sender.

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::Draft;
use mxr_core::SortOrder;
use mxr_protocol::{ResponseData, SuggestedRecipientData};
use std::collections::{HashMap, HashSet};

const MIN_SUPPORT_THREADS: usize = 3;

pub(crate) async fn suggest(state: &AppState, draft: &Draft, limit: usize) -> super::HandlerResult {
    let query = topic_query(draft);
    if query.trim().is_empty() {
        return Ok(ResponseData::SuggestedCollaborators {
            suggestions: vec![],
        });
    }
    let page = state
        .search
        .search(&query, 50, 0, SortOrder::Relevance)
        .await?;

    let self_addresses = self_addresses_for(state, &draft.account_id).await;
    let on_draft: HashSet<String> = draft
        .to
        .iter()
        .chain(draft.cc.iter())
        .chain(draft.bcc.iter())
        .map(|a| a.email.to_lowercase())
        .collect();

    // Per-candidate aggregate: distinct thread_ids (for support) +
    // evidence msg ids (for citations). Bcc-leak safety: a candidate
    // who appears on a thread *only* because of a bcc on a message
    // not sent by the user is excluded; the simplest way to enforce
    // that is to ignore Bcc fields entirely when iterating candidates.
    let mut by_email: HashMap<String, Aggregate> = HashMap::new();
    for hit in page.results.iter() {
        let id: mxr_core::MessageId = match hit.message_id.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let env = match state.store.get_envelope(&id).await {
            Ok(Some(e)) => e,
            _ => continue,
        };
        if env.account_id != draft.account_id {
            continue;
        }
        let participants = collect_participants_no_bcc(&env);
        for email in participants {
            let key = email.to_lowercase();
            if self_addresses.contains(&key) || on_draft.contains(&key) {
                continue;
            }
            let agg = by_email.entry(key.clone()).or_default();
            agg.thread_ids.insert(env.thread_id.to_string());
            if agg.evidence_msg_ids.len() < 5 {
                agg.evidence_msg_ids.push(env.id.to_string());
            }
            if agg.display_name.is_none() {
                agg.display_name = candidate_name(&env, &key);
            }
        }
    }

    let mut suggestions: Vec<SuggestedRecipientData> = by_email
        .into_iter()
        .filter_map(|(email, agg)| {
            let support = agg.thread_ids.len();
            if support < MIN_SUPPORT_THREADS {
                return None;
            }
            let confidence = if support >= 5 { "high" } else { "medium" };
            Some(SuggestedRecipientData {
                email,
                display_name: agg.display_name,
                reason: format!("co-participant on {support} similar prior thread(s)"),
                confidence: confidence.into(),
                evidence_msg_ids: agg.evidence_msg_ids,
            })
        })
        .collect();
    suggestions.sort_by_key(|suggestion| std::cmp::Reverse(suggestion.evidence_msg_ids.len()));
    suggestions.truncate(limit);

    Ok(ResponseData::SuggestedCollaborators { suggestions })
}

#[derive(Default)]
struct Aggregate {
    thread_ids: HashSet<String>,
    evidence_msg_ids: Vec<String>,
    display_name: Option<String>,
}

fn topic_query(draft: &Draft) -> String {
    let first_para = draft
        .body_markdown
        .split("\n\n")
        .next()
        .unwrap_or("")
        .replace('\n', " ");
    format!("{} {}", draft.subject.trim(), first_para.trim())
        .trim()
        .to_string()
}

async fn self_addresses_for(state: &AppState, account_id: &AccountId) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(Some(account)) = state.store.get_account(account_id).await {
        set.insert(account.email.to_lowercase());
    }
    if let Ok(addrs) = state.store.list_account_addresses(account_id).await {
        for addr in addrs {
            set.insert(addr.email.to_lowercase());
        }
    }
    set
}

/// Bcc safety: do NOT include bcc participants in the candidate set.
/// Even when the current user was the sender, Bcc'd addresses are
/// confidential; we never surface them as suggestions.
fn collect_participants_no_bcc(env: &mxr_core::types::Envelope) -> Vec<String> {
    std::iter::once(env.from.email.clone())
        .chain(env.to.iter().map(|a| a.email.clone()))
        .chain(env.cc.iter().map(|a| a.email.clone()))
        .collect()
}

fn candidate_name(env: &mxr_core::types::Envelope, email: &str) -> Option<String> {
    if env.from.email.eq_ignore_ascii_case(email) {
        return env.from.name.clone();
    }
    for a in env.to.iter().chain(env.cc.iter()) {
        if a.email.eq_ignore_ascii_case(email) {
            return a.name.clone();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
    use std::sync::Arc;

    struct EnvelopeFixture<'a> {
        from: &'a str,
        to: Vec<&'a str>,
        cc: Vec<&'a str>,
        bcc: Vec<&'a str>,
        subject: &'a str,
        body: &'a str,
    }

    fn envelope(
        account_id: &AccountId,
        thread_id: &ThreadId,
        fixture: EnvelopeFixture<'_>,
    ) -> Envelope {
        let EnvelopeFixture {
            from,
            to,
            cc,
            bcc,
            subject,
            body,
        } = fixture;
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("p-{}", uuid::Uuid::now_v7()),
            thread_id: thread_id.clone(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: from.into(),
            },
            to: to
                .iter()
                .map(|e| Address {
                    name: None,
                    email: (*e).into(),
                })
                .collect(),
            cc: cc
                .iter()
                .map(|e| Address {
                    name: None,
                    email: (*e).into(),
                })
                .collect(),
            bcc: bcc
                .iter()
                .map(|e| Address {
                    name: None,
                    email: (*e).into(),
                })
                .collect(),
            subject: subject.into(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: body.into(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    async fn fixture() -> (Arc<AppState>, AccountId) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        (state, account_id)
    }

    async fn index(state: &AppState, env: &Envelope, body_text: &str) {
        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some(body_text.into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        state
            .store
            .upsert_envelope_with_direction(env, MessageDirection::Inbound)
            .await
            .unwrap();
        state.store.insert_body(&body).await.unwrap();
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries: vec![SearchIndexEntry {
                    envelope: env.clone(),
                    body: Some(body),
                    reply_later: false,
                }],
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();
    }

    fn draft(account_id: &AccountId, subject: &str, body: &str, to: Vec<&str>) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: account_id.clone(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: to
                .iter()
                .map(|e| Address {
                    name: None,
                    email: (*e).into(),
                })
                .collect(),
            cc: vec![],
            bcc: vec![],
            subject: subject.into(),
            body_markdown: body.into(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn repeated_co_participant_is_suggested() {
        let (state, account) = fixture().await;
        // Three threads about "rollout planning" co-starring bob.
        for _ in 0..3 {
            let thread = ThreadId::new();
            let env = envelope(
                &account,
                &thread,
                EnvelopeFixture {
                    from: "alice@example.com",
                    to: vec!["user@example.com", "bob@example.com"],
                    cc: vec![],
                    bcc: vec![],
                    subject: "rollout planning",
                    body: "rollout planning details",
                },
            );
            index(&state, &env, "rollout planning details").await;
        }
        let d = draft(
            &account,
            "rollout planning kick-off",
            "starting rollout planning",
            vec!["alice@example.com"],
        );
        let resp = suggest(&state, &d, 5).await.unwrap();
        match resp {
            ResponseData::SuggestedCollaborators { suggestions } => {
                assert!(
                    suggestions.iter().any(|s| s.email == "bob@example.com"),
                    "bob expected, got {suggestions:?}"
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn one_off_thread_does_not_yield_suggestions() {
        let (state, account) = fixture().await;
        let thread = ThreadId::new();
        let env = envelope(
            &account,
            &thread,
            EnvelopeFixture {
                from: "alice@example.com",
                to: vec!["user@example.com", "bob@example.com"],
                cc: vec![],
                bcc: vec![],
                subject: "rollout",
                body: "one-off rollout",
            },
        );
        index(&state, &env, "one-off rollout").await;
        let d = draft(&account, "rollout", "rollout", vec!["alice@example.com"]);
        let resp = suggest(&state, &d, 5).await.unwrap();
        let ResponseData::SuggestedCollaborators { suggestions } = resp else {
            panic!("unexpected");
        };
        assert!(
            suggestions.is_empty(),
            "must not suggest from a single thread (got {suggestions:?})"
        );
    }

    #[tokio::test]
    async fn bcc_recipient_is_never_leaked_as_suggestion() {
        let (state, account) = fixture().await;
        // Three "rollout" threads where carol is Bcc'd. Carol must
        // never appear as a suggestion.
        for _ in 0..3 {
            let thread = ThreadId::new();
            let env = envelope(
                &account,
                &thread,
                EnvelopeFixture {
                    from: "alice@example.com",
                    to: vec!["user@example.com"],
                    cc: vec![],
                    bcc: vec!["carol@example.com"],
                    subject: "rollout secret",
                    body: "rollout secret",
                },
            );
            index(&state, &env, "rollout secret").await;
        }
        let d = draft(
            &account,
            "rollout secret",
            "rollout secret",
            vec!["alice@example.com"],
        );
        let resp = suggest(&state, &d, 10).await.unwrap();
        let ResponseData::SuggestedCollaborators { suggestions } = resp else {
            panic!("unexpected");
        };
        assert!(
            suggestions.iter().all(|s| s.email != "carol@example.com"),
            "Bcc'd address must not leak as suggestion: {suggestions:?}"
        );
    }

    #[tokio::test]
    async fn existing_recipients_are_excluded() {
        let (state, account) = fixture().await;
        for _ in 0..3 {
            let thread = ThreadId::new();
            let env = envelope(
                &account,
                &thread,
                EnvelopeFixture {
                    from: "alice@example.com",
                    to: vec!["user@example.com", "bob@example.com"],
                    cc: vec![],
                    bcc: vec![],
                    subject: "rollout",
                    body: "rollout details",
                },
            );
            index(&state, &env, "rollout details").await;
        }
        // Bob is already on the draft -- must not be suggested.
        let d = draft(
            &account,
            "rollout follow-up",
            "rollout follow-up",
            vec!["alice@example.com", "bob@example.com"],
        );
        let resp = suggest(&state, &d, 10).await.unwrap();
        let ResponseData::SuggestedCollaborators { suggestions } = resp else {
            panic!("unexpected");
        };
        assert!(
            suggestions.iter().all(|s| s.email != "bob@example.com"),
            "draft recipients must not be re-suggested: {suggestions:?}"
        );
    }
}
