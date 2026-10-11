//! `DetectPromises` and `RecordPromise`: catching "I'll send the deck by
//! Friday" in mail you send, and keeping it as a dated commitment when you
//! say so.
//!
//! Detection reads only the outgoing text and its recipients, under the
//! `Commitments` feature's privacy policy, never relationship history. The
//! model names the deliverable and copies the due words; the words are
//! checked against the text and resolved by the natural-time parser in the
//! caller's zone, so the date shown is the parser's, not the model's.
//! Detection has a short budget and never fails a send: a missing, blocked
//! or slow model is a status. Nothing is stored until `RecordPromise`.

use super::commitments_extract::COMMITMENT_PREFILTER;
use super::relationship_profile::commitment_data;
use super::thread_context::owned_addresses;
use super::thread_gist::{collapse_whitespace, find_phrase_ignoring_case, json_object, plain_text};
use super::time::{parse_zone, resolve_in_zone};
use super::{HandlerError, HandlerResult};
use crate::state::{llm_endpoint_is_local, AppState};
use chrono::{DateTime, Utc};
use mxr_core::id::MessageId;
use mxr_core::natural_time::TimeResolution;
use mxr_core::types::{Address, MessageFlags};
use mxr_llm::{
    guarded_system_prompt, wrap_untrusted_mail, ChatMessage, CompletionRequest, LlmError,
    LlmFeature,
};
use mxr_protocol::{
    AiLocalityData, AiProvenanceData, AiSourceData, DetectedPromiseData, PromiseDetectionData,
    PromiseDetectionStatusData, PromiseSourceData, ResponseData,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{new_candidate_id, CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};
use serde::Deserialize;
use std::time::Duration;

const FEATURE: LlmFeature = LlmFeature::Commitments;
/// Long enough for a local model on a short email, short enough that the
/// web's undo window (10s by default) usually still covers the answer.
const DETECT_BUDGET: Duration = Duration::from_secs(8);
const TEXT_MAX_CHARS: usize = 8_000;
const WHAT_MAX_CHARS: usize = 120;
const DUE_MAX_CHARS: usize = 60;
const MAX_PROMISES: usize = 5;
/// Words in front of a due phrase that the time parser doesn't read.
const DUE_FILLERS: &[&str] = &[
    "by", "on", "before", "until", "till", "no", "later", "than", "at", "the", "this", "for",
    "sometime", "around", "within",
];

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

/// The outgoing text as the model will see it.
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
    // An unknown zone is the caller's mistake: say so before any model work.
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

/// The envelope of a message the account itself sent: promises are only
/// ever yours.
async fn sent_envelope(
    state: &AppState,
    message_id: &MessageId,
) -> Result<mxr_core::types::Envelope, HandlerError> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or_else(|| format!("Message not found: {message_id}"))?;
    // Sent means filed as sent: the provider's Sent label or folder, or
    // mxr's own record of the send (both set MessageFlags::SENT), and from
    // an address of this account. A From header alone is only a claim.
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
    // Most mail makes no promise at all; don't ask a model about it.
    if !COMMITMENT_PREFILTER.is_match(&outgoing.body) {
        return Ok(answer(PromiseDetectionStatusData::Ready, Vec::new(), None));
    }
    // Pin once: the provenance label and the call follow the same endpoint.
    let llm = state.llm.for_feature(FEATURE).pin();
    let locality = if llm_endpoint_is_local(llm.base_url()) {
        AiLocalityData::Local
    } else {
        AiLocalityData::Cloud
    };
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
        // Only what the message says is offered, in its own words: an
        // invented deliverable is dropped even when its date is real.
        let Some(what) = verified(&outgoing.body, &raw.what, WHAT_MAX_CHARS) else {
            continue;
        };
        if promises
            .iter()
            .any(|p: &DetectedPromiseData| p.what.eq_ignore_ascii_case(&what))
        {
            continue;
        }
        let due_phrase = raw
            .due
            .and_then(|due| verified(&outgoing.body, &due, DUE_MAX_CHARS));
        let due = match due_phrase.as_deref() {
            Some(phrase) => resolve_due(state, phrase, now, time_zone)?,
            None => None,
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
            model,
            locality,
            sources: vec![AiSourceData::YourMessage],
        }),
    ))
}

/// Model text found in the message, as the message writes it (whitespace
/// collapsed for display), or `None` when it isn't there or is too long.
fn verified(body: &str, model_text: &str, max_chars: usize) -> Option<String> {
    let wanted = model_text.trim().trim_end_matches(['.', ',', ';', '!']);
    let slice = find_phrase_ignoring_case(body, wanted)?;
    let text = collapse_whitespace(slice);
    (text.chars().count() <= max_chars).then_some(text)
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

/// Resolve the due words with the same parser every time field uses. People
/// write "by Friday" or "first thing Monday", so leading words the parser
/// doesn't read are dropped until what's left resolves.
fn resolve_due(
    state: &AppState,
    phrase: &str,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> Result<Option<TimeResolution>, HandlerError> {
    let words: Vec<&str> = phrase
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| matches!(c, ',' | '.' | '!' | '?' | ';')))
        .filter(|word| !word.is_empty())
        .collect();
    let first_kept = words
        .iter()
        .position(|word| !DUE_FILLERS.contains(&word.to_ascii_lowercase().as_str()))
        .unwrap_or(words.len());
    // "within 2 days" reads as "in 2 days".
    let within = first_kept > 0 && words[first_kept - 1].eq_ignore_ascii_case("within");
    for start in first_kept..words.len() {
        let mut candidate = words[start..].join(" ");
        if within && start == first_kept {
            candidate = format!("in {candidate}");
        }
        if let Ok(resolution) = resolve_in_zone(state, &candidate, now, time_zone)? {
            return Ok(Some(resolution));
        }
    }
    Ok(None)
}

