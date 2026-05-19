//! Thread summarisation. Pulls the full thread + bodies from the store,
//! includes explicit sender/recipient context for each message, and asks
//! the configured LLM for a per-message conversation summary.

use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_core::types::{AccountAddress, Address, Envelope};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature, LlmRuntime};
use mxr_protocol::{ResponseData, ThreadSummaryData};
use mxr_store::{thread_summary_content_hash, Store, ThreadSummaryRecord};
use std::collections::BTreeSet;
use std::sync::Arc;

const SUMMARY_PROMPT_VERSION: &str = "v2";

const SYSTEM_PROMPT: &str = r#"You write thread summaries for a terminal email client. The reader already sees subject, sender, recipient, date, and full thread structure on screen — never restate those.

Output must be PLAIN TEXT. No markdown. No asterisks. No bold. No headers. No emoji. No bullet symbols unless the rules below require them.

Step 1 — Classify silently (do not output the type): newsletter, transactional, personal, reply, notification.

Step 2 — Write the summary by type:

- newsletter: one or two short sentences (≤220 chars total) naming the 2–3 most concrete stories with their key facts (numbers, names, decisions). Lead with the most newsworthy. Do not preface with "this newsletter covers" or similar.
- transactional: one sentence stating amount, date, account/entity, and action ("$24.53 autopay from acct ...1111 on Jul 31"). Skip the preamble.
- personal / reply (single message): one or two sentences stating the actual point — the ask, the answer, the decision, the concrete content. Not "Alice writes about X."
- personal / reply (multi-message thread, 2+ messages with back-and-forth): chronological bullets, one per substantive turn, prefixed with "- ". Each bullet names the speaker (first name or "you") and states what they said or decided. Skip routine acknowledgements. Maximum 6 bullets — collapse older turns if longer.
- notification: one sentence stating the event and the affected entity.

Step 3 — Action line (conditional):

If, and only if, the latest inbound message contains an explicit, unambiguous ask directed at the account owner that has not already been answered, append on its own line:

Action: <one short clause naming what you owe and any deadline>

If there is no such ask, omit the line entirely. Do NOT write "no action needed", "FYI only", or any equivalent.

Rules:
- Say what the email SAYS, not what it IS. Concrete facts beat meta-descriptions.
- Preserve names, numbers, dates, deadlines, decisions verbatim from the source.
- Use "you" only for the account owner addresses listed in the user message.
- Do not invent context. If a message is purely informational, just state the information.
- Do not repeat the subject line, sender name, recipient address, or message date in the summary body."#;

pub(super) async fn summarize_thread(state: &AppState, thread_id: &ThreadId) -> HandlerResult {
    let summary =
        summarize_thread_cached(state.store.clone(), state.llm.clone(), thread_id).await?;
    Ok(ResponseData::ThreadSummary {
        text: summary.text,
        model: summary.model,
    })
}

pub(crate) async fn valid_cached_summary(
    store: &Store,
    thread_id: &ThreadId,
    envelopes: &[Envelope],
) -> Option<ThreadSummaryData> {
    let relationship_hash = match envelopes.first() {
        Some(first) => {
            let owned_addresses = store
                .list_account_addresses(&first.account_id)
                .await
                .unwrap_or_default();
            relationship_context_for_summary(store, &first.account_id, &owned_addresses, envelopes)
                .await
                .1
        }
        None => String::new(),
    };
    let content_hash = format!(
        "{}:{relationship_hash}:{SUMMARY_PROMPT_VERSION}",
        thread_summary_content_hash(envelopes)
    );
    let record = store.get_thread_summary(thread_id).await.ok().flatten()?;
    (record.content_hash == content_hash).then_some(ThreadSummaryData {
        text: record.text,
        model: record.model,
        generated_at: record.generated_at,
    })
}

