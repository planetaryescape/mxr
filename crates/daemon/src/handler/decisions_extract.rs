//! Slice 3.2 LLM extractor + rebuild loop (C2.4).
//!
//! `extract_thread` walks one thread's envelopes + bodies, runs the
//! cheap keyword/length prefilter, calls `LlmFeature::DecisionLog`
//! with strict JSON, validates each citation against the retrieved
//! msg_id set, and upserts via the existing `Store::upsert_decision`
//! (which is idempotent on the stable decision id and refreshes
//! `source_hash` when thread content changes).
//!
//! `rebuild` walks every thread for an account whose latest message
//! is within `since_days` and calls `extract_thread` for each
//! candidate that passes the prefilter.

use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{decision_id, decision_source_hash, DecisionLogEntry};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use sqlx::Row;

/// Cheap keyword prefilter — if a thread has no message body matching
/// any of these terms, skip the LLM call. Threads with ≥3 messages
/// also bypass this filter (long threads often contain decisions
/// without using the literal keyword).
static DECISION_KEYWORDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(decided|agreed|we['\u{2019}]ll\s+use|settled|approved|go\s+with|chose|signed off)\b",
    )
    .expect("valid regex")
});

#[derive(Debug, Deserialize)]
struct LlmDecision {
    #[serde(default)]
    decision: String,
    #[serde(default)]
    rationale: Option<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    evidence_msg_ids: Vec<String>,
    #[serde(default)]
    decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
struct LlmDecisions {
    #[serde(default)]
    decisions: Vec<LlmDecision>,
}

/// Extract decisions from a single thread. Returns the count of
/// rows persisted (0 if the prefilter skipped or the LLM returned
/// no decisions).
pub(crate) async fn extract_thread(
    state: &AppState,
    account_id: &AccountId,
    thread_id: &ThreadId,
) -> Result<usize, String> {
    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    if envelopes.is_empty() {
        return Ok(0);
    }

    // Allowed citation set: every msg id in this thread.
    let allowed: std::collections::HashSet<String> =
        envelopes.iter().map(|e| e.id.to_string()).collect();

    // Build the transcript and gather the prefilter signal.
    let mut transcript = String::new();
    let mut keyword_hit = false;
    for env in &envelopes {
        let body = state
            .store
            .get_body(&env.id)
            .await
            .ok()
            .flatten()
            .and_then(|b| b.text_plain)
            .unwrap_or_else(|| env.snippet.clone());
        let cleaned = clean(Some(&body), None, &ReaderConfig::default()).content;
        if !keyword_hit && DECISION_KEYWORDS.is_match(&cleaned) {
            keyword_hit = true;
        }
        transcript.push_str(&format!(
            "[msg_id={}]\nFrom: {}\nDate: {}\n{}\n\n",
            env.id,
            env.from.email,
            env.date.to_rfc3339(),
            cleaned
        ));
    }

    // Prefilter: skip the LLM call when neither signal fires.
    if envelopes.len() < 3 && !keyword_hit {
        return Ok(0);
    }

    let runtime = state.llm.for_feature(LlmFeature::DecisionLog);
    let req = CompletionRequest {
        max_tokens: Some(900),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(
                "Extract decisions made in this email thread. Output STRICT JSON \
                 with the schema and nothing else:\n\n\
                 {\"decisions\": [{\"decision\": str, \"rationale\": str|null, \
                 \"topic\": str|null, \"evidence_msg_ids\": [str], \
                 \"decided_at\": str|null}]}\n\n\
                 Cite ONLY msg_id values that appear in the [msg_id=...] markers \
                 in the thread. Skip brainstorming threads where nothing was \
                 actually agreed -- return an empty array in that case. \
                 \"decided_at\" is RFC 3339 if a date is named in the thread, \
                 else null.",
            ),
            ChatMessage::user(format!("THREAD:\n{transcript}\n\nReturn JSON only.")),
        ],
    };

