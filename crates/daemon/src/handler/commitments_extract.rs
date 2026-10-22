//! Slice 2.1 of docs/reference/ai-email.md
//!
//! Extracts commitment candidates from an outgoing draft. Combines a
//! cheap regex prefilter (so we don't bother the LLM for drafts with
//! no commitment markers at all) with an `LlmFeature::Commitments`
//! call. Stores results in `draft_commitment_candidates`; promotion
//! to `contact_commitments` happens after a successful send.
//!
//! The model copies the due words; code checks them against the draft and
//! resolves them with the natural-time parser, so a deadline is never a
//! date the model wrote (D111).

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

/// Cheap prefilter — strict word-boundary forms of "I'll", "I will",
/// "I can", "I'll send", "I'll follow up", "I'll get back". Matches
/// against reader-cleaned text so quoted material does not trigger.
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
    /// The draft's own words for when, never a date the model works out.
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
    let cleaned = {
        let (text, html) = draft.content.reader_input();
        clean(text, html, &ReaderConfig::default())
    }
    .content;
    if !COMMITMENT_PREFILTER.is_match(&cleaned) {
        return Ok(Vec::new());
    }

    // The draft body is analyzed as data (it may quote inbound mail, or be
    // AI-drafted from a poisoned thread), and the recipient addresses are
    // mail-derived for replies. Wrap the whole FROM/TO/DRAFT block in
    // untrusted-content delimiters. Strict-JSON parsing is the boundary.
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
        // The draft is being written now, so its "by Friday" means the
        // coming Friday in the daemon's zone.
        let by_when = match entry.due.as_deref() {
            Some(due) => due_words_in_zone(state, &cleaned, due, Some(now), None)
                .map_err(|e| e.to_string())?
                // One reading only: a guess stays undated, not a deadline.
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
            // Dated only by `unambiguous_at` above, from the draft's words.
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
            // The candidate's own provenance: one saved before due words
            // existed has none, and its date stays unverified.
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
        // Prefilter must fire ("I'll send") so the LLM is actually called.
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
        // "tomorrow" has one reading on any day; "by Friday" typed on a
        // Friday would not.
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
        // "fri 3" is 15:00 or 03:00, and "one week after Friday" reads only
        // as "Friday": neither is a deadline to store.
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
    async fn a_due_the_draft_never_says_leaves_the_commitment_undated() {
        // "by Monday" isn't in the draft; the RFC 3339 date is the model
        // working out a date, which the draft doesn't say either.
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"send the deck","due":"by Monday","direction":"yours"},
            {"who_owes":"me@example.com","what":"send the notes","due":"2026-10-09T17:00:00Z","direction":"yours"}
        ]}"#;
        let (state, account_id, _) = fixture(body).await;
        let draft = draft_with_body(
            &account_id,
            "I'll send the deck soon, and I'll send the notes when I can.",
        );
        let stored = extract_and_store(&state, &draft).await.unwrap();
        assert_eq!(stored.len(), 2, "undated commitments are still kept");
        assert!(stored.iter().all(|candidate| candidate.by_when.is_none()));
    }

    #[tokio::test]
    async fn extract_drops_entries_with_empty_what() {
        let body = r#"{"commitments":[
            {"who_owes":"me@example.com","what":"","due":null,"direction":"yours"},
            {"who_owes":"me@example.com","what":"send invoice","due":null,"direction":"yours"}
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
            {"who_owes":"me@example.com","what":"send the deck","due":null,"direction":"yours"}
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
            {"who_owes":"me@example.com","what":"send the deck","due":null,"direction":"yours"}
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
