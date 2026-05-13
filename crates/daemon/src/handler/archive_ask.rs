//! Slice 3.1 of docs/ai-email/03-archive-intelligence.md.
//!
//! `mxr ask "<question>"` runs a citation-validated retrieval against
//! the local archive. Flow:
//!   1. Apply explicit filters (from/to/after/before) up-front so the
//!      LLM only ever sees in-scope messages.
//!   2. Lexical search via SearchService (semantic fallback deferred —
//!      `executed_mode` reports what actually ran).
//!   3. Fetch top-N envelopes + reader-cleaned bodies and stamp every
//!      one with a `[msg_id=...]` marker the LLM can cite back.
//!   4. LLM `ArchiveAsk` feature → strict JSON.
//!   5. Reject citations whose msg id is not in the retrieved set.

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{
    ArchiveAnswerData, ArchiveAskFiltersData, ArchiveAskMode, ArchiveCitationData,
    ArchiveRetrievalData, ResponseData,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_core::SortOrder;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LlmAsk {
    #[serde(default)]
    answer: String,
    #[serde(default)]
    citations: Vec<LlmCitation>,
}

#[derive(Debug, Deserialize)]
struct LlmCitation {
    #[serde(default)]
    msg_id: String,
    #[serde(default)]
    quote: String,
}

pub(crate) async fn ask(
    state: &AppState,
    question: &str,
    filters: &ArchiveAskFiltersData,
    limit: usize,
) -> super::HandlerResult {
    if question.trim().is_empty() {
        return Err("question cannot be empty".into());
    }

    // Always lexical for the slice. Semantic fallback is a follow-up
    // — we still report the requested mode honestly.
    let requested_mode = filters.mode;

    // Lexical search uses the raw question. Filters are applied at
    // the envelope level — feeding them as tantivy field syntax fails
    // for fields it doesn't index (e.g. 'after').
    let page = state
        .search
        .search(question, (limit * 4).max(1), 0, SortOrder::Relevance)
        .await
        .map_err(|e| e.to_string())?;

    let mut allowed = Vec::new();
    let mut transcript = String::new();
    for hit in page.results.iter() {
        if allowed.len() >= limit {
            break;
        }
        let id: mxr_core::MessageId = match hit.message_id.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let envelope = match state.store.get_envelope(&id).await {
            Ok(Some(env)) => env,
            _ => continue,
        };
        if !pass_filter(&envelope, filters) {
            continue;
        }
        let body = state
            .store
            .get_body(&envelope.id)
            .await
            .ok()
            .flatten()
            .and_then(|b| b.text_plain)
            .unwrap_or_else(|| envelope.snippet.clone());
        let cleaned = clean(Some(&body), None, &ReaderConfig::default()).content;
        transcript.push_str(&format!(
            "[msg_id={}]\nFrom: {}\nTo: {}\nSubject: {}\nDate: {}\n{}\n\n",
            envelope.id,
            envelope.from.email,
            envelope
                .to
                .iter()
                .map(|a| a.email.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            envelope.subject,
            envelope.date.to_rfc3339(),
            cleaned,
        ));
        allowed.push(envelope.id.to_string());
    }

    let runtime = state.llm.for_feature(LlmFeature::ArchiveAsk);
    let req = CompletionRequest {
        max_tokens: Some(900),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(
                "Answer the user's question using ONLY the provided messages. \
                 Output STRICT JSON with the schema and nothing else:\n\
                 {\"answer\": str, \"citations\": [{\"msg_id\": str, \"quote\": str}]}\n\n\
                 Cite ONLY msg_id values that appear in [msg_id=...] markers. \
                 If the messages do not contain enough evidence, set \"answer\" \
                 to a short note such as \"Not enough evidence in archive\" and \
                 return an empty citations array.",
            ),
            ChatMessage::user(format!(
                "MESSAGES:\n{transcript}\n\nQUESTION: {question}\n\nReturn JSON only."
            )),
        ],
    };

    let response = match runtime.complete(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled) | Err(LlmError::PrivacyBlocked(_)) => {
            return Ok(ResponseData::ArchiveAnswer {
                answer: ArchiveAnswerData {
                    text: "Archive ask is disabled or blocked by privacy policy.".into(),
                    citations: vec![],
                    retrieval: ArchiveRetrievalData {
                        requested_mode,
                        executed_mode: ArchiveAskMode::Lexical,
                        candidate_count: allowed.len() as u32,
                    },
                },
            });
        }
        Err(e) => return Err(format!("ArchiveAsk LLM error: {e}")),
    };

    let parsed: LlmAsk = serde_json::from_str(response.content.trim())
        .map_err(|e| format!("ArchiveAsk: LLM returned non-JSON ({e})"))?;

    let allowed_set: std::collections::HashSet<&str> =
        allowed.iter().map(|s| s.as_str()).collect();
    let mut citations = Vec::new();
    for citation in parsed.citations {
        if !allowed_set.contains(citation.msg_id.as_str()) {
            return Err(format!(
                "ArchiveAsk: LLM cited unknown msg_id {} (not in retrieved set)",
                citation.msg_id
            ));
        }
        citations.push(ArchiveCitationData {
            msg_id: citation.msg_id,
            quote: citation.quote,
        });
    }

    Ok(ResponseData::ArchiveAnswer {
        answer: ArchiveAnswerData {
            text: parsed.answer,
            citations,
            retrieval: ArchiveRetrievalData {
                requested_mode,
                executed_mode: ArchiveAskMode::Lexical,
                candidate_count: allowed.len() as u32,
            },
        },
    })
}

