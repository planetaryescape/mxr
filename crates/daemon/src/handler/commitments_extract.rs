use super::time::due_words_in_zone;
use crate::state::AppState;
use mxr_core::types::Draft;
use mxr_llm::{
    guarded_system_prompt, wrap_untrusted_mail, ChatMessage, CompletionRequest, LlmError,
    LlmFeature,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{new_candidate_id, CommitmentDirection, DraftCommitmentCandidate};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;

pub(super) static COMMITMENT_PREFILTER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(I['\u{2019}]ll|I\s+will|I\s+can|I['\u{2019}]ll\s+send|I['\u{2019}]ll\s+follow\s+up|I['\u{2019}]ll\s+get\s+back)\b",
    )
    .expect("valid regex")
});

#[derive(Debug, Deserialize)]
struct LlmCommitment {
    #[serde(default)]
    who_owes: String,
    #[serde(default)]
    what: String,
    #[serde(default)]
    due: Option<String>,
    #[serde(default)]
    direction: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LlmCommitmentsBatch {
    #[serde(default)]
    commitments: Vec<LlmCommitment>,
}

pub(crate) async fn extract_and_store(
    state: &AppState,
    draft: &Draft,
) -> Result<Vec<DraftCommitmentCandidate>, String> {
    let cleaned = {
        let (text, html) = draft.content.reader_input();
        clean(text, html, &ReaderConfig::default())
    }
    .content;
    if !COMMITMENT_PREFILTER.is_match(&cleaned) {
        return Ok(Vec::new());
    }

    let mail_block = format!(
        "FROM: {}\nTO: {}\n\nDRAFT:\n{cleaned}",
        first_email(&draft.to),
        draft
            .to
            .iter()
            .map(|a| a.email.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let runtime = state.llm.for_feature(LlmFeature::Commitments);
    let req = CompletionRequest {
        max_tokens: Some(600),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(guarded_system_prompt(
                "You extract concrete future-tense commitments the SENDER is making. \
                 Output STRICT JSON with the schema: \
                 {\"commitments\": [{\"who_owes\": str, \"what\": str, \
                 \"due\": str|null, \"direction\": \"yours\"|\"theirs\"}]}\n\n\
                 Rules: \"what\" must name a concrete deliverable (skip vague items \
                 like \"think about it\"). \"who_owes\" is the sender's email. \
                 \"due\" is the words in the draft that say when, copied exactly \
                 as written (for example \"by Friday\" or \"next week\"), or null \
                 when no time is named. Never work out a date yourself. \
                 If there are none, return {\"commitments\": []}.",
            )),
            ChatMessage::user(format!(
                "{}\n\nReturn JSON only.",
                wrap_untrusted_mail(&mail_block)
            )),
        ],
    };

    let response = match runtime.complete(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled | LlmError::PrivacyBlocked(_)) => return Ok(Vec::new()),
        Err(e) => return Err(format!("commitments LLM error: {e}")),
    };

    let parsed: LlmCommitmentsBatch =
        serde_json::from_str(response.content.trim()).map_err(|e| {
            format!(
                "commitments: LLM returned non-JSON ({e}); raw={}",
                response.content
            )
        })?;

    let primary_recipient = first_email(&draft.to);
    let now = chrono::Utc::now();
    let mut stored = Vec::new();
    for entry in parsed.commitments {
        let what = entry.what.trim();
        if what.is_empty() {
            continue;
        }
        let direction = match entry.direction.as_deref() {
            Some("theirs") => CommitmentDirection::Theirs,
            _ => CommitmentDirection::Yours,
        };
        let by_when = match entry.due.as_deref() {
            Some(due) => due_words_in_zone(state, &cleaned, due, Some(now), None)
                .map_err(|e| e.to_string())?
                .and_then(|words| words.unambiguous_at()),
            None => None,
        };
        let candidate = DraftCommitmentCandidate {
            id: new_candidate_id(),
            draft_id: draft.id.clone(),
            account_id: draft.account_id.clone(),
            email: primary_recipient.clone(),
            direction,
            who_owes: if entry.who_owes.trim().is_empty() {
                primary_recipient.clone()
            } else {
                entry.who_owes
            },
            what: what.to_string(),
            by_when,
            by_when_source: by_when.map(|_| mxr_store::ByWhenSource::Words),
            extracted_at: now,
        };
        state
            .store
            .upsert_draft_commitment_candidate(&candidate)
            .await
            .map_err(|e| e.to_string())?;
        stored.push(candidate);
    }
    Ok(stored)
}

fn first_email(addrs: &[mxr_core::types::Address]) -> String {
    addrs.first().map(|a| a.email.clone()).unwrap_or_default()
}

pub(crate) async fn extract_request(state: &AppState, draft: &Draft) -> super::HandlerResult {
    let candidates = extract_and_store(state, draft).await?;
    Ok(mxr_protocol::ResponseData::DraftCommitments {
        candidates: candidates
            .into_iter()
            .map(|c| mxr_protocol::DraftCommitmentCandidateData {
                id: c.id,
                draft_id: c.draft_id,
                account_id: c.account_id,
                email: c.email,
                direction: match c.direction {
                    CommitmentDirection::Yours => mxr_protocol::CommitmentDirectionData::Yours,
                    CommitmentDirection::Theirs => mxr_protocol::CommitmentDirectionData::Theirs,
                },
                who_owes: c.who_owes,
                what: c.what,
                by_when: c.by_when,
                extracted_at: c.extracted_at,
            })
            .collect(),
    })
}

pub(crate) async fn promote_after_send(
    state: &AppState,
    draft: &Draft,
    sent_message_id: &mxr_core::MessageId,
) -> Result<usize, String> {
    let candidates = state
        .store
        .list_draft_commitment_candidates(&draft.id)
        .await
        .map_err(|e| e.to_string())?;

    let thread_id = match state.store.get_envelope(sent_message_id).await {
        Ok(Some(env)) => env.thread_id,
        _ => mxr_core::ThreadId::from_uuid(*sent_message_id.as_uuid()),
    };

    let mut promoted = 0usize;
    for c in &candidates {
        let record = mxr_store::ContactCommitmentRecord {
            id: format!("{}::{}", sent_message_id, c.id),
            account_id: c.account_id.clone(),
            email: c.email.clone(),
            thread_id: thread_id.clone(),
            direction: c.direction,
            status: mxr_store::CommitmentStatus::Open,
            who_owes: c.who_owes.clone(),
            what: c.what.clone(),
            by_when: c.by_when,
            by_when_source: c.by_when.and(c.by_when_source),
            evidence_msg_id: sent_message_id.clone(),
            extracted_at: c.extracted_at,
            resolved_at: None,
        };
        state
            .store
            .upsert_contact_commitment(&record)
            .await
            .map_err(|e| e.to_string())?;
        promoted += 1;
    }
    let _ = state
        .store
        .delete_draft_commitment_candidates(&draft.id)
        .await;
    Ok(promoted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::{Address, DraftContent, DraftIntent};
    use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
    use std::sync::{Arc, Mutex};

    struct CannedLlm {
        body: String,
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CannedLlm {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.calls.lock().unwrap() += 1;
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

    fn draft_with_body(account_id: &mxr_core::AccountId, body: &str) -> Draft {
        Draft {
            revision: Some(1),
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "alice@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: deck".into(),
            content: DraftContent::markdown(body),
            inline_assets: Vec::new(),
            managed_attachments: None,
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    async fn fixture(llm_body: &str) -> (Arc<AppState>, mxr_core::AccountId, Arc<CannedLlm>) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let stub = Arc::new(CannedLlm {
            body: llm_body.to_string(),
            calls: Mutex::new(0),
        });
        state.llm.replace(stub.clone());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        (state, account_id, stub)
    }

    #[tokio::test]
    async fn prompt_wraps_draft_and_system_carries_guard() {
        struct Capture {
            msgs: Mutex<Vec<mxr_llm::ChatMessage>>,
        }
        #[async_trait::async_trait]
        impl LlmProvider for Capture {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                *self.msgs.lock().unwrap() = req.messages.clone();
                Ok(CompletionResponse {
                    content: r#"{"commitments":[]}"#.into(),
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

        let (state, account_id, _) = fixture(r#"{"commitments":[]}"#).await;
        let cap = Arc::new(Capture {
            msgs: Mutex::new(vec![]),
        });
        state.llm.replace(cap.clone());
        let draft = draft_with_body(&account_id, "I'll send the DRAFT-MARKER deck Friday.");
        extract_and_store(&state, &draft).await.unwrap();

        let msgs = cap.msgs.lock().unwrap();
        assert!(
            msgs[0].content.contains(mxr_llm::UNTRUSTED_MAIL_GUARD),
            "system prompt must carry the shared injection guard"
        );
        let user = &msgs[1].content;
        let begin = user
            .find(mxr_llm::UNTRUSTED_MAIL_BEGIN)
            .expect("begin marker present");
        let end = user
            .find(mxr_llm::UNTRUSTED_MAIL_END)
            .expect("end marker present");
        let body = user.find("DRAFT-MARKER").expect("draft body present");
        assert!(
            begin < body && body < end,
            "draft body must sit between the untrusted-content markers"
        );
    }

    #[tokio::test]
    async fn extract_persists_candidates_returned_by_llm() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","due":null,"direction":"yours"}
        ]}"#;
        let (state, account_id, stub) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll send the deck Friday. Thanks!");
        let stored = extract_and_store(&state, &draft).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(*stub.calls.lock().unwrap(), 1);
        let listed = state
            .store
            .list_draft_commitment_candidates(&draft.id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].what, "send the deck");
    }

    #[tokio::test]
    async fn due_words_from_the_draft_become_the_deadline_through_promotion() {
        use chrono::Local;
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","due":"tomorrow","direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll send the deck tomorrow. Thanks!");
        let started = chrono::Utc::now();
        let stored = extract_and_store(&state, &draft).await.unwrap();
        let due = stored[0].by_when.expect("dated from the draft's words");
        let expected = mxr_core::natural_time::resolve_time(
            "tomorrow",
            &started.with_timezone(&Local),
            &state.snooze_time_prefs(),
        )
        .unwrap()
        .at;
        assert_eq!(due, expected);

        promote_after_send(&state, &draft, &mxr_core::MessageId::new())
            .await
            .unwrap();
        let promoted = state
            .store
            .list_contact_commitments(&account_id, Some("alice@example.com"), None)
            .await
            .unwrap();
        assert_eq!(promoted[0].by_when, Some(due));
        assert_eq!(
            promoted[0].by_when_source,
            Some(mxr_store::ByWhenSource::Words)
        );
    }

    #[tokio::test]
    async fn due_words_with_more_than_one_reading_leave_the_draft_commitment_undated() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","due":"fri 3","direction":"yours"},
            {"who_owes":"me@example.com","what":"send the notes","due":"one week after Friday","direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(
            &account_id,
            "I'll send the deck fri 3, and I'll send the notes one week after Friday.",
        );
        let stored = extract_and_store(&state, &draft).await.unwrap();
        assert_eq!(stored.len(), 2, "the commitments are kept");
        assert!(stored.iter().all(|candidate| candidate.by_when.is_none()));

        promote_after_send(&state, &draft, &mxr_core::MessageId::new())
            .await
            .unwrap();
        let promoted = state
            .store
            .list_contact_commitments(&account_id, Some("alice@example.com"), None)
            .await
            .unwrap();
        assert!(promoted
            .iter()
            .all(|row| row.by_when.is_none() && row.by_when_source.is_none()));
    }

    #[tokio::test]
    async fn promotion_after_send_writes_exactly_one_contact_commitment_row() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","due":null,"direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll send the deck Friday.");
        extract_and_store(&state, &draft).await.unwrap();

        let sent_id = mxr_core::MessageId::new();
        let n = promote_after_send(&state, &draft, &sent_id).await.unwrap();
        assert_eq!(n, 1);

        let leftover = state
            .store
            .list_draft_commitment_candidates(&draft.id)
            .await
            .unwrap();
        assert!(leftover.is_empty());

        let promoted = state
            .store
            .list_contact_commitments(
                &account_id,
                Some("alice@example.com"),
                Some(mxr_store::CommitmentStatus::Open),
            )
            .await
            .unwrap();
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].what, "send the deck");
    }

    #[tokio::test]
    async fn promotion_is_idempotent_when_run_twice() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","due":null,"direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll send the deck Friday.");
        extract_and_store(&state, &draft).await.unwrap();

        let sent_id = mxr_core::MessageId::new();
        promote_after_send(&state, &draft, &sent_id).await.unwrap();
        extract_and_store(&state, &draft).await.unwrap();
        promote_after_send(&state, &draft, &sent_id).await.unwrap();

        let promoted = state
            .store
            .list_contact_commitments(
                &account_id,
                Some("alice@example.com"),
                Some(mxr_store::CommitmentStatus::Open),
            )
            .await
            .unwrap();
        assert_eq!(promoted.len(), 1, "idempotent resend must not duplicate");
    }
}
