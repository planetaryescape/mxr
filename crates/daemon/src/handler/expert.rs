//! Slice 5.4 of docs/ai-email/05-context-briefings.md.
//!
//! Find people who *answer* questions in the user's archive that
//! resemble the current message/query. The ranker distinguishes
//! askers from answerers by looking at message order within a
//! thread: an "answer" message is one that came after at least one
//! prior message in the same thread (i.e. it responded to something).

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::SortOrder;
use mxr_protocol::{ExpertSuggestionData, ResponseData};
use std::collections::{HashMap, HashSet};

pub(crate) async fn find(
    state: &AppState,
    account_id: &AccountId,
    query: &str,
    include_self: bool,
    limit: usize,
) -> super::HandlerResult {
    if query.trim().is_empty() {
        return Ok(ResponseData::ExpertSuggestions { experts: vec![] });
    }
    let page = state
        .search
        .search(query, 50, 0, SortOrder::Relevance)
        .await
        .map_err(|e| e.to_string())?;

    let self_addresses = self_addresses_for(state, account_id).await;

    // Group hits by thread and walk in chronological order. Anyone
    // whose message came AFTER at least one earlier message in the
    // same thread counts as an "answerer" for that thread.
    let mut by_thread: HashMap<String, Vec<mxr_core::types::Envelope>> = HashMap::new();
    for hit in page.results.iter() {
        let id: mxr_core::MessageId = match hit.message_id.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let env = match state.store.get_envelope(&id).await {
            Ok(Some(e)) => e,
            _ => continue,
        };
        if env.account_id != *account_id {
            continue;
        }
        by_thread.entry(env.thread_id.to_string()).or_default().push(env);
    }

    let mut by_email: HashMap<String, Aggregate> = HashMap::new();
    for (_thread_id, mut envs) in by_thread {
        // Need full thread context to know first message; pull all
        // envelopes for the thread.
        if envs.is_empty() {
            continue;
        }
        let thread_id = envs[0].thread_id.clone();
        let mut full = match state.store.get_thread_envelopes(&thread_id).await {
            Ok(rows) => rows,
            Err(_) => {
                envs.sort_by(|a, b| a.date.cmp(&b.date));
                envs
            }
        };
        full.sort_by(|a, b| a.date.cmp(&b.date));
        if full.is_empty() {
            continue;
        }
        let first_date = full[0].date;
        for env in &full {
            let email = env.from.email.to_lowercase();
            if !include_self && self_addresses.contains(&email) {
                continue;
            }
            // Only credit messages that came AFTER the first one in
            // the thread -- the first message is the question, every
            // later one is an answer.
            if env.date <= first_date {
                continue;
            }
            let agg = by_email.entry(email.clone()).or_default();
            agg.threads.insert(env.thread_id.to_string());
            if agg.evidence_msg_ids.len() < 5 {
                agg.evidence_msg_ids.push(env.id.to_string());
            }
            if agg.display_name.is_none() {
                agg.display_name = env.from.name.clone();
            }
        }
    }

    let mut experts: Vec<ExpertSuggestionData> = by_email
        .into_iter()
        .map(|(email, agg)| ExpertSuggestionData {
            email,
            display_name: agg.display_name,
            reason: format!(
                "answered in {} similar prior thread(s)",
                agg.threads.len()
            ),
            answered_thread_count: agg.threads.len() as u32,
            evidence_msg_ids: agg.evidence_msg_ids,
        })
        .collect();
    experts.sort_by(|a, b| {
        b.answered_thread_count
            .cmp(&a.answered_thread_count)
            .then(b.evidence_msg_ids.len().cmp(&a.evidence_msg_ids.len()))
    });
    experts.truncate(limit);
    Ok(ResponseData::ExpertSuggestions { experts })
}

#[derive(Default)]
struct Aggregate {
    threads: HashSet<String>,
    evidence_msg_ids: Vec<String>,
    display_name: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
    use std::sync::Arc;