pub(super) async fn record_promise(
    state: &AppState,
    message_id: &MessageId,
    what: &str,
    due_at: DateTime<Utc>,
    dry_run: bool,
) -> HandlerResult {
    let what = plain_text(what, WHAT_MAX_CHARS);
    if what.is_empty() {
        return Err("Say what you promised, such as \"send the deck\".".into());
    }
    let envelope = sent_envelope(state, message_id).await?;
    let Some(recipient) = envelope.to.first().or_else(|| envelope.cc.first()) else {
        return Err(format!("Message {message_id} has no recipient to keep a promise to").into());
    };
    let mut record = ContactCommitmentRecord {
        id: format!("{message_id}::promise::{}", new_candidate_id()),
        account_id: envelope.account_id.clone(),
        email: recipient.email.clone(),
        thread_id: envelope.thread_id.clone(),
        direction: CommitmentDirection::Yours,
        status: CommitmentStatus::Open,
        who_owes: envelope.from.email.clone(),
        what,
        by_when: Some(due_at),
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

    /// Tuesday 29 September 2026, 14:00 UTC (15:00 in London).
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
        // Friday 2 October at the morning hour, 09:00 London (BST).
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
    async fn due_words_the_email_never_says_are_dropped() {
        let (state, account_id, _) = fixture(
            r#"{"promises":[{"what":"send the deck","due":"by Monday"}]}"#,
            Duration::ZERO,
        )
        .await;
        let detection = run(&state, &account_id, "I'll send the deck soon.").await;
        assert_eq!(detection.promises.len(), 1);
        assert!(detection.promises[0].due_phrase.is_none());
        assert!(detection.promises[0].due.is_none());
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
        // The message's own words, not the model's spelling.
        assert_eq!(promise.what, "send the \"deck\"");
        assert_eq!(promise.due_phrase.as_deref(), Some("by Friday"));
        assert!(promise.due.is_some());
    }

    #[tokio::test]
    async fn mail_without_a_promise_marker_never_reaches_the_model() {
        let (state, account_id, stub) = fixture(r#"{"promises":[]}"#, Duration::ZERO).await;
        let detection = run(&state, &account_id, "Thanks, looks good to me.").await;
        assert_eq!(detection.status, PromiseDetectionStatusData::Ready);
        assert!(detection.promises.is_empty());
        assert!(detection.provenance.is_none());
        assert!(stub.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_slow_model_times_out_as_a_status_not_an_error() {
        let (state, _, _) = fixture(
            r#"{"promises":[{"what":"send the deck","due":"Friday"}]}"#,
            Duration::from_secs(30),
        )
        .await;
        let outgoing = OutgoingText {
            recipients: Vec::new(),
            body: "I'll send the deck Friday.".into(),
        };
        let started = std::time::Instant::now();
        let detection = detect(
            &state,
            &outgoing,
            Some(anchor()),
            None,
            Duration::from_millis(50),
        )
        .await
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(detection.status, PromiseDetectionStatusData::TimedOut);
        assert!(detection.promises.is_empty());
        assert!(detection.message.is_some());
    }

    #[tokio::test]
    async fn an_unusable_answer_is_a_failed_status() {
        let (state, account_id, _) = fixture("I can't do that.", Duration::ZERO).await;
        let detection = run(&state, &account_id, "I'll send the deck Friday.").await;
        assert_eq!(detection.status, PromiseDetectionStatusData::Failed);
    }

    #[tokio::test]
    async fn an_unknown_zone_is_refused() {
        let (state, account_id, _) = fixture(r#"{"promises":[]}"#, Duration::ZERO).await;
        let source = PromiseSourceData::Draft {
            draft: Box::new(draft(&account_id, "I'll send it.")),
        };
        let error = detect_promises(&state, &source, None, Some("Mars/Olympus"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("Unknown time zone"));
    }

    #[tokio::test]
    async fn due_words_drop_leading_fillers_until_they_resolve() {
        let (state, _, _) = fixture(r#"{"promises":[]}"#, Duration::ZERO).await;
        let at = |phrase: &str| {
            resolve_due(&state, phrase, Some(anchor()), Some("Europe/London"))
                .unwrap()
                .map(|resolution| resolution.at)
        };
        let friday_nine = Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap();
        assert_eq!(at("by Friday"), Some(friday_nine));
        assert_eq!(at("no later than friday."), Some(friday_nine));
        assert_eq!(at("first thing Friday"), Some(friday_nine));
        assert_eq!(
            at("within 2 days"),
            Some(Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap())
        );
        assert_eq!(at("at some point"), None);
    }

    async fn sync(state: &AppState) {
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
    }

    /// A message the account sent, stored like a synced Sent item.
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
        // Inbound mail whose From header claims to be you: not filed as sent.
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

    #[tokio::test]
    async fn only_mail_you_sent_holds_your_promises() {
        let (state, account_id, _) = fixture(r#"{"promises":[]}"#, Duration::ZERO).await;
        sync(&state).await;
        let inbound = state
            .store
            .list_envelopes_by_account(&account_id, 50, 0)
            .await
            .unwrap()
            .into_iter()
            .find(|envelope| envelope.from.email != "user@example.com")
            .expect("fake mail has inbound messages");
        let friday = Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap();
        let error = record_promise(&state, &inbound.id, "send the deck", friday, false)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not sent from this account"));
    }
}
