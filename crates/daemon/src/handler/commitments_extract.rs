//! Slice 2.1 of docs/reference/ai-email.md
//!
//! Extracts commitment candidates from an outgoing draft. Combines a
//! cheap regex prefilter (so we don't bother the LLM for drafts with
//! no commitment markers at all) with an `LlmFeature::Commitments`
//! call. Stores results in `draft_commitment_candidates`; promotion
//! to `contact_commitments` happens after a successful send.

use crate::state::AppState;
use mxr_core::types::Draft;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{new_candidate_id, CommitmentDirection, DraftCommitmentCandidate};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;

/// Cheap prefilter — strict word-boundary forms of "I'll", "I will",
/// "I can", "I'll send", "I'll follow up", "I'll get back". Matches
/// against reader-cleaned text so quoted material does not trigger.
static COMMITMENT_PREFILTER: Lazy<Regex> = Lazy::new(|| {
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
    by_when: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    direction: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LlmCommitmentsBatch {
    #[serde(default)]
    commitments: Vec<LlmCommitment>,
}

/// Extract commitment candidates from `draft`, persist them in the
/// `draft_commitment_candidates` table, and return the persisted
/// records. If no prefilter triggers, returns an empty vector and
/// makes no LLM call. If the LLM is disabled, returns an empty
/// vector — degradation is silent (vs answer-coverage which
/// surfaces a warning, since commitments are an enhancement).
pub(crate) async fn extract_and_store(
    state: &AppState,
    draft: &Draft,
) -> Result<Vec<DraftCommitmentCandidate>, String> {
    let cleaned = clean(Some(&draft.body_markdown), None, &ReaderConfig::default()).content;
    if !COMMITMENT_PREFILTER.is_match(&cleaned) {
        return Ok(Vec::new());
    }

    let runtime = state.llm.for_feature(LlmFeature::Commitments);
    let req = CompletionRequest {
        max_tokens: Some(600),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(
                "You extract concrete future-tense commitments the SENDER is making. \
                 Output STRICT JSON with the schema: \
                 {\"commitments\": [{\"who_owes\": str, \"what\": str, \
                 \"by_when\": str|null, \"direction\": \"yours\"|\"theirs\"}]}\n\n\
                 Rules: \"what\" must name a concrete deliverable (skip vague items \
                 like \"think about it\"). \"who_owes\" is the sender's email. \
                 \"by_when\" is RFC 3339 if a date is named, else null. \
                 If there are none, return {\"commitments\": []}.",
            ),
            ChatMessage::user(format!(
                "FROM: {}\nTO: {}\n\nDRAFT:\n{cleaned}\n\nReturn JSON only.",
                first_email(&draft.to),
                draft
                    .to
                    .iter()
                    .map(|a| a.email.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
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
            by_when: entry.by_when,
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

/// IPC dispatch entry point for `Request::ExtractDraftCommitments`.
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

/// Promote any draft-scoped candidates for `draft` to
/// `contact_commitments`, then delete the draft rows. Idempotent:
/// the destination's UNIQUE constraint absorbs duplicate promotions.
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
    use mxr_core::types::{Address, DraftIntent};
    use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
    use std::sync::{Arc, Mutex};

    #[test]
    fn prefilter_matches_ill_send_friday() {
        assert!(COMMITMENT_PREFILTER.is_match("I'll send the deck Friday."));
        assert!(COMMITMENT_PREFILTER.is_match("Hey \u{2014} I will follow up."));
        assert!(COMMITMENT_PREFILTER.is_match("I can have it by Tuesday."));
    }

    #[test]
    fn prefilter_rejects_bare_acknowledgement() {
        assert!(!COMMITMENT_PREFILTER.is_match("Thanks!"));
        assert!(!COMMITMENT_PREFILTER.is_match("Got it, looks good."));
        // "I'll" inside another word should not trigger.
        assert!(!COMMITMENT_PREFILTER.is_match("Bill is great"));
    }

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
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "alice@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: deck".into(),
            body_markdown: body.into(),
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
    async fn extract_skips_llm_when_prefilter_does_not_match() {
        let (state, account_id, stub) = fixture(r#"{"commitments":[]}"#).await;
        let draft = draft_with_body(&account_id, "Thanks for the deck. Looks good.");
        let stored = extract_and_store(&state, &draft).await.unwrap();
        assert!(stored.is_empty());
        assert_eq!(
            *stub.calls.lock().unwrap(),
            0,
            "LLM must not be called when prefilter fails"
        );
    }

    #[tokio::test]
    async fn extract_persists_candidates_returned_by_llm() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","by_when":null,"direction":"yours"}
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
    async fn extract_drops_entries_with_empty_what() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"","by_when":null,"direction":"yours"},
            {"who_owes":"me@example.com","what":"send invoice","by_when":null,"direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll get back to you tomorrow.");
        let stored = extract_and_store(&state, &draft).await.unwrap();
        assert_eq!(stored.len(), 1, "empty 'what' must be filtered out");
        assert_eq!(stored[0].what, "send invoice");
    }

    #[tokio::test]
    async fn promotion_after_send_writes_exactly_one_contact_commitment_row() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","by_when":null,"direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll send the deck Friday.");
        extract_and_store(&state, &draft).await.unwrap();

        // Pretend the synthetic Sent envelope has this id; we don't
        // actually need it to exist for the promote path because
        // promote_after_send falls back to deriving a thread id from
        // the message id when the envelope is missing.
        let sent_id = mxr_core::MessageId::new();
        let n = promote_after_send(&state, &draft, &sent_id).await.unwrap();
        assert_eq!(n, 1);

        // Draft-scoped candidates were cleared on promotion.
        let leftover = state
            .store
            .list_draft_commitment_candidates(&draft.id)
            .await
            .unwrap();
        assert!(leftover.is_empty());

        // contact_commitments has exactly one row for the recipient.
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
            {"who_owes":"me@example.com","what":"send the deck","by_when":null,"direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(&account_id, "I'll send the deck Friday.");
        extract_and_store(&state, &draft).await.unwrap();

        let sent_id = mxr_core::MessageId::new();
        promote_after_send(&state, &draft, &sent_id).await.unwrap();
        // Re-extract (simulating a retry) and promote again with the
        // same sent id — UNIQUE constraint must absorb the duplicate.
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