    async fn fixture() -> (Arc<AppState>, AccountId) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        (state, account_id)
    }

    fn env(
        account_id: &AccountId,
        thread_id: &ThreadId,
        from: &str,
        subject: &str,
        body: &str,
        offset_minutes: i64,
    ) -> Envelope {
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
            to: vec![Address {
                name: None,
                email: "user@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.into(),
            date: Utc::now() + Duration::minutes(offset_minutes),
            flags: MessageFlags::empty(),
            snippet: body.into(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
        }
    }

    async fn index(state: &AppState, e: &Envelope, body: &str) {
        let body_full = MessageBody {
            message_id: e.id.clone(),
            text_plain: Some(body.into()),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        };
        state
            .store
            .upsert_envelope_with_direction(e, MessageDirection::Inbound)
            .await
            .unwrap();
        state.store.insert_body(&body_full).await.unwrap();
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries: vec![SearchIndexEntry {
                    envelope: e.clone(),
                    body: Some(body_full),
                    reply_later: false,
                }],
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();
    }

    #[tokio::test]
    async fn answerer_outranks_asker() {
        let (state, account) = fixture().await;
        // Two threads: in each, alice asks first, bob answers. Bob
        // should be the top expert; alice (who only asks) should not
        // appear at all.
        for _ in 0..2 {
            let thread = ThreadId::new();
            let q = env(
                &account,
                &thread,
                "alice@example.com",
                "kafka rebalance question",
                "kafka rebalance question",
                -120,
            );
            let a = env(
                &account,
                &thread,
                "bob@example.com",
                "Re: kafka rebalance question",
                "kafka answer details",
                -60,
            );
            index(&state, &q, "kafka rebalance question").await;
            index(&state, &a, "kafka answer details").await;
        }
        let resp = find(&state, &account, "kafka rebalance", false, 5)
            .await
            .unwrap();
        let ResponseData::ExpertSuggestions { experts } = resp else {
            panic!("unexpected");
        };
        assert!(!experts.is_empty(), "expected at least one expert");
        assert_eq!(experts[0].email, "bob@example.com");
        assert!(
            experts.iter().all(|e| e.email != "alice@example.com"),
            "asker must not be ranked: {experts:?}"
        );
    }

    #[tokio::test]
    async fn citations_point_to_answer_messages_only() {
        let (state, account) = fixture().await;
        let thread = ThreadId::new();
        let q = env(
            &account,
            &thread,
            "alice@example.com",
            "topic q",
            "topic q",
            -120,
        );
        let a = env(
            &account,
            &thread,
            "bob@example.com",
            "Re: topic q",
            "topic answer",
            -60,
        );
        let q_id = q.id.to_string();
        let a_id = a.id.to_string();
        index(&state, &q, "topic q").await;
        index(&state, &a, "topic answer").await;
        let resp = find(&state, &account, "topic", false, 5).await.unwrap();
        let ResponseData::ExpertSuggestions { experts } = resp else {
            panic!("unexpected");
        };
        let bob = experts
            .iter()
            .find(|e| e.email == "bob@example.com")
            .expect("bob expected");
        assert!(bob.evidence_msg_ids.contains(&a_id));
        assert!(
            !bob.evidence_msg_ids.contains(&q_id),
            "evidence must not include the question message"
        );
    }

    #[tokio::test]
    async fn current_user_excluded_by_default() {
        let (state, account) = fixture().await;
        let thread = ThreadId::new();
        let q = env(
            &account,
            &thread,
            "alice@example.com",
            "topic q",
            "topic q",
            -120,
        );
        let me = env(
            &account,
            &thread,
            "user@example.com",
            "Re: topic q",
            "topic answer",
            -60,
        );
        index(&state, &q, "topic q").await;
        index(&state, &me, "topic answer").await;
        let resp = find(&state, &account, "topic", false, 5).await.unwrap();
        let ResponseData::ExpertSuggestions { experts } = resp else {
            panic!("unexpected");
        };
        assert!(
            experts.iter().all(|e| e.email != "user@example.com"),
            "self must be excluded by default: {experts:?}"
        );
    }
}