fn pass_filter(env: &mxr_core::types::Envelope, f: &ArchiveAskFiltersData) -> bool {
    if let Some(account) = f.account_id.as_ref() {
        if env.account_id != *account {
            return false;
        }
    }
    if let Some(from) = f.from.as_deref() {
        if !env.from.email.eq_ignore_ascii_case(from) {
            return false;
        }
    }
    if let Some(after) = f.after {
        if env.date < after {
            return false;
        }
    }
    if let Some(before) = f.before {
        if env.date > before {
            return false;
        }
    }
    true
}

// `account_id` is informational on the filter struct; the dispatch
// layer fills it in. We accept it here but don't require it.
#[allow(dead_code)]
fn _silence_unused() -> Option<AccountId> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::*;
    use mxr_llm::{
        CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
    };
    use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
    use std::sync::{Arc, Mutex};

    struct CannedLlm {
        body: String,
        last_user: Mutex<String>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CannedLlm {
        async fn complete(
            &self,
            req: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            if let Some(last) = req.messages.last() {
                *self.last_user.lock().unwrap() = last.content.clone();
            }
            Ok(CompletionResponse {
                content: self.body.clone(),
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

    fn envelope(account_id: &AccountId, from: &str, subject: &str, days_ago: i64) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
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
            date: chrono::Utc::now() - chrono::Duration::days(days_ago),
            flags: MessageFlags::empty(),
            snippet: subject.to_string(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        }
    }

    async fn fixture(
        llm: Arc<CannedLlm>,
    ) -> (Arc<crate::state::AppState>, AccountId, Vec<MessageId>) {
        let (state, _) = crate::state::AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        state.llm.replace(llm);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let mut ids = Vec::new();
        let mut entries = Vec::new();
        for i in 0..3 {
            let env = envelope(
                &account_id,
                "alice@example.com",
                &format!("status update {i}"),
                i,
            );
            state
                .store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
            let body = MessageBody {
                message_id: env.id.clone(),
                text_plain: Some(format!("Body of message {i}: status update content.")),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            };
            state.store.insert_body(&body).await.unwrap();
            ids.push(env.id.clone());
            entries.push(SearchIndexEntry {
                envelope: env,
                body: Some(body),
                reply_later: false,
            });
        }
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries,
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();
        (state, account_id, ids)
    }

    #[tokio::test]
    async fn answer_includes_only_cited_msg_ids_from_retrieved_set() {
        let stub = Arc::new(CannedLlm {
            body: String::new(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, ids) = fixture(stub.clone()).await;
        // Replace canned body with valid citations referencing real ids.
        let body = format!(
            r#"{{"answer":"They confirmed the price.","citations":[{{"msg_id":"{}","quote":"price"}}]}}"#,
            ids[0]
        );
        // Re-replace with new stub holding the real-id body.
        let stub2 = Arc::new(CannedLlm {
            body,
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub2.clone());

        let resp = ask(
            &state,
            "what was the status update?",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                ..Default::default()
            },
            5,
        )
        .await
        .unwrap();
        match resp {
            ResponseData::ArchiveAnswer { answer } => {
                assert!(!answer.text.is_empty());
                assert_eq!(answer.citations.len(), 1);
                assert_eq!(answer.citations[0].msg_id, ids[0].to_string());
                assert!(answer.retrieval.candidate_count > 0);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_citation_is_rejected() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"Made up answer","citations":[{"msg_id":"00000000-0000-0000-0000-000000000099","quote":"fabricated"}]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, _ids) = fixture(stub).await;
        let res = ask(
            &state,
            "status",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                ..Default::default()
            },
            5,
        )
        .await;
        let err = res.expect_err("must reject unknown msg_id citation");
        assert!(
            err.contains("00000000-0000-0000-0000-000000000099"),
            "error must name the bad citation: {err}"
        );
    }

    #[tokio::test]
    async fn empty_question_is_rejected() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, _, _) = fixture(stub).await;
        let err = ask(
            &state,
            "   ",
            &ArchiveAskFiltersData::default(),
            5,
        )
        .await
        .expect_err("blank question must be rejected before LLM");
        assert!(err.contains("question cannot be empty"));
    }

    #[tokio::test]
    async fn date_filter_drops_out_of_range_envelopes_from_prompt() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"ok","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, ids) = fixture(stub.clone()).await;
        // Re-stub the same way to keep the prompt-capture mutex stable.
        let stub2 = Arc::new(CannedLlm {
            body: r#"{"answer":"ok","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub2.clone());

        // Only ids[2] is 2 days old; ids[0] is 0 days, ids[1] is 1 day.
        // Filter `after` = 1.5 days ago in the past should keep ids[0]
        // and ids[1] but drop ids[2].
        let after = chrono::Utc::now() - chrono::Duration::hours(36);
        let _ = ask(
            &state,
            "status",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                after: Some(after),
                ..Default::default()
            },
            5,
        )
        .await
        .unwrap();
        let prompt = stub2.last_user.lock().unwrap().clone();
        // ids[2] (2 days old) must NOT be in the prompt.
        assert!(
            !prompt.contains(&ids[2].to_string()),
            "filter must keep out-of-range messages out of the LLM prompt"
        );
        // ids[0] (today) MUST be in the prompt.
        assert!(prompt.contains(&ids[0].to_string()), "in-range message missing");
    }
}
