use super::commitments_extract::COMMITMENT_PREFILTER;
use super::relationship_profile::commitment_data;
use super::thread_context::owned_addresses;
use super::thread_gist::{json_object, plain_text};
use super::time::{due_words_in_zone, parse_zone};
use super::{HandlerError, HandlerResult};
use crate::state::{llm_answer_locality, AppState};
use chrono::{DateTime, Utc};
use mxr_core::id::MessageId;
use mxr_core::text::find_verbatim;
use mxr_core::types::{Address, MessageFlags};
use mxr_llm::{
    guarded_system_prompt, wrap_untrusted_mail, ChatMessage, CompletionRequest, LlmError,
    LlmFeature,
};
use mxr_protocol::{
    AiProvenanceData, AiSourceData, DetectedPromiseData, PromiseDetectionData,
    PromiseDetectionStatusData, PromiseSourceData, ResponseData,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{
    new_candidate_id, ByWhenSource, CommitmentDirection, CommitmentStatus, ContactCommitmentRecord,
};
use serde::Deserialize;
use std::time::Duration;

const FEATURE: LlmFeature = LlmFeature::Commitments;
const DETECT_BUDGET: Duration = Duration::from_secs(8);
const TEXT_MAX_CHARS: usize = 8_000;
const WHAT_MAX_CHARS: usize = 120;
const MAX_PROMISES: usize = 5;

pub(crate) const SYSTEM_PROMPT: &str = r#"You find promises the SENDER of an email makes: things the sender says they will do.

Output STRICT JSON and nothing else:
{"promises": [{"what": string, "due": string or null}]}

what: the deliverable copied exactly as the email words it, starting with the verb, at most 100 characters: from "I'll send you the deck by Friday" it is "send you the deck". Skip vague intentions ("think about it", "keep you posted"), things already done, and anything someone else promises.
due: the words in the email that say when, copied exactly as written, for example "by Friday", "tomorrow", "next week", "on 3 October". Use null when no time is named.
If the sender promises nothing, return {"promises": []}."#;

#[derive(Debug, Deserialize)]
struct RawPromises {
    #[serde(default)]
    promises: Vec<RawPromise>,
}

#[derive(Debug, Deserialize)]
struct RawPromise {
    #[serde(default)]
    what: String,
    #[serde(default)]
    due: Option<String>,
}

struct OutgoingText {
    recipients: Vec<Address>,
    body: String,
}

pub(super) async fn detect_promises(
    state: &AppState,
    source: &PromiseSourceData,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> HandlerResult {
    parse_zone(time_zone)?;
    let outgoing = outgoing_text(state, source).await?;
    let detection = detect(state, &outgoing, now, time_zone, DETECT_BUDGET).await?;
    Ok(ResponseData::Promises { detection })
}

async fn outgoing_text(
    state: &AppState,
    source: &PromiseSourceData,
) -> Result<OutgoingText, HandlerError> {
    match source {
        PromiseSourceData::Draft { draft } => {
            let (text, html) = draft.content.reader_input();
            Ok(OutgoingText {
                recipients: draft.to.iter().chain(&draft.cc).cloned().collect(),
                body: clean(text, html, &ReaderConfig::default()).content,
            })
        }
        PromiseSourceData::SentMessage { message_id } => {
            let envelope = sent_envelope(state, message_id).await?;
            let body = state.store.get_body(message_id).await?;
            let body = body
                .map(|body| {
                    clean(
                        body.text_plain.as_deref(),
                        body.text_html.as_deref(),
                        &ReaderConfig::default(),
                    )
                    .content
                })
                .unwrap_or_default();
            Ok(OutgoingText {
                recipients: envelope.to.iter().chain(&envelope.cc).cloned().collect(),
                body,
            })
        }
    }
}

async fn sent_envelope(
    state: &AppState,
    message_id: &MessageId,
) -> Result<mxr_core::types::Envelope, HandlerError> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or_else(|| format!("Message not found: {message_id}"))?;
    let owned = owned_addresses(state, &envelope.account_id).await?;
    if !envelope.flags.contains(MessageFlags::SENT)
        || !owned.contains(&envelope.from.email.to_ascii_lowercase())
    {
        return Err(format!(
            "Message {message_id} was not sent from this account, so it holds no promises of yours"
        )
        .into());
    }
    Ok(envelope)
}

async fn detect(
    state: &AppState,
    outgoing: &OutgoingText,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
    budget: Duration,
) -> Result<PromiseDetectionData, HandlerError> {
    if !COMMITMENT_PREFILTER.is_match(&outgoing.body) {
        return Ok(answer(PromiseDetectionStatusData::Ready, Vec::new(), None));
    }
    let llm = state.llm.for_feature(FEATURE).pin();
    let mut body = outgoing.body.clone();
    mxr_core::text::truncate_to_char_boundary(&mut body, TEXT_MAX_CHARS);
    let recipients = outgoing
        .recipients
        .iter()
        .map(|address| address.email.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(guarded_system_prompt(SYSTEM_PROMPT)),
            ChatMessage::user(format!(
                "{}\n\nReturn JSON only.",
                wrap_untrusted_mail(&format!("TO: {recipients}\n\nEMAIL:\n{body}"))
            )),
        ],
        max_tokens: Some(400),
        temperature: Some(0.0),
    };
    let response = match tokio::time::timeout(budget, llm.complete(request)).await {
        Err(_) => {
            let seconds = budget.as_secs();
            return Ok(unavailable(
                PromiseDetectionStatusData::TimedOut,
                &format!("The model took longer than {seconds} seconds, so there is no promise check this time."),
            ));
        }
        Ok(Err(LlmError::Disabled)) => {
            return Ok(unavailable(
                PromiseDetectionStatusData::Disabled,
                "No language model is configured.",
            ))
        }
        Ok(Err(LlmError::PrivacyBlocked(reason))) => {
            return Ok(unavailable(PromiseDetectionStatusData::Blocked, &reason))
        }
        Ok(Err(error)) => {
            return Ok(unavailable(
                PromiseDetectionStatusData::Failed,
                &error.to_string(),
            ))
        }
        Ok(Ok(response)) => response,
    };
    let Ok(raw) = serde_json::from_str::<RawPromises>(json_object(&response.content)) else {
        tracing::warn!("promise detection: model answer was not JSON");
        return Ok(unavailable(
            PromiseDetectionStatusData::Failed,
            "The model's answer wasn't usable.",
        ));
    };
    let mut promises = Vec::new();
    for raw in raw.promises {
        let Some(what) = find_verbatim(&outgoing.body, &raw.what, WHAT_MAX_CHARS) else {
            continue;
        };
        if promises
            .iter()
            .any(|p: &DetectedPromiseData| p.what.eq_ignore_ascii_case(&what))
        {
            continue;
        }
        let due_words = match raw.due.as_deref() {
            Some(due) => due_words_in_zone(state, &outgoing.body, due, now, time_zone)?,
            None => None,
        };
        let (due_phrase, due) = match due_words {
            Some(words) => (Some(words.phrase), words.resolution),
            None => (None, None),
        };
        promises.push(DetectedPromiseData {
            what,
            due_phrase,
            due,
        });
        if promises.len() == MAX_PROMISES {
            break;
        }
    }
    let model = if response.model.trim().is_empty() {
        llm.model_name().to_string()
    } else {
        response.model
    };
    Ok(answer(
        PromiseDetectionStatusData::Ready,
        promises,
        Some(AiProvenanceData {
            locality: llm_answer_locality(&llm, &model),
            model,
            sources: vec![AiSourceData::YourMessage],
        }),
    ))
}