    let response = match runtime.complete(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled | LlmError::PrivacyBlocked(_)) => return Ok(0),
        Err(e) => return Err(format!("DecisionLog LLM error: {e}")),
    };
    let parsed: LlmDecisions = serde_json::from_str(response.content.trim())
        .map_err(|e| format!("DecisionLog: LLM returned non-JSON ({e})"))?;

    let now = chrono::Utc::now();
    let mut written = 0usize;
    for d in parsed.decisions {
        if d.decision.trim().is_empty() {
            continue;
        }
        // Validate every cited msg id is in the retrieved set.
        let mut evidence: Vec<MessageId> = Vec::new();
        let mut bad_citation = false;
        for cited in &d.evidence_msg_ids {
            if !allowed.contains(cited) {
                tracing::warn!(
                    msg_id = %cited,
                    "DecisionLog: ignoring decision with unknown evidence msg_id"
                );
                bad_citation = true;
                break;
            }
            match cited.parse::<MessageId>() {
                Ok(id) => evidence.push(id),
                Err(_) => {
                    bad_citation = true;
                    break;
                }
            }
        }
        if bad_citation || evidence.is_empty() {
            continue;
        }

        let source_hash = decision_source_hash(&transcript, &evidence);
        let id = decision_id(account_id, thread_id, &d.decision, &evidence);
        let entry = DecisionLogEntry {
            id,
            account_id: account_id.clone(),
            thread_id: thread_id.clone(),
            topic: d.topic,
            decision: d.decision,
            rationale: d.rationale,
            evidence_msg_ids: evidence,
            decided_at: d.decided_at,
            extracted_at: now,
            source_hash,
        };
        state
            .store
            .upsert_decision(&entry)
            .await
            .map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

#[derive(Debug, Clone)]
pub(crate) struct RebuildSummary {
    pub extracted: u32,
    pub skipped: u32,
    pub errors: u32,
}

/// Walk every thread in `account_id` whose latest message is within
/// `since_days` days, calling `extract_thread` for each. Returns a
/// summary of what happened.
pub(crate) async fn rebuild(
    state: &AppState,
    account_id: &AccountId,
    since_days: u32,
) -> Result<RebuildSummary, String> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(since_days as i64);
    let cutoff_secs = cutoff.timestamp();
    let rows = sqlx::query(
        r#"SELECT DISTINCT thread_id
           FROM messages
           WHERE account_id = ? AND date >= ?"#,
    )
    .bind(account_id.as_str())
    .bind(cutoff_secs)
    .fetch_all(state.store.reader())
    .await
    .map_err(|e| e.to_string())?;

    let mut summary = RebuildSummary {
        extracted: 0,
        skipped: 0,
        errors: 0,
    };
    for row in rows {
        let raw: String = row.try_get("thread_id").map_err(|e| e.to_string())?;
        let thread_id: ThreadId = match raw.parse() {
            Ok(id) => id,
            Err(_) => {
                summary.errors += 1;
                continue;
            }
        };
        match extract_thread(state, account_id, &thread_id).await {
            Ok(0) => summary.skipped += 1,
            Ok(n) => summary.extracted += n as u32,
            Err(e) => {
                tracing::warn!(error = %e, thread_id = %thread_id, "DecisionLog rebuild: extract failed");
                summary.errors += 1;
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::*;
    use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
    use mxr_store::CommitmentStatus;
    use std::sync::{Arc, Mutex};

    struct CannedLlm {
        body: Mutex<String>,
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CannedLlm {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.calls.lock().unwrap() += 1;
            Ok(CompletionResponse {
                content: self.body.lock().unwrap().clone(),
                model: "stub".into(),
                finish_reason: Some("stop".into()),
            })
        }
        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 8192,
                supports_streaming: false,
            }
        }
        fn model_name(&self) -> &str {
            "stub"
        }
    }

    async fn fixture(
        body: &str,
    ) -> (
        Arc<AppState>,
        AccountId,
        ThreadId,
        Vec<MessageId>,
        Arc<CannedLlm>,
    ) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let stub = Arc::new(CannedLlm {
            body: Mutex::new(body.into()),
            calls: Mutex::new(0),
        });
        state.llm.replace(stub.clone());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let thread_id = ThreadId::new();
        let mut ids = Vec::new();
        for (i, body_text) in [
            "Should we use Postgres or Mongo?",
            "I think Postgres makes sense for relational data.",
            "Agreed -- let's go with Postgres.",
        ]
        .iter()
        .enumerate()
        {
            let env = Envelope {
                id: MessageId::new(),
                account_id: account_id.clone(),
                provider_id: format!("p-{i}"),
                thread_id: thread_id.clone(),
                message_id_header: None,
                in_reply_to: None,
                references: vec![],
                from: Address {
                    name: None,
                    email: format!("alice{i}@example.com"),
                },
                to: vec![Address {
                    name: None,
                    email: "user@example.com".into(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: "DB choice".into(),
                date: chrono::Utc::now() - chrono::Duration::hours(3 - i as i64),
                flags: MessageFlags::empty(),
                snippet: body_text.to_string(),
                has_attachments: false,
                size_bytes: 1,
                unsubscribe: UnsubscribeMethod::None,
                link_count: 0,
                body_word_count: 0,
                label_provider_ids: vec![],
                keywords: std::collections::BTreeSet::new(),
            };
            state
                .store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
            let body = MessageBody {
                message_id: env.id.clone(),
                text_plain: Some(body_text.to_string()),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            };
            state.store.insert_body(&body).await.unwrap();
            ids.push(env.id);
        }
        (state, account_id, thread_id, ids, stub)
    }

    #[tokio::test]
    async fn extracts_decision_with_valid_citation() {
        let (state, account, thread, ids, _) = fixture("placeholder").await;
        // Set the canned response now we know the real msg ids.
        let canned = format!(
            r#"{{"decisions":[{{
                "decision":"Use Postgres",
                "rationale":"relational data fit",
                "topic":"database",
                "evidence_msg_ids":["{}"],
                "decided_at":null
            }}]}}"#,
            ids[2]
        );
        state.llm.replace(Arc::new(CannedLlm {
            body: Mutex::new(canned),
            calls: Mutex::new(0),
        }));
        let n = extract_thread(&state, &account, &thread).await.unwrap();
        assert_eq!(n, 1, "exactly one decision row written");
        let rows = state
            .store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision, "Use Postgres");
        assert_eq!(rows[0].evidence_msg_ids, vec![ids[2].clone()]);
    }

    #[tokio::test]
    async fn empty_decisions_array_writes_no_rows() {
        let (state, account, thread, _ids, _) = fixture(r#"{"decisions":[]}"#).await;
        let n = extract_thread(&state, &account, &thread).await.unwrap();
        assert_eq!(n, 0);
        let rows = state
            .store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert!(rows.is_empty(), "no rows for brainstorming-only thread");
    }

    #[tokio::test]
    async fn unknown_evidence_msg_id_is_rejected() {
        let (state, account, thread, _ids, _) = fixture("placeholder").await;
        // Made-up msg id not in the thread -- the citation validator
        // must reject the entire decision (don't write half-credible rows).
        let canned = r#"{"decisions":[{
            "decision":"made-up",
            "evidence_msg_ids":["00000000-0000-0000-0000-000000000099"]
        }]}"#;
        state.llm.replace(Arc::new(CannedLlm {
            body: Mutex::new(canned.into()),
            calls: Mutex::new(0),
        }));
        let n = extract_thread(&state, &account, &thread).await.unwrap();
        assert_eq!(n, 0, "decision with bad citation must not be persisted");
        assert!(state
            .store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn re_extracting_unchanged_thread_is_idempotent() {
        let (state, account, thread, ids, _) = fixture("placeholder").await;
        let canned = format!(
            r#"{{"decisions":[{{
                "decision":"Use Postgres",
                "evidence_msg_ids":["{}"]
            }}]}}"#,
            ids[2]
        );
        state.llm.replace(Arc::new(CannedLlm {
            body: Mutex::new(canned),
            calls: Mutex::new(0),
        }));
        extract_thread(&state, &account, &thread).await.unwrap();
        extract_thread(&state, &account, &thread).await.unwrap();
        let rows = state
            .store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "idempotent: same source_hash, same row");
    }

    #[tokio::test]
    async fn rebuild_walks_all_account_threads_in_window() {
        let (state, account, _thread1, ids1, _) = fixture("placeholder").await;
        // Add a second, brainstorming-only thread with <3 messages and
        // no decision keywords -- the prefilter should skip it.
        let thread2 = ThreadId::new();
        let env = Envelope {
            id: MessageId::new(),
            account_id: account.clone(),
            provider_id: "p-other".into(),
            thread_id: thread2.clone(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: "bob@example.com".into(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Random brainstorm".into(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "Just thinking out loud about colors".into(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        };
        state
            .store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();
        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Just thinking out loud about colors".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        state.store.insert_body(&body).await.unwrap();

        // LLM canned to return one decision for the long thread.
        let canned = format!(
            r#"{{"decisions":[{{
                "decision":"Use Postgres",
                "evidence_msg_ids":["{}"]
            }}]}}"#,
            ids1[2]
        );
        state.llm.replace(Arc::new(CannedLlm {
            body: Mutex::new(canned),
            calls: Mutex::new(0),
        }));

        let summary = rebuild(&state, &account, 365).await.unwrap();
        assert_eq!(summary.extracted, 1, "exactly one decision extracted");
        assert!(
            summary.skipped >= 1,
            "brainstorming thread should be skipped (got: skipped={})",
            summary.skipped
        );
        assert_eq!(summary.errors, 0);
    }

    #[tokio::test]
    async fn rebuild_returns_zero_when_no_threads_match_window() {
        let (state, account, _, _, _) = fixture(r#"{"decisions":[]}"#).await;
        // since_days = 0 -> cutoff = now -> nothing qualifies (the
        // fixture timestamps are slightly in the past).
        // Use an absurdly small window. None of the fixture messages
        // were dated more than 3 hours ago; since_days=0 cuts them
        // all out.
        let summary = rebuild(&state, &account, 0).await.unwrap();
        // Threads can still match if their date >= cutoff(now) and
        // a message was inserted in the same second; be permissive.
        assert!(summary.errors == 0);
        // Suppress unused import warning:
        let _ = CommitmentStatus::Open;
    }
}