pub(crate) async fn summarize_thread_cached(
    store: Arc<Store>,
    llm: Arc<LlmRuntime>,
    thread_id: &ThreadId,
) -> Result<ThreadSummaryData, String> {
    let context = load_summary_context(&store, thread_id).await?;
    if let Some(record) = store
        .get_thread_summary(thread_id)
        .await
        .map_err(|e| e.to_string())?
    {
        if record.content_hash == context.content_hash {
            return Ok(ThreadSummaryData {
                text: record.text,
                model: record.model,
                generated_at: record.generated_at,
            });
        }
    }

    let max_tokens = summary_token_budget(context.message_count);
    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(context.prompt),
        ],
        max_tokens: Some(max_tokens),
        temperature: Some(0.2),
    };

    let response = match llm
        .for_feature(LlmFeature::Summarize)
        .complete(request)
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => {
            return Err(
                "LLM is disabled. Enable it in [llm] in your config and configure a model \
                 (Ollama / LM Studio / OpenAI). See `mxr config`."
                    .to_string(),
            );
        }
        Err(e) => return Err(format!("LLM error: {e}")),
    };

    let generated_at = chrono::Utc::now();
    let summary = ThreadSummaryData {
        text: response.content.trim().to_string(),
        model: response.model,
        generated_at,
    };
    let record = ThreadSummaryRecord {
        thread_id: thread_id.clone(),
        account_id: context.account_id,
        content_hash: context.content_hash,
        text: summary.text.clone(),
        model: summary.model.clone(),
        generated_at,
    };
    store
        .upsert_thread_summary(&record)
        .await
        .map_err(|e| e.to_string())?;
    Ok(summary)
}

struct SummaryContext {
    account_id: mxr_core::AccountId,
    content_hash: String,
    message_count: u32,
    prompt: String,
}

async fn load_summary_context(
    store: &Store,
    thread_id: &ThreadId,
) -> Result<SummaryContext, String> {
    let thread = store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Thread {} not found", thread_id))?;
    let envelopes = store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    if envelopes.is_empty() {
        return Err(format!("Thread {} has no messages", thread_id));
    }
    let owned_addresses = store
        .list_account_addresses(&thread.account_id)
        .await
        .unwrap_or_default();

    let mut prompt = String::new();
    prompt.push_str("Account owner addresses:\n");
    if owned_addresses.is_empty() {
        prompt.push_str("- unknown\n");
    } else {
        for address in &owned_addresses {
            let primary = if address.is_primary { " (primary)" } else { "" };
            prompt.push_str(&format!("- {}{}\n", address.email, primary));
        }
    }
    prompt.push_str(&format!(
        "\nThread subject: {}\nThread id: {}\nMessage count: {}\n\n",
        thread.subject,
        thread.id,
        envelopes.len()
    ));
    let (relationship_prompt, relationship_hash) =
        relationship_context_for_summary(store, &thread.account_id, &owned_addresses, &envelopes)
            .await;
    if !relationship_prompt.is_empty() {
        prompt
            .push_str("Relationship context (weak background only; message content is primary):\n");
        prompt.push_str(&relationship_prompt);
        prompt.push('\n');
    }
    prompt.push_str("Messages, oldest to newest:\n");

    for (index, envelope) in envelopes.iter().enumerate() {
        let date = envelope
            .date
            .with_timezone(&chrono::Local)
            .format("%a %b %e %Y %H:%M %Z");
        let body = match store.get_body(&envelope.id).await {
            Ok(Some(body)) => body
                .text_plain
                .or(body.text_html)
                .unwrap_or_else(|| envelope.snippet.clone()),
            _ => envelope.snippet.clone(),
        };
        prompt.push_str(&format!(
            "\n--- Message {} of {} ---\nDate: {}\nFrom: {}\nTo: {}\nCc: {}\nBcc: {}\nSubject: {}\nBody:\n{}\n",
            index + 1,
            envelopes.len(),
            date,
            format_address(&envelope.from),
            format_addresses(&envelope.to),
            format_addresses(&envelope.cc),
            format_addresses(&envelope.bcc),
            envelope.subject,
            body.trim(),
        ));
    }

    Ok(SummaryContext {
        account_id: thread.account_id,
        content_hash: format!(
            "{}:{relationship_hash}:{SUMMARY_PROMPT_VERSION}",
            thread_summary_content_hash(&envelopes)
        ),
        message_count: envelopes.len().try_into().unwrap_or(u32::MAX),
        prompt,
    })
}