fn answer(
    status: PromiseDetectionStatusData,
    promises: Vec<DetectedPromiseData>,
    provenance: Option<AiProvenanceData>,
) -> PromiseDetectionData {
    PromiseDetectionData {
        status,
        promises,
        provenance,
        message: None,
    }
}

fn unavailable(status: PromiseDetectionStatusData, message: &str) -> PromiseDetectionData {
    PromiseDetectionData {
        message: Some(message.to_string()),
        ..answer(status, Vec::new(), None)
    }
}

pub(super) async fn record_promise(
    state: &AppState,
    message_id: &MessageId,
    what: &str,
    due_at: DateTime<Utc>,
    dry_run: bool,
) -> HandlerResult {
    let supplied = what.trim();
    let envelope = sent_envelope(state, message_id).await?;
    let answer = super::ask_assessments::promise_source(state, &envelope).await?;
    let source_safe = supplied.chars().count() <= WHAT_MAX_CHARS
        && !supplied
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'));
    let canonical_match = source_safe
        && !supplied.is_empty()
        && answer.as_ref().is_some_and(|answer| {
            !answer.pending_body
                && answer.units.iter().any(|unit| {
                    unit.answer == mxr_core::asks::AskAnswer::YouPromised
                        && answer
                            .quote(unit)
                            .is_some_and(|quote| quote.contains(supplied))
                })
        });
    let source_match = if canonical_match {
        true
    } else if source_safe && !supplied.is_empty() {
        state
            .store
            .get_body(message_id)
            .await?
            .and_then(|body| body.text_plain)
            .is_some_and(|text| text.contains(supplied))
    } else {
        false
    };
    let mut what = if source_match {
        supplied.to_string()
    } else {
        plain_text(supplied, WHAT_MAX_CHARS)
    };
    if what.is_empty() {
        return Err("Say what you promised, such as \"send the deck\".".into());
    }
    if let Some(answer) = answer.as_ref().filter(|answer| !answer.pending_body) {
        let quotes: Vec<_> = answer
            .units
            .iter()
            .filter(|unit| unit.answer == mxr_core::asks::AskAnswer::YouPromised)
            .filter_map(|unit| answer.quote(unit))
            .filter(|quote| quote.contains(&what))
            .collect();
        if !quotes.contains(&what.as_str()) {
            if let Some(quote) = quotes.first() {
                if quotes.iter().any(|other| other != quote) {
                    return Err("Those words match more than one promise. Use the exact promise from the sent email.".into());
                }
                what = (*quote).to_string();
            }
        }
    }
    let owned = state.owned_addresses(&envelope.account_id).await?;
    let Some(recipient) =
        mxr_core::first_non_owned_recipient(&envelope.to, &envelope.cc, |email| {
            owned.contains(email)
        })
    else {
        return Err(format!("Message {message_id} has no recipient to keep a promise to").into());
    };
    let mut record = ContactCommitmentRecord {
        id: format!("{message_id}::promise::{}", new_candidate_id()),
        account_id: envelope.account_id.clone(),
        email: recipient.email.to_lowercase(),
        thread_id: envelope.thread_id.clone(),
        direction: CommitmentDirection::Yours,
        status: CommitmentStatus::Open,
        who_owes: envelope.from.email.clone(),
        what,
        by_when: Some(due_at),
        by_when_source: Some(ByWhenSource::User),
        evidence_msg_id: message_id.clone(),
        extracted_at: Utc::now(),
        resolved_at: None,
    };
    if dry_run {
        record.id.clear();
    } else {
        record.id = state
            .store
            .record_own_promise(&record)
            .await
            .map_err(|error| error.to_string())?;
        if state.config_snapshot().todo.enabled {
            let cfg = super::todos::pass_config(state, Utc::now());
            mxr_todo::pass::project_stored_ask(&state.store, &cfg, message_id)
                .await
                .map_err(|error| error.to_string())?;
            mxr_todo::accepted_promises::sync_commitment(
                &state.store,
                &cfg,
                &envelope.account_id,
                &record.id,
            )
            .await
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(ResponseData::RecordedPromise {
        commitment: commitment_data(record),
        dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mxr_core::id::AccountId;
    use mxr_core::types::{Draft, DraftContent, DraftIntent};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use std::sync::{Arc, Mutex};

    struct CannedLlm {
        body: String,
        delay: Duration,
        calls: Mutex<Vec<CompletionRequest>>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CannedLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.calls.lock().unwrap().push(req);
            tokio::time::sleep(self.delay).await;
            Ok(CompletionResponse {
                content: self.body.clone(),
                model: "stub-7b".into(),
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
            "stub-7b"
        }
    }

    fn anchor() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 29, 14, 0, 0).unwrap()
    }

    async fn fixture(body: &str, delay: Duration) -> (Arc<AppState>, AccountId, Arc<CannedLlm>) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let stub = Arc::new(CannedLlm {
            body: body.to_string(),
            delay,
            calls: Mutex::new(Vec::new()),
        });
        state.llm.replace(stub.clone());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        (state, account_id, stub)
    }

    fn draft(account_id: &AccountId, body: &str) -> Draft {
        Draft {
            revision: Some(1),
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "nora@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: deck".into(),
            content: DraftContent::markdown(body),
            inline_assets: Vec::new(),
            managed_attachments: None,
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    async fn run(state: &AppState, account_id: &AccountId, body: &str) -> PromiseDetectionData {
        let source = PromiseSourceData::Draft {
            draft: Box::new(draft(account_id, body)),
        };
        match detect_promises(state, &source, Some(anchor()), Some("Europe/London"))
            .await
            .unwrap()
        {
            ResponseData::Promises { detection } => detection,
            other => panic!("expected Promises, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_dated_promise_resolves_its_due_words_in_the_callers_zone() {
        let (state, account_id, stub) = fixture(
            r#"```json
{"promises":[{"what":"send the deck","due":"by Friday"},{"what":"loop in Sam","due":null}]}
```"#,
            Duration::ZERO,
        )
        .await;
        let detection = run(
            &state,
            &account_id,
            "Thanks Nora. I'll send the deck by Friday and I will loop in Sam.",
        )
        .await;

        assert_eq!(detection.status, PromiseDetectionStatusData::Ready);
        assert_eq!(detection.promises.len(), 2);
        let deck = &detection.promises[0];
        assert_eq!(deck.what, "send the deck");
        assert_eq!(deck.due_phrase.as_deref(), Some("by Friday"));
        assert_eq!(
            deck.due.as_ref().unwrap().at,
            Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap()
        );
        assert!(detection.promises[1].due.is_none());
        let provenance = detection.provenance.unwrap();
        assert_eq!(provenance.model, "stub-7b");
        assert_eq!(provenance.sources, vec![AiSourceData::YourMessage]);

        let calls = stub.calls.lock().unwrap();
        let user = &calls[0].messages[1].content;
        assert!(user.contains(mxr_llm::UNTRUSTED_MAIL_BEGIN));
        assert!(user.contains("nora@example.com"));
    }

    #[tokio::test]
    async fn an_invented_deliverable_is_dropped_even_with_a_real_date() {
        let (state, account_id, _) = fixture(
            r#"{"promises":[
                {"what":"send the signed contract","due":"by Friday"},
                {"what":"Send   the “deck”","due":"BY FRIDAY"}
            ]}"#,
            Duration::ZERO,
        )
        .await;
        let detection = run(
            &state,
            &account_id,
            "Thanks. I'll send the \"deck\"\nby Friday.",
        )
        .await;
        assert_eq!(
            detection.promises.len(),
            1,
            "the contract was never promised"
        );
        let promise = &detection.promises[0];
        assert_eq!(promise.what, "send the \"deck\"");
        assert_eq!(promise.due_phrase.as_deref(), Some("by Friday"));
        assert!(promise.due.is_some());
    }

    async fn sync(state: &AppState) {
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
    }

    async fn stored_sent_message(state: &AppState, account_id: &AccountId) -> MessageId {
        sync(state).await;
        let owned = owned_addresses(state, account_id).await.unwrap();
        let from = owned
            .iter()
            .next()
            .expect("fake account has an address")
            .clone();
        let mut envelope = state
            .store
            .list_envelopes_by_account(account_id, 1, 0)
            .await
            .unwrap()
            .remove(0);
        envelope.id = MessageId::new();
        envelope.provider_id = "sent-promise".into();
        envelope.from = Address {
            name: None,
            email: from,
        };
        envelope.flags |= MessageFlags::SENT;
        envelope.to = vec![Address {
            name: None,
            email: "nora@example.com".into(),
        }];
        state.store.upsert_envelope(&envelope).await.unwrap();
        envelope.id
    }

    async fn open_promises(
        state: &AppState,
        account_id: &AccountId,
        sent: &MessageId,
    ) -> Vec<ContactCommitmentRecord> {
        state
            .store
            .list_contact_commitments(account_id, None, Some(CommitmentStatus::Open))
            .await
            .unwrap()
            .into_iter()
            .filter(|row| &row.evidence_msg_id == sent)
            .collect()
    }

    #[tokio::test]
    async fn recording_a_promise_keeps_it_open_and_due_and_a_repeat_moves_the_date() {
        let (state, account_id, _) = fixture(r#"{"promises":[]}"#, Duration::ZERO).await;
        let sent = stored_sent_message(&state, &account_id).await;
        let friday = Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap();

        let preview = record_promise(&state, &sent, "send the deck", friday, true)
            .await
            .unwrap();
        let ResponseData::RecordedPromise {
            commitment,
            dry_run: true,
        } = preview
        else {
            panic!("expected a dry-run RecordedPromise");
        };
        assert!(commitment.id.is_empty());
        assert_eq!(commitment.by_when, Some(friday));
        assert!(
            open_promises(&state, &account_id, &sent).await.is_empty(),
            "a dry run writes nothing"
        );

        let ResponseData::RecordedPromise { commitment, .. } =
            record_promise(&state, &sent, "send the deck", friday, false)
                .await
                .unwrap()
        else {
            panic!("expected RecordedPromise");
        };
        let rows = open_promises(&state, &account_id, &sent).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, commitment.id);
        assert_eq!(rows[0].email, "nora@example.com");
        assert_eq!(rows[0].direction, CommitmentDirection::Yours);
        assert_eq!(rows[0].by_when, Some(friday));

        let monday = Utc.with_ymd_and_hms(2026, 10, 5, 8, 0, 0).unwrap();
        let ResponseData::RecordedPromise {
            commitment: again, ..
        } = record_promise(&state, &sent, "send the deck", monday, false)
            .await
            .unwrap()
        else {
            panic!("expected RecordedPromise");
        };
        let rows = open_promises(&state, &account_id, &sent).await;
        assert_eq!(rows.len(), 1, "recording again never duplicates");
        assert_eq!(again.id, commitment.id);
        assert_eq!(rows[0].by_when, Some(monday));
    }

    #[tokio::test]
    async fn a_spoofed_from_is_not_mail_you_sent() {
        let (state, account_id, _) = fixture(r#"{"promises":[]}"#, Duration::ZERO).await;
        let sent = stored_sent_message(&state, &account_id).await;
        let mut spoofed = state.store.get_envelope(&sent).await.unwrap().unwrap();
        spoofed.id = MessageId::new();
        spoofed.provider_id = "spoofed".into();
        spoofed.flags.remove(MessageFlags::SENT);
        state.store.upsert_envelope(&spoofed).await.unwrap();
        let friday = Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap();
        let error = record_promise(&state, &spoofed.id, "send the deck", friday, false)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not sent from this account"));
        let source = PromiseSourceData::SentMessage {
            message_id: spoofed.id.clone(),
        };
        assert!(detect_promises(&state, &source, None, None).await.is_err());
    }
}
