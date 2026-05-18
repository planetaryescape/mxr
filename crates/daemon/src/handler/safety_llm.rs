//! LLM-backed safety checks. Currently only `answer_coverage`
//! (Slice 1.4 of docs/ai-email/01-pre-send-safety.md).
//!
//! Lives in the daemon — not the `mxr-safety` crate — so that
//! `mxr-safety` stays purely deterministic and depends only on
//! `mxr-core` / `mxr-reader` / `mxr-relationship`. The daemon owns
//! the LLM runtime and calls these functions when a draft check
//! requests `allow_llm` and a thread context is available.

use crate::state::AppState;
use mxr_core::types::{
    CitationRef, Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity,
    MessageDirection,
};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_reader::{clean, ReaderConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LlmAsk {
    question: String,
    evidence_msg_id: String,
    addressed: bool,
    #[serde(default)]
    #[allow(dead_code)]
    draft_evidence: String,
}

#[derive(Debug, Deserialize)]
struct LlmAnswerCoverage {
    asks: Vec<LlmAsk>,
}

/// Run the LLM-backed answer-coverage check against the parent thread.
///
/// Returns at most one issue. If the LLM is disabled, blocked, or
/// errors, returns an Info-severity degradation note instead of
/// surfacing nothing — so the caller can show the user that the check
/// did not run.
pub(crate) async fn check_answer_coverage(
    state: &AppState,
    draft: &Draft,
    thread_id: &mxr_core::ThreadId,
) -> Vec<DraftSafetyIssue> {
    let envelopes = match state.store.get_thread_envelopes(thread_id).await {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => return Vec::new(),
        Err(e) => {
            return vec![degradation(format!(
                "answer-coverage: failed to load thread {thread_id}: {e}"
            ))]
        }
    };

    let ids: Vec<_> = envelopes.iter().map(|e| e.id.clone()).collect();
    let directions = match state.store.list_message_directions_by_ids(&ids).await {
        Ok(d) => d,
        Err(e) => {
            return vec![degradation(format!(
                "answer-coverage: failed to load directions: {e}"
            ))]
        }
    };

    let inbound: Vec<_> = envelopes
        .iter()
        .filter(|e| {
            matches!(
                directions
                    .get(&e.id)
                    .copied()
                    .unwrap_or(MessageDirection::Unknown),
                MessageDirection::Inbound
            )
        })
        .collect();
    if inbound.is_empty() {
        return Vec::new();
    }

    // Allowed citation set: every inbound msg id loaded above.
    let allowed: std::collections::HashSet<String> =
        inbound.iter().map(|e| e.id.to_string()).collect();

    // Build prompt. Use reader-cleaned snippets to keep token use sane.
    let mut transcript = String::new();
    for env in inbound {
        let body = match state.store.get_body(&env.id).await {
            Ok(Some(b)) => b.text_plain.unwrap_or_default(),
            _ => env.snippet.clone(),
        };
        let cleaned = clean(
            Some(&body),
            None,
            &ReaderConfig {
                strip_signatures: true,
                collapse_quotes: true,
                strip_boilerplate: true,
                strip_tracking: true,
                html_command: None,
            },
        )
        .content;
        transcript.push_str(&format!(
            "[msg_id={}]\nFrom: {}\nSubject: {}\n{}\n\n",
            env.id, env.from.email, env.subject, cleaned
        ));
    }

    let cleaned_draft = clean(Some(&draft.body_markdown), None, &ReaderConfig::default()).content;

    let runtime = state.llm.for_feature(LlmFeature::AnswerCoverage);
    let req = CompletionRequest {
        max_tokens: Some(800),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(
                "You extract explicit asks from an email thread and check whether the \
                 user's draft addresses each one. Output STRICT JSON with this schema and \
                 nothing else:\n\n\
                 {\"asks\": [{\"question\": str, \"evidence_msg_id\": str, \"addressed\": \
                 bool, \"draft_evidence\": str}]}\n\n\
                 Only cite evidence_msg_id values that appear in the [msg_id=...] \
                 markers in the thread. If there are no asks, return {\"asks\": []}.",
            ),
            ChatMessage::user(format!(
                "THREAD:\n{transcript}\n\nDRAFT REPLY:\n{cleaned_draft}\n\nReturn JSON only."
            )),
        ],
    };

    let response = match runtime.complete(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled) | Err(LlmError::PrivacyBlocked(_)) => {
            return vec![degradation(
                "answer-coverage skipped: LLM disabled or blocked by privacy policy".to_string(),
            )]
        }
        Err(e) => {
            return vec![degradation(format!(
                "answer-coverage skipped: LLM error: {e}"
            ))];
        }
    };

    let parsed: LlmAnswerCoverage = match serde_json::from_str(response.content.trim()) {
        Ok(p) => p,
        Err(e) => {
            return vec![degradation(format!(
                "answer-coverage: LLM returned non-JSON ({e})"
            ))]
        }
    };

    let mut missing = Vec::new();
    for ask in parsed.asks {
        // Critical: reject any ask whose evidence_msg_id is not in the
        // retrieved set. The LLM must not invent message ids.
        if !allowed.contains(&ask.evidence_msg_id) {
            tracing::warn!(
                msg_id = %ask.evidence_msg_id,
                "answer-coverage: LLM cited unknown msg_id; ignoring entry"
            );
            continue;
        }
        if !ask.addressed {
            missing.push(ask);
        }
    }

    if missing.is_empty() {
        return Vec::new();
    }

    let summary = if missing.len() == 1 {
        format!("draft does not address: {}", missing[0].question)
    } else {
        format!(
            "draft does not address {} asks; first: {}",
            missing.len(),
            missing[0].question
        )
    };
    let citations = missing
        .iter()
        .map(|m| CitationRef {
            message_id: Some(m.evidence_msg_id.clone()),
            thread_id: Some(thread_id.to_string()),
            field: "body".into(),
            quote: m.question.clone(),
        })
        .collect();

    vec![DraftSafetyIssue::new(
        DraftSafetyIssueCode::AnswerCoverage,
        DraftSafetySeverity::Warning,
        summary,
    )
    .with_citations(citations)]
}

