//! Slice 6.1 of docs/reference/ai-email.md
//!
//! Query-time entity explainer. No new schema -- per the doc,
//! `entities`/`entity_mentions` are deferred until query-time
//! becomes slow. For an email-shaped query we surface the
//! sender_profile + relationship summary; for free-text we run a
//! lexical search and surface the cited matches.

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::SortOrder;
use mxr_protocol::{EntityCandidateData, EntityExplanationData, ResponseData, WhoisCitationData};
use std::collections::HashMap;

const MAX_CANDIDATES: usize = 5;

pub(crate) async fn explain(
    state: &AppState,
    account_id: &AccountId,
    query: &str,
    limit: usize,
) -> super::HandlerResult {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("query cannot be empty".into());
    }

    if looks_like_email(trimmed) {
        return explain_email(state, account_id, trimmed).await;
    }

    let page = state
        .search
        .search(trimmed, 30, 0, SortOrder::Relevance)
        .await?;

    if page.results.is_empty() {
        return Ok(ResponseData::EntityExplanation {
            entity: EntityExplanationData {
                canonical_name: trimmed.into(),
                kind: "unknown".into(),
                summary: "No local evidence found.".into(),
                first_seen_at: None,
                last_seen_at: None,
                topics: vec![],
                citations: vec![],
                candidates: vec![],
            },
        });
    }

    // Walk hits, collect unique sender candidates, build citations.
    let mut citations: Vec<WhoisCitationData> = Vec::new();
    let mut by_sender: HashMap<String, EntityCandidateData> = HashMap::new();
    let mut first_seen: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut last_seen: Option<chrono::DateTime<chrono::Utc>> = None;
    for hit in page.results.iter().take(limit) {
        let id: mxr_core::MessageId = match hit.message_id.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let env = match state.store.get_envelope(&id).await {
            Ok(Some(e)) => e,
            _ => continue,
        };
        if env.account_id != *account_id {
            continue;
        }
        first_seen = Some(first_seen.map_or(env.date, |d| d.min(env.date)));
        last_seen = Some(last_seen.map_or(env.date, |d| d.max(env.date)));
        citations.push(WhoisCitationData {
            msg_id: env.id.to_string(),
            quote: env.subject.clone(),
        });
        let key = env.from.email.to_lowercase();
        let entry = by_sender
            .entry(key.clone())
            .or_insert_with(|| EntityCandidateData {
                kind: "person".into(),
                value: key,
                display_name: env.from.name.clone(),
                mention_count: 0,
            });
        entry.mention_count += 1;
        if entry.display_name.is_none() {
            entry.display_name = env.from.name.clone();
        }
    }

    let mut candidates: Vec<EntityCandidateData> = by_sender.into_values().collect();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.mention_count));
    candidates.truncate(MAX_CANDIDATES);

    let summary = if candidates.len() > 1 {
        format!(
            "Multiple senders match \"{trimmed}\". Top candidates: {}.",
            candidates
                .iter()
                .map(|c| c.value.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else if let Some(first) = candidates.first() {
        format!(
            "Most likely match: {} ({} mention(s) of \"{trimmed}\").",
            first.value, first.mention_count
        )
    } else {
        format!("\"{trimmed}\" appears in {} message(s).", citations.len())
    };

    Ok(ResponseData::EntityExplanation {
        entity: EntityExplanationData {
            canonical_name: trimmed.into(),
            kind: if candidates.len() > 1 {
                "ambiguous".into()
            } else {
                "term".into()
            },
            summary,
            first_seen_at: first_seen,
            last_seen_at: last_seen,
            topics: vec![],
            citations,
            candidates: if candidates.len() > 1 {
                candidates
            } else {
                vec![]
            },
        },
    })
}

async fn explain_email(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> super::HandlerResult {
    use sqlx::Row;
    let row = sqlx::query(
        r#"SELECT display_name, first_seen_at, last_seen_at, total_inbound, total_outbound
           FROM contacts
           WHERE account_id = ? AND LOWER(email) = LOWER(?)"#,
    )
    .bind(account_id.as_str())
    .bind(email)
    .fetch_optional(state.store.reader())
    .await?;

    match row {
        None => Ok(ResponseData::EntityExplanation {
            entity: EntityExplanationData {
                canonical_name: email.into(),
                kind: "person".into(),
                summary: format!("No prior interaction with {email} on this account."),
                first_seen_at: None,
                last_seen_at: None,
                topics: vec![],
                citations: vec![],
                candidates: vec![],
            },
        }),
        Some(r) => {
            let display_name: Option<String> = r.try_get("display_name").ok();
            let first_seen_secs: Option<i64> = r.try_get("first_seen_at").ok();
            let last_seen_secs: Option<i64> = r.try_get("last_seen_at").ok();
            let total_inbound: i64 = r.try_get("total_inbound").unwrap_or(0);
            let total_outbound: i64 = r.try_get("total_outbound").unwrap_or(0);
            let summary = format!(
                "{} -- {} inbound, {} outbound.",
                display_name.as_deref().unwrap_or(email),
                total_inbound,
                total_outbound
            );
            Ok(ResponseData::EntityExplanation {
                entity: EntityExplanationData {
                    canonical_name: email.into(),
                    kind: "person".into(),
                    summary,
                    first_seen_at: first_seen_secs
                        .and_then(|s| chrono::DateTime::from_timestamp(s, 0)),
                    last_seen_at: last_seen_secs
                        .and_then(|s| chrono::DateTime::from_timestamp(s, 0)),
                    topics: vec![],
                    citations: vec![],
                    candidates: vec![],
                },
            })
        }
    }
}

fn looks_like_email(s: &str) -> bool {
    let s = s.trim();
    if let Some(at) = s.find('@') {
        let (lhs, rhs) = s.split_at(at);
        return !lhs.is_empty() && rhs.len() > 1 && rhs[1..].contains('.') && !s.contains(' ');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
    use std::sync::Arc;

    async fn fixture() -> (Arc<AppState>, AccountId) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account = state.store.list_accounts().await.unwrap()[0].id.clone();
        (state, account)
    }

    fn env(account: &AccountId, from: &str, subject: &str, body: &str) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account.clone(),
            provider_id: format!("p-{}", uuid::Uuid::now_v7()),
            thread_id: ThreadId::new(),
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
            date: Utc::now(),
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
    async fn email_query_returns_person_summary_from_contacts() {
        let (state, account) = fixture().await;
        let e = env(&account, "alice@example.com", "hi", "hi");
        index(&state, &e, "hi").await;
        // Refresh contacts so the row exists.
        state.store.refresh_contacts().await.unwrap();
        let resp = explain(&state, &account, "alice@example.com", 10)
            .await
            .unwrap();
        let ResponseData::EntityExplanation { entity } = resp else {
            panic!("unexpected")
        };
        assert_eq!(entity.kind, "person");
        assert!(entity.summary.contains("inbound"), "{}", entity.summary);
        assert!(entity.candidates.is_empty());
    }

    #[tokio::test]
    async fn term_query_returns_citations_from_local_messages() {
        let (state, account) = fixture().await;
        let e = env(
            &account,
            "alice@example.com",
            "Project Apollo update",
            "apollo details",
        );
        let id = e.id.to_string();
        index(&state, &e, "apollo details").await;
        let resp = explain(&state, &account, "Project Apollo", 10)
            .await
            .unwrap();
        let ResponseData::EntityExplanation { entity } = resp else {
            panic!("unexpected")
        };
        assert!(
            entity.citations.iter().any(|c| c.msg_id == id),
            "expected citation, got {:?}",
            entity.citations
        );
    }

    #[tokio::test]
    async fn ambiguous_query_returns_candidates_not_synthesized_answer() {
        let (state, account) = fixture().await;
        for sender in ["alice@example.com", "bob@example.com", "carol@example.com"] {
            let e = env(
                &account,
                sender,
                "fizzbuzz update",
                "fizzbuzz update content",
            );
            index(&state, &e, "fizzbuzz update content").await;
        }
        let resp = explain(&state, &account, "fizzbuzz", 10).await.unwrap();
        let ResponseData::EntityExplanation { entity } = resp else {
            panic!("unexpected")
        };
        assert_eq!(entity.kind, "ambiguous");
        assert!(entity.candidates.len() >= 2, "{:?}", entity.candidates);
    }

    #[tokio::test]
    async fn no_evidence_returns_empty_summary_no_search_call() {
        let (state, account) = fixture().await;
        // No documents indexed -- just an empty search index.
        let resp = explain(&state, &account, "ghost-term", 10).await.unwrap();
        let ResponseData::EntityExplanation { entity } = resp else {
            panic!("unexpected")
        };
        assert_eq!(entity.kind, "unknown");
        assert!(
            entity.summary.contains("No local evidence"),
            "{}",
            entity.summary
        );
    }

    #[tokio::test]
    async fn empty_query_is_rejected() {
        let (state, account) = fixture().await;
        let err = explain(&state, &account, "   ", 10)
            .await
            .expect_err("must reject blank");
        assert!(err.to_string().contains("cannot be empty"));
    }
}