async fn relationship_context_for_summary(
    store: &Store,
    account_id: &mxr_core::AccountId,
    owned_addresses: &[AccountAddress],
    envelopes: &[Envelope],
) -> (String, String) {
    let owned = owned_addresses
        .iter()
        .map(|address| address.email.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut contacts = BTreeSet::new();
    for envelope in envelopes {
        maybe_insert_contact(&mut contacts, &owned, &envelope.from.email);
        for address in envelope
            .to
            .iter()
            .chain(envelope.cc.iter())
            .chain(envelope.bcc.iter())
        {
            maybe_insert_contact(&mut contacts, &owned, &address.email);
        }
    }
    let mut prompt = String::new();
    let mut hash_parts = Vec::new();
    for email in contacts.into_iter().take(3) {
        let Ok(Some(profile)) =
            relationship_profile::load_relationship_profile_for_store(store, account_id, &email)
                .await
        else {
            continue;
        };
        prompt.push_str(&format!("- {email}: "));
        if let Some(summary) = profile.summary {
            prompt.push_str(summary.text.trim());
            if !summary.known_topics.is_empty() {
                prompt.push_str(&format!(
                    " Known topics: {}.",
                    summary.known_topics.join(", ")
                ));
            }
            hash_parts.push(summary.source_hash);
        }
        if let Some(style) = profile.style {
            prompt.push_str(&format!(
                " Style: your formality {:.2}, their formality {:.2}.",
                style.formality_score, style.formality_score_theirs
            ));
            hash_parts.push(style.source_hash);
        }
        prompt.push('\n');
        if prompt.len() > 2_000 {
            prompt.truncate(2_000);
            prompt.push_str("\n[...relationship context truncated...]\n");
            break;
        }
    }
    (prompt, hash_parts.join("|"))
}

fn maybe_insert_contact(contacts: &mut BTreeSet<String>, owned: &BTreeSet<String>, email: &str) {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() || owned.contains(&email) {
        return;
    }
    contacts.insert(email);
}

fn summary_token_budget(message_count: u32) -> u32 {
    400u32
        .saturating_add(message_count.saturating_mul(120))
        .clamp(700, 2_400)
}

fn format_addresses(addresses: &[Address]) -> String {
    if addresses.is_empty() {
        return "(none)".into();
    }
    addresses
        .iter()
        .map(format_address)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_address(address: &Address) -> String {
    match address
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        Some(name) => format!("{name} <{}>", address.email),
        None => address.email.clone(),
    }
}

#[allow(dead_code)]
fn _owned_address_emails(addresses: &[AccountAddress]) -> Vec<&str> {
    addresses
        .iter()
        .map(|address| address.email.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::{MessageBody, MessageMetadata};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use mxr_store::ContactRelationshipSummaryRecord;
    use std::sync::Mutex;

    #[derive(Default)]
    struct CapturingLlm {
        requests: Mutex<Vec<CompletionRequest>>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CapturingLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.requests.lock().expect("requests").push(req);
            Ok(CompletionResponse {
                content: "Alice asks you to approve the launch plan.\nAction: reply with approval or pushback by Fri."
                    .to_string(),
                model: "test-llm".to_string(),
                finish_reason: Some("stop".to_string()),
            })
        }

        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 8192,
                supports_streaming: false,
            }
        }

        fn model_name(&self) -> &str {
            "test-llm"
        }
    }

    fn body(message_id: mxr_core::MessageId, text: &str) -> MessageBody {
        MessageBody {
            message_id,
            text_plain: Some(text.to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[tokio::test]
    async fn summary_prompt_includes_full_sender_and_recipient_context() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let _ = state
            .store
            .add_account_address(&account_id, "alias@example.com", false)
            .await;
        let thread_id = mxr_core::ThreadId::new();
        let mut first = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread_id.clone())
            .provider_id("first")
            .from_address("Alice", "alice@example.com")
            .to_address(Some("Bob"), "bob@example.com")
            .subject("Decision")
            .snippet("Please approve")
            .build();
        first.cc = vec![Address {
            name: Some("Me".into()),
            email: "alias@example.com".into(),
        }];
        let second = TestEnvelopeBuilder::new()
            .account_id(account_id)
            .thread_id(thread_id.clone())
            .provider_id("second")
            .from_address("Bob", "bob@example.com")
            .to_address(Some("Alice"), "alice@example.com")
            .subject("Decision")
            .snippet("Approved")
            .build();
        state.store.upsert_envelope(&first).await.unwrap();
        state.store.upsert_envelope(&second).await.unwrap();
        state
            .store
            .insert_body(&body(first.id.clone(), "Please approve the plan."))
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(second.id.clone(), "Approved."))
            .await
            .unwrap();

        let response = summarize_thread(&state, &thread_id).await.unwrap();
        assert!(matches!(response, ResponseData::ThreadSummary { .. }));

        let requests = llm.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        let prompt = &requests[0].messages[1].content;
        assert!(prompt.contains("Account owner addresses:"));
        assert!(prompt.contains("alias@example.com"));
        assert!(prompt.contains("Message 1 of 2"));
        assert!(prompt.contains("From: Alice <alice@example.com>"));
        assert!(prompt.contains("To: Bob <bob@example.com>"));
        assert!(prompt.contains("Cc: Me <alias@example.com>"));
        assert!(prompt.contains("Message 2 of 2"));
        assert!(prompt.contains("Approved."));
    }

    #[tokio::test]
    async fn summary_is_persisted_and_reused_until_thread_changes() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let thread_id = mxr_core::ThreadId::new();
        let first = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread_id.clone())
            .provider_id("first")
            .build();
        state.store.upsert_envelope(&first).await.unwrap();
        state
            .store
            .insert_body(&body(first.id.clone(), "Initial body."))
            .await
            .unwrap();

        summarize_thread(&state, &thread_id).await.unwrap();
        summarize_thread(&state, &thread_id).await.unwrap();
        assert_eq!(llm.requests.lock().expect("requests").len(), 1);

        let second = TestEnvelopeBuilder::new()
            .account_id(account_id)
            .thread_id(thread_id.clone())
            .provider_id("second")
            .build();
        state.store.upsert_envelope(&second).await.unwrap();
        state
            .store
            .insert_body(&body(second.id.clone(), "New reply."))
            .await
            .unwrap();

        summarize_thread(&state, &thread_id).await.unwrap();
        assert_eq!(llm.requests.lock().expect("requests").len(), 2);
    }

    #[tokio::test]
    async fn summary_cache_includes_relationship_context_hash() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let thread_id = mxr_core::ThreadId::new();
        let message = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread_id.clone())
            .provider_id("first")
            .from_address("Alice", "alice@example.com")
            .subject("Launch")
            .snippet("Can you review the launch note?")
            .build();
        state.store.upsert_envelope(&message).await.unwrap();
        state
            .store
            .insert_body(&body(message.id.clone(), "Can you review the launch note?"))
            .await
            .unwrap();
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: "alice@example.com".to_string(),
                text: "Alice prefers launch context before asks.".to_string(),
                model: "test-model".to_string(),
                known_topics: vec!["launch".to_string()],
                computed_at: chrono::Utc::now(),
                source_hash: "relationship-v1".to_string(),
                last_error: None,
            })
            .await
            .unwrap();

        summarize_thread(&state, &thread_id).await.unwrap();
        summarize_thread(&state, &thread_id).await.unwrap();
        {
            let requests = llm.requests.lock().expect("requests");
            assert_eq!(requests.len(), 1);
            let prompt = &requests[0].messages[1].content;
            assert!(prompt.contains("Relationship context (weak background only"));
            assert!(prompt.contains("Alice prefers launch context before asks."));
        }
        let envelopes = state.store.get_thread_envelopes(&thread_id).await.unwrap();
        assert!(valid_cached_summary(&state.store, &thread_id, &envelopes)
            .await
            .is_some());

        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id,
                email: "alice@example.com".to_string(),
                text: "Alice now wants launch risks called out first.".to_string(),
                model: "test-model".to_string(),
                known_topics: vec!["launch".to_string(), "risk".to_string()],
                computed_at: chrono::Utc::now(),
                source_hash: "relationship-v2".to_string(),
                last_error: None,
            })
            .await
            .unwrap();
        assert!(valid_cached_summary(&state.store, &thread_id, &envelopes)
            .await
            .is_none());

        summarize_thread(&state, &thread_id).await.unwrap();
        let requests = llm.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[1].messages[1]
            .content
            .contains("Alice now wants launch risks called out first."));
    }
}