fn degradation(reason: String) -> DraftSafetyIssue {
    DraftSafetyIssue::new(
        DraftSafetyIssueCode::AnswerCoverage,
        DraftSafetySeverity::Info,
        reason,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::{Address, Draft, DraftIntent, MessageBody, MessageMetadata};
    use mxr_llm::{
        ChatRole, CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct StubLlm {
        canned: Mutex<Option<String>>,
        last_request: Mutex<Option<CompletionRequest>>,
        force_disabled: Mutex<bool>,
    }

    impl StubLlm {
        fn with_canned(content: &str) -> Self {
            Self {
                canned: Mutex::new(Some(content.to_string())),
                last_request: Mutex::new(None),
                force_disabled: Mutex::new(false),
            }
        }
        fn disabled() -> Self {
            Self {
                canned: Mutex::new(None),
                last_request: Mutex::new(None),
                force_disabled: Mutex::new(true),
            }
        }
        fn last_user_text(&self) -> String {
            self.last_request
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|r| {
                    r.messages
                        .iter()
                        .rev()
                        .find(|m| matches!(m.role, ChatRole::User))
                        .map(|m| m.content.clone())
                })
                .unwrap_or_default()
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for StubLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.last_request.lock().unwrap() = Some(req);
            if *self.force_disabled.lock().unwrap() {
                return Err(LlmError::Disabled);
            }
            let content = self
                .canned
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "{\"asks\": []}".into());
            Ok(CompletionResponse {
                content,
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
        llm: Arc<dyn LlmProvider>,
    ) -> (
        Arc<AppState>,
        mxr_core::AccountId,
        mxr_core::ThreadId,
        Vec<mxr_core::MessageId>,
    ) {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        state.llm.replace(llm);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let thread_id = mxr_core::ThreadId::new();
        let mut ids = Vec::new();
        for (i, q) in [
            "Can you confirm the price?",
            "And the launch timeline?",
            "Lastly, who owns rollout?",
        ]
        .iter()
        .enumerate()
        {
            let env = mxr_core::types::Envelope {
                id: mxr_core::MessageId::new(),
                account_id: account_id.clone(),
                provider_id: format!("msg-{i}"),
                thread_id: thread_id.clone(),
                message_id_header: Some(format!("<msg-{i}@example.com>")),
                in_reply_to: None,
                references: vec![],
                from: Address {
                    name: Some("Alice".into()),
                    email: "alice@example.com".into(),
                },
                to: vec![Address {
                    name: None,
                    email: "user@example.com".into(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: format!("Q {i}"),
                date: chrono::Utc::now(),
                flags: mxr_core::types::MessageFlags::empty(),
                snippet: q.to_string(),
                has_attachments: false,
                size_bytes: 1024,
                unsubscribe: mxr_core::types::UnsubscribeMethod::None,
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
                text_plain: Some(format!("Hi,\n\n{q}\n\nThanks,\nAlice")),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            };
            state.store.insert_body(&body).await.unwrap();
            ids.push(env.id);
        }
        (state, account_id, thread_id, ids)
    }

    fn draft_for(account_id: mxr_core::AccountId, body: &str) -> Draft {
        Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: DraftIntent::Reply,
            to: vec![Address {
                name: None,
                email: "alice@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "re".into(),
            body_markdown: body.into(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn warns_with_citation_when_one_ask_unaddressed() {
        let stub = Arc::new(StubLlm::default());
        let (state, account_id, thread_id, ids) = fixture(stub.clone()).await;
        let canned = format!(
            r#"{{"asks":[
                {{"question":"price","evidence_msg_id":"{}","addressed":true,"draft_evidence":"$10"}},
                {{"question":"timeline","evidence_msg_id":"{}","addressed":true,"draft_evidence":"Q3"}},
                {{"question":"owner","evidence_msg_id":"{}","addressed":false,"draft_evidence":""}}
            ]}}"#,
            ids[0], ids[1], ids[2]
        );
        *stub.canned.lock().unwrap() = Some(canned);

        let draft = draft_for(account_id, "$10, Q3 launch.");
        let issues = check_answer_coverage(&state, &draft, &thread_id).await;
        assert_eq!(issues.len(), 1, "expected one warning, got {issues:?}");
        let issue = &issues[0];
        assert_eq!(issue.severity, DraftSafetySeverity::Warning);
        assert_eq!(issue.code, DraftSafetyIssueCode::AnswerCoverage);
        assert!(issue.message.to_lowercase().contains("owner"));
        assert_eq!(issue.citations.len(), 1);
        assert_eq!(
            issue.citations[0].message_id.as_deref(),
            Some(ids[2].to_string().as_str())
        );
    }

    #[tokio::test]
    async fn rejects_unknown_msg_id_citation() {
        let stub = Arc::new(StubLlm::with_canned(
            r#"{"asks":[
                {"question":"made up","evidence_msg_id":"00000000-0000-0000-0000-000000000099","addressed":false,"draft_evidence":""}
            ]}"#,
        ));
        let (state, account_id, thread_id, _) = fixture(stub).await;
        let draft = draft_for(account_id, "ok");
        let issues = check_answer_coverage(&state, &draft, &thread_id).await;
        assert!(
            issues.is_empty(),
            "ask with unknown msg_id must be rejected, got {issues:?}"
        );
    }

    #[tokio::test]
    async fn llm_disabled_emits_info_degradation() {
        let stub = Arc::new(StubLlm::disabled());
        let (state, account_id, thread_id, _) = fixture(stub).await;
        let draft = draft_for(account_id, "ok");
        let issues = check_answer_coverage(&state, &draft, &thread_id).await;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Info);
        assert!(
            issues[0].message.to_lowercase().contains("disabled"),
            "{}",
            issues[0].message
        );
    }

    #[tokio::test]
    async fn prompt_contains_thread_transcript_and_draft_body() {
        let stub = Arc::new(StubLlm::with_canned(r#"{"asks":[]}"#));
        let (state, account_id, thread_id, _) = fixture(stub.clone()).await;
        let draft = draft_for(account_id, "UNIQUE-DRAFT-MARKER-XYZ");
        let _ = check_answer_coverage(&state, &draft, &thread_id).await;
        let user_text = stub.last_user_text();
        assert!(
            user_text.contains("UNIQUE-DRAFT-MARKER-XYZ"),
            "prompt missing draft body marker, got:\n{user_text}"
        );
        assert!(
            user_text.contains("[msg_id="),
            "prompt missing msg_id markers, got:\n{user_text}"
        );
    }
}
