//! `GetThreadGist`: the model-written half of the reader's context block, a
//! one-sentence gist and what the conversation asks of you.
//!
//! The thread's own messages go to the model under the same rule as
//! `SummarizeThread` (the `Summarize` feature). Your history with the
//! counterparty only rides along when `state::relationship_data_allowed`
//! says so. The ask's quote is checked against the message text before it
//! is returned, so a client can highlight it without trusting the model.
//! Answers are cached per thread and newest message.

use super::thread_context::{build_thread_context, owned_addresses};
use super::{HandlerError, HandlerResult};
use crate::state::{llm_endpoint_is_local, AppState};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::Envelope;
use mxr_llm::{
    guarded_system_prompt, wrap_untrusted_mail, ChatMessage, CompletionRequest, LlmError,
    LlmFeature, PinnedLlm,
};
use mxr_protocol::{
    AiLocalityData, AiProvenanceData, AiSourceData, ResponseData, ThreadAskData, ThreadContextData,
    ThreadGistData, ThreadGistStatusData, VerifiedQuoteData,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{new_briefing_id, BriefingKind, ContextBriefing};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};

/// Bump when the prompt or the output rules change, so cached gists
/// written under the old rules regenerate.
const GIST_PROMPT_VERSION: &str = "v1";
/// Model output limits, in characters. Longer text is cut at a word.
const GIST_MAX_CHARS: usize = 240;
const ASK_MAX_CHARS: usize = 160;
const QUOTE_MAX_CHARS: usize = 400;
/// Shorter "quotes" ("yes", "ok?") would highlight noise.
const QUOTE_MIN_CHARS: usize = 8;
/// Prompt budget: each message is capped, and older messages drop first.
const MESSAGE_MAX_CHARS: usize = 6_000;
const TRANSCRIPT_MAX_CHARS: usize = 24_000;

const FEATURE: LlmFeature = LlmFeature::Summarize;

pub(crate) const SYSTEM_PROMPT: &str = r#"You write the reader gist for one email conversation, shown above the messages in an email client.

Output STRICT JSON and nothing else:
{"gist": string, "ask": null or {"summary": string, "msg_id": string, "quote": string}}

gist: one plain-text sentence of at most 200 characters saying what is currently true in the conversation: the decision, the state, the open question. Concrete names, numbers and dates beat descriptions. Do not restate the subject line or who sent it. No markdown.

ask: what the other people want the account owner to do (reply, decide, send, confirm, attend, pay, review), taken from messages the owner has not answered yet. Use null when nothing is asked of the owner, including newsletters, receipts, notifications and marketing.
- summary: a short imperative clause starting with a verb, at most 120 characters, for example "confirm who owns the rollout check before Monday".
- msg_id: the [msg_id=...] value of the message that makes the ask.
- quote: the sentence in that message that makes the ask, copied exactly, character for character. Do not paraphrase, fix typos, shorten or join sentences.

Messages the account owner sent are marked (you)."#;

#[derive(Debug, Deserialize)]
struct RawGist {
    #[serde(default)]
    gist: String,
    #[serde(default)]
    ask: Option<RawAsk>,
}

#[derive(Debug, Deserialize)]
struct RawAsk {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    msg_id: String,
    #[serde(default)]
    quote: String,
}

/// What the cache row holds, as JSON in `context_briefings.body_markdown`.
#[derive(Debug, Serialize, Deserialize)]
struct CachedGist {
    gist: String,
    ask: Option<ThreadAskData>,
    provenance: AiProvenanceData,
    /// Messages that had no synced body, so their snippet stood in. The gist
    /// is regenerated once any of them syncs.
    #[serde(default)]
    snippet_ids: Vec<MessageId>,
}

/// The gist shares the briefings cache table; the prefix keeps its rows
/// apart from `GetThreadBriefing`'s, which key on the bare thread id.
fn cache_key(thread_id: &ThreadId) -> String {
    format!("gist:{thread_id}")
}

/// How the model is called: a reader waiting on the gist gets the
/// foreground budget; the list-row writer runs under the background
/// timeout and breaker, so a slow or dead endpoint can't pile up work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GistCall {
    Foreground,
    Background,
}

pub(super) async fn get_thread_gist(
    state: &AppState,
    thread_id: &ThreadId,
    refresh: bool,
) -> HandlerResult {
    let (gist, _) = load_and_write_gist(state, thread_id, refresh, GistCall::Foreground).await?;
    Ok(ResponseData::ThreadGist { gist })
}

/// Load the conversation and return its gist (cached unless `refresh`),
/// with the content hash it answers for.
pub(super) async fn load_and_write_gist(
    state: &AppState,
    thread_id: &ThreadId,
    refresh: bool,
    call: GistCall,
) -> Result<(ThreadGistData, String), HandlerError> {
    let envelopes = thread_envelopes(state, thread_id).await?;
    let setup = GistSetup::new(&GistPolicy::pin(state), thread_id, &envelopes)
        .ok_or_else(|| HandlerError::from(format!("thread {thread_id} not found")))?;
    let gist = thread_gist(state, thread_id, &envelopes, &setup, refresh, call).await?;
    Ok((gist, setup.content_hash))
}

/// The conversation's messages, oldest first; empty for an unknown thread.
pub(super) async fn thread_envelopes(
    state: &AppState,
    thread_id: &ThreadId,
) -> Result<Vec<Envelope>, HandlerError> {
    let mut envelopes = state.store.get_thread_envelopes(thread_id).await?;
    envelopes.sort_by_key(|envelope| envelope.date);
    Ok(envelopes)
}

/// The model a gist request uses and what its prompt may carry, decided
/// once per request (or once per batch).
#[derive(Clone)]
pub(super) struct GistPolicy {
    /// Pin the provider once: what the prompt may carry, the provenance
    /// label and the call itself all follow this one endpoint, even if a
    /// config reload swaps the runtime mid-request.
    pub llm: PinnedLlm,
    share_history: bool,
    locality: AiLocalityData,
}

impl GistPolicy {
    pub fn pin(state: &AppState) -> Self {
        let llm = state.llm.for_feature(FEATURE).pin();
        let local = llm_endpoint_is_local(llm.base_url());
        let share_history = local || state.config_snapshot().llm.allow_cloud_relationship_data;
        let locality = if local {
            AiLocalityData::Local
        } else {
            AiLocalityData::Cloud
        };
        Self {
            llm,
            share_history,
            locality,
        }
    }
}

/// One conversation's gist request: the policy plus its cache key.
pub(super) struct GistSetup {
    policy: GistPolicy,
    account_id: AccountId,
    pub content_hash: String,
    key: String,
}

impl GistSetup {
    /// `envelopes` is the whole conversation, oldest first; `None` when it
    /// has no messages.
    pub fn new(policy: &GistPolicy, thread_id: &ThreadId, envelopes: &[Envelope]) -> Option<Self> {
        let newest = envelopes.last()?;
        Some(Self {
            content_hash: gist_content_hash(envelopes, policy.share_history, &policy.llm),
            policy: policy.clone(),
            account_id: newest.account_id.clone(),
            key: cache_key(thread_id),
        })
    }
}

/// The cached gist, when it was written for this conversation's newest
/// message under the same model, endpoint and privacy setting, and no
/// message it read from a snippet has synced its body since. No model call.
pub(super) async fn cached_gist(
    state: &AppState,
    thread_id: &ThreadId,
    setup: &GistSetup,
) -> Result<Option<ThreadGistData>, HandlerError> {
    let Some(cached) = state
        .store
        .get_context_briefing(&setup.account_id, BriefingKind::Thread, &setup.key)
        .await?
        .filter(|row| row.content_hash == setup.content_hash)
    else {
        return Ok(None);
    };
    let Ok(payload) = serde_json::from_str::<CachedGist>(&cached.body_markdown) else {
        return Ok(None);
    };
    if !payload.snippet_ids.is_empty() && state.store.any_body_synced(&payload.snippet_ids).await? {
        return Ok(None);
    }
    Ok(Some(ready(thread_id, payload, cached.generated_at, true)))
}

/// The gist for a conversation: from the cache unless `refresh`, else
/// written by the model and cached. An unavailable model is a status.
async fn thread_gist(
    state: &AppState,
    thread_id: &ThreadId,
    envelopes: &[Envelope],
    setup: &GistSetup,
    refresh: bool,
    call: GistCall,
) -> Result<ThreadGistData, HandlerError> {
    if !refresh {
        if let Some(cached) = cached_gist(state, thread_id, setup).await? {
            return Ok(cached);
        }
    }
    let llm = &setup.policy.llm;
    let owned = owned_addresses(state, &setup.account_id).await?;
    let facts = if setup.policy.share_history {
        Some(build_thread_context(state, thread_id, envelopes, &owned).await?)
    } else {
        None
    };
    let texts = message_texts(state, envelopes).await;
    let history = facts.as_ref().and_then(history_line);
    let prompt = build_user_prompt(&owned, envelopes, &texts, history.as_deref());
    let mut sources = vec![AiSourceData::ThisThread];
    if history.is_some() {
        sources.push(AiSourceData::RelationshipHistory);
    }

    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(guarded_system_prompt(SYSTEM_PROMPT)),
            ChatMessage::user(prompt),
        ],
        max_tokens: Some(400),
        temperature: Some(0.1),
    };
    let answer = match call {
        GistCall::Foreground => llm.complete(request).await,
        GistCall::Background => llm.complete_background(request).await,
    };
    let response = match answer {
        Ok(response) => response,
        Err(LlmError::Disabled) => {
            return Ok(unavailable(
                thread_id,
                ThreadGistStatusData::Disabled,
                "No language model is configured.",
            ))
        }
        Err(LlmError::PrivacyBlocked(reason)) => {
            return Ok(unavailable(
                thread_id,
                ThreadGistStatusData::Blocked,
                &reason,
            ))
        }
        Err(error) => {
            return Ok(unavailable(
                thread_id,
                ThreadGistStatusData::Failed,
                &error.to_string(),
            ))
        }
    };
    // Quotes are checked against each message's stored plain text first,
    // then against the text the prompt carried.
    let candidates: Vec<(MessageId, &str)> = texts
        .iter()
        .flat_map(|text| {
            text.plain
                .as_deref()
                .into_iter()
                .chain(std::iter::once(text.prompt.as_str()))
                .map(|candidate| (text.id.clone(), candidate))
        })
        .collect();
    let Some(parsed) = parse_gist(&response.content, &candidates) else {
        tracing::warn!(%thread_id, "thread gist: model answer was not usable");
        return Ok(unavailable(
            thread_id,
            ThreadGistStatusData::Failed,
            "The model's answer wasn't usable.",
        ));
    };
    let model = if response.model.trim().is_empty() {
        llm.model_name().to_string()
    } else {
        response.model
    };
    let payload = CachedGist {
        gist: parsed.0,
        ask: parsed.1,
        provenance: AiProvenanceData {
            model,
            locality: setup.policy.locality,
            sources,
        },
        snippet_ids: texts
            .iter()
            .filter(|text| text.from_snippet)
            .map(|text| text.id.clone())
            .collect(),
    };
    let generated_at = chrono::Utc::now();
    state
        .store
        .upsert_context_briefing(&ContextBriefing {
            id: new_briefing_id(),
            account_id: setup.account_id.clone(),
            kind: BriefingKind::Thread,
            subject_key: setup.key.clone(),
            content_hash: setup.content_hash.clone(),
            body_markdown: serde_json::to_string(&payload)?,
            citations: vec![],
            generated_at,
        })
        .await?;
    Ok(ready(thread_id, payload, generated_at, false))
}

fn ready(
    thread_id: &ThreadId,
    payload: CachedGist,
    generated_at: chrono::DateTime<chrono::Utc>,
    from_cache: bool,
) -> ThreadGistData {
    ThreadGistData {
        thread_id: thread_id.clone(),
        status: ThreadGistStatusData::Ready,
        gist: Some(payload.gist),
        ask: payload.ask,
        provenance: Some(payload.provenance),
        reason: None,
        generated_at: Some(generated_at),
        from_cache,
    }
}

fn unavailable(thread_id: &ThreadId, status: ThreadGistStatusData, reason: &str) -> ThreadGistData {
    ThreadGistData {
        thread_id: thread_id.clone(),
        status,
        gist: None,
        ask: None,
        provenance: None,
        reason: Some(reason.to_string()),
        generated_at: None,
        from_cache: false,
    }
}

/// Changes when a message arrives, the prompt changes, the model or its
/// endpoint changes (so a same-named model moved from local to cloud never
/// serves a gist with the old provenance), or the privacy setting changes
/// what the prompt may carry.
fn gist_content_hash(envelopes: &[Envelope], share_history: bool, llm: &PinnedLlm) -> String {
    let mut hash = Sha256::new();
    hash.update(GIST_PROMPT_VERSION.as_bytes());
    hash.update(envelopes.len().to_le_bytes());
    if let Some(newest) = envelopes.last() {
        hash.update(newest.id.as_str().as_bytes());
        hash.update(newest.date.timestamp().to_le_bytes());
    }
    hash.update([u8::from(share_history)]);
    hash.update(llm.model_name().as_bytes());
    hash.update(b"|");
    hash.update(llm.base_url().unwrap_or("on-machine").as_bytes());
    base16ct::lower::encode_string(&hash.finalize())
}

/// One message as the gist reads it.
struct MessageText {
    id: MessageId,
    /// What the prompt carries: the plain part when there is one, else the
    /// HTML reduced to text, capped per message.
    prompt: String,
    /// The plain part as stored, which the readers show; quotes are looked
    /// up here first so the returned span is literally in the body.
    plain: Option<String>,
    /// No body was synced yet, so the prompt used the snippet.
    from_snippet: bool,
}

/// The newest messages that fit the prompt budget, oldest first. Older
/// messages past the budget are never read.
async fn message_texts(state: &AppState, envelopes: &[Envelope]) -> Vec<MessageText> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for envelope in envelopes.iter().rev() {
        let body = state.store.get_body(&envelope.id).await.ok().flatten();
        let (mut prompt, plain, from_snippet) = match body {
            Some(body) if body.text_plain.is_some() || body.text_html.is_some() => {
                let cleaned = clean(
                    body.text_plain.as_deref(),
                    body.text_html.as_deref(),
                    &ReaderConfig::default(),
                )
                .content;
                let prompt = if cleaned.trim().is_empty() {
                    body.text_plain.clone().unwrap_or_default()
                } else {
                    cleaned
                };
                (prompt, body.text_plain, false)
            }
            _ => (envelope.snippet.clone(), None, true),
        };
        mxr_core::text::truncate_to_char_boundary(&mut prompt, MESSAGE_MAX_CHARS);
        if used + prompt.len() > TRANSCRIPT_MAX_CHARS && !out.is_empty() {
            break;
        }
        used += prompt.len();
        out.push(MessageText {
            id: envelope.id.clone(),
            prompt,
            plain,
            from_snippet,
        });
    }
    out.reverse();
    out
}

fn build_user_prompt(
    owned: &BTreeSet<String>,
    envelopes: &[Envelope],
    texts: &[MessageText],
    history: Option<&str>,
) -> String {
    let mut prompt = String::from("Account owner addresses:\n");
    if owned.is_empty() {
        prompt.push_str("- unknown\n");
    }
    for email in owned {
        prompt.push_str(&format!("- {email}\n"));
    }

    let bodies: HashMap<&MessageId, &str> = texts
        .iter()
        .map(|text| (&text.id, text.prompt.as_str()))
        .collect();
    let blocks: Vec<String> = envelopes
        .iter()
        .filter_map(|envelope| {
            let body = bodies.get(&envelope.id)?;
            let from_owner = owned.contains(&envelope.from.email.to_ascii_lowercase());
            Some(format!(
                "[msg_id={}]\nFrom: {}{}\nDate: {}\nSubject: {}\n{}\n",
                envelope.id,
                envelope.from.email,
                if from_owner { " (you)" } else { "" },
                envelope.date.to_rfc3339(),
                envelope.subject,
                body.trim(),
            ))
        })
        .collect();

    let mut untrusted = String::new();
    if let Some(history) = history {
        untrusted.push_str(history);
        untrusted.push_str("\n\n");
    }
    untrusted.push_str("Messages, oldest to newest:\n\n");
    untrusted.push_str(&blocks.join("\n"));
    prompt.push('\n');
    prompt.push_str(&wrap_untrusted_mail(&untrusted));
    prompt.push_str("\n\nReturn JSON only.");
    prompt
}

/// Relationship stats as one line of background, or `None` when there is
/// nothing to say.
fn history_line(facts: &ThreadContextData) -> Option<String> {
    let person = facts.counterparty.as_ref()?;
    let mut line = format!(
        "History with {}: {} messages from them, {} from the owner",
        person.email, person.messages_from_them, person.messages_from_you
    );
    if let Some(seconds) = person.your_reply_p50_seconds {
        line.push_str(&format!(
            ", the owner usually replies within {} hours",
            (f64::from(seconds) / 3600.0).ceil()
        ));
    }
    match person.last_contact_elsewhere_at {
        Some(when) => line.push_str(&format!(
            ", last contact before this thread on {}",
            when.format("%Y-%m-%d")
        )),
        None => line.push_str(", no earlier conversations"),
    }
    line.push('.');
    Some(line)
}

/// Validate the model's JSON: plain text within limits, and a quote only
/// when it really appears in the cited message (or, when the model cited
/// the wrong id, in another message of the thread).
fn parse_gist(
    content: &str,
    texts: &[(MessageId, &str)],
) -> Option<(String, Option<ThreadAskData>)> {
    let raw: RawGist = serde_json::from_str(json_object(content)).ok()?;
    let gist = plain_text(&raw.gist, GIST_MAX_CHARS);
    if gist.is_empty() {
        return None;
    }
    let ask = raw.ask.and_then(|ask| {
        let summary = plain_text(&ask.summary, ASK_MAX_CHARS);
        if summary.is_empty() {
            return None;
        }
        Some(ThreadAskData {
            summary,
            quote: verify_quote(&ask.quote, ask.msg_id.trim(), texts),
        })
    });
    Some((gist, ask))
}

/// The JSON object inside a completion that may carry prose or code fences.
pub(super) fn json_object(content: &str) -> &str {
    let trimmed = content.trim();
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => trimmed,
    }
}

/// One line of display text: no control characters, markdown marks or em
/// dashes, whitespace collapsed, cut at a word boundary within `max_chars`.
pub(super) fn plain_text(value: &str, max_chars: usize) -> String {
    let cleaned: String = value
        .replace(" — ", ", ")
        .replace('—', ", ")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| !matches!(c, '*' | '`' | '#'))
        .collect();
    let collapsed = collapse_whitespace(&cleaned);
    let text = collapsed
        .trim_start_matches(['-', '>', ' '])
        .trim_matches(['"', '\''])
        .trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

pub(super) fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Curly quotes and apostrophes compare equal to straight ones; models
/// swap them freely. The mapping is one char for one char, so a match in
/// the folded text is the same span in the unfolded one.
fn fold_quotes(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
        other => other,
    }
}

/// The quote exactly as it appears in a message's text, or `None` when it is
/// too short, too long, or not in the thread. Matching ignores how
/// whitespace is laid out and curly versus straight quote marks; the span
/// returned is the original slice of the text, so it is literally a
/// substring of it. A paraphrase never matches. `texts` holds each
/// message's candidate texts (stored plain part first); the cited message is
/// tried before the others, newest first.
fn verify_quote(
    quote: &str,
    msg_id: &str,
    texts: &[(MessageId, &str)],
) -> Option<VerifiedQuoteData> {
    let wanted = normalize(
        collapse_whitespace(quote).trim_matches(['"', '\u{201C}', '\u{201D}']),
        fold_quotes,
    );
    if wanted.len() < QUOTE_MIN_CHARS || wanted.len() > QUOTE_MAX_CHARS {
        return None;
    }
    let cited = texts.iter().filter(|(id, _)| id.as_str() == msg_id);
    let others = texts.iter().rev().filter(|(id, _)| id.as_str() != msg_id);
    cited.chain(others).find_map(|(id, text)| {
        let span = find_normalized(text, &wanted, fold_quotes)?;
        Some(VerifiedQuoteData {
            message_id: id.clone(),
            text: text[span].to_string(),
        })
    })
}

/// `phrase` as it is written in `text`, matching the way quotes are checked
/// but also ignoring letter case: the original slice, or `None` when the
/// text doesn't say it. Model output is only shown when it passes this.
pub(super) fn find_phrase_ignoring_case<'a>(text: &'a str, phrase: &str) -> Option<&'a str> {
    let fold = |c: char| fold_quotes(c.to_lowercase().next().unwrap_or(c));
    let wanted = normalize(&collapse_whitespace(phrase), fold);
    if wanted.is_empty() {
        return None;
    }
    find_normalized(text, &wanted, fold).map(|span| &text[span])
}

/// `phrase` (already whitespace-collapsed) as the chars `find_normalized`
/// compares against.
fn normalize(phrase: &str, fold: impl Fn(char) -> char) -> Vec<char> {
    phrase.chars().map(fold).collect()
}

/// Byte range in `text` whose whitespace-collapsed, quote-folded form equals
/// `wanted` (already collapsed and folded).
fn find_normalized(
    text: &str,
    wanted: &[char],
    fold: impl Fn(char) -> char,
) -> Option<std::ops::Range<usize>> {
    // Each normalized char with the byte range of the original it stands for.
    let mut normalized: Vec<(char, usize, usize)> = Vec::new();
    for (index, c) in text.char_indices() {
        let end = index + c.len_utf8();
        if c.is_whitespace() {
            match normalized.last_mut() {
                Some((' ', _, last_end)) => *last_end = end,
                _ => normalized.push((' ', index, end)),
            }
        } else {
            normalized.push((fold(c), index, end));
        }
    }
    let start = normalized
        .windows(wanted.len())
        .position(|window| window.iter().map(|(c, _, _)| *c).eq(wanted.iter().copied()))?;
    let first = normalized[start];
    let last = normalized[start + wanted.len() - 1];
    Some(first.1..last.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    const ASK_SENTENCE: &str = "Can you confirm who owns the rollout check before Monday?";

    struct ScriptedLlm {
        answer: String,
        calls: AtomicUsize,
        prompts: Mutex<Vec<String>>,
        base_url: Option<String>,
    }

    impl ScriptedLlm {
        fn new(answer: impl Into<String>) -> Arc<Self> {
            Self::at(answer, None)
        }

        /// A stub that reports `base_url` as its endpoint.
        fn at(answer: impl Into<String>, base_url: Option<&str>) -> Arc<Self> {
            Arc::new(Self {
                answer: answer.into(),
                calls: AtomicUsize::new(0),
                prompts: Mutex::new(Vec::new()),
                base_url: base_url.map(str::to_string),
            })
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for ScriptedLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.prompts.lock().unwrap().push(
                req.messages
                    .iter()
                    .map(|message| message.content.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            Ok(CompletionResponse {
                content: self.answer.clone(),
                model: "qwen2.5:7b".into(),
                finish_reason: Some("stop".into()),
            })
        }
        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 32_000,
                supports_streaming: false,
            }
        }
        fn model_name(&self) -> &str {
            "qwen2.5:7b"
        }
        fn base_url(&self) -> Option<&str> {
            self.base_url.as_deref()
        }
    }

    fn texts(body: &str) -> (MessageId, Vec<(MessageId, &str)>) {
        let id = MessageId::new();
        (id.clone(), vec![(id, body)])
    }

    #[test]
    fn quote_verification_accepts_the_exact_sentence_across_line_breaks() {
        let (id, texts) = texts(
            "Rollout risk: watch sync latency.\nCan you confirm who owns the\nrollout check before Monday?\n\nThanks",
        );
        let quote = verify_quote(ASK_SENTENCE, &id.as_str(), &texts).expect("exact quote verifies");
        assert_eq!(quote.message_id, id);
        // The original slice, line breaks and all: literally in the body.
        assert_eq!(
            quote.text,
            "Can you confirm who owns the\nrollout check before Monday?"
        );
        assert!(texts[0].1.contains(&quote.text));
    }

    #[test]
    fn quote_verification_rejects_a_paraphrase() {
        let body = format!("Hi,\n{ASK_SENTENCE}\nMaya");
        let (id, texts) = texts(&body);
        for paraphrase in [
            "Could you confirm who owns the rollout check before Monday?",
            "Can you confirm who owns the rollout check by Monday?",
            "can you confirm who owns the rollout check before monday?",
        ] {
            assert_eq!(
                verify_quote(paraphrase, &id.as_str(), &texts),
                None,
                "{paraphrase}"
            );
        }
    }

    #[test]
    fn quote_verification_folds_curly_quotes_but_returns_the_message_text() {
        let (id, texts) = texts("We’re blocked. Can you send Maya’s deck today?");
        let quote = verify_quote("Can you send Maya's deck today?", &id.as_str(), &texts)
            .expect("apostrophe style may differ");
        assert_eq!(quote.text, "Can you send Maya’s deck today?");
    }

    #[test]
    fn quote_verification_finds_a_quote_cited_under_the_wrong_message() {
        let first = MessageId::new();
        let second = MessageId::new();
        let hello = format!("Hello. {ASK_SENTENCE}");
        let texts = vec![
            (first.clone(), "Earlier note with nothing to ask."),
            (second.clone(), hello.as_str()),
        ];
        let quote = verify_quote(ASK_SENTENCE, &first.as_str(), &texts).unwrap();
        assert_eq!(quote.message_id, second);
        assert_eq!(
            verify_quote("ok?", &second.as_str(), &texts),
            None,
            "too short"
        );
    }

    #[test]
    fn model_output_is_validated_to_plain_text_within_limits() {
        let (id, texts) = texts(ASK_SENTENCE);
        let long = "word ".repeat(100);
        let content = format!(
            "```json\n{{\"gist\": \"**Canary** stays at 5% — {long}\", \"ask\": {{\"summary\": \"# confirm the owner\", \"msg_id\": \"{id}\", \"quote\": \"not in the message at all\"}}}}\n```"
        );
        let (gist, ask) = parse_gist(&content, &texts).expect("fenced JSON parses");
        assert!(gist.starts_with("Canary stays at 5%, word"), "{gist}");
        assert!(gist.chars().count() <= GIST_MAX_CHARS);
        assert!(gist.ends_with('…'));
        let ask = ask.unwrap();
        assert_eq!(ask.summary, "confirm the owner");
        assert_eq!(
            ask.quote, None,
            "an unverifiable quote is dropped, the ask stays"
        );
        assert!(parse_gist("not json", &texts).is_none());
        assert!(parse_gist(r#"{"gist": "   ", "ask": null}"#, &texts).is_none());
    }

    async fn seed_thread(state: &AppState) -> (ThreadId, MessageId) {
        let account_id = state.default_account_id();
        let thread_id = ThreadId::new();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread_id.clone())
            .provider_id("gist-1")
            .sender_address("Maya Ortiz", "maya@example.com")
            .subject("Launch checklist")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some(format!(
                    "Rollout risk: watch sync latency for the first hour.\n{ASK_SENTENCE}\n\nMaya"
                )),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            })
            .await
            .unwrap();
        (thread_id, envelope.id)
    }

    fn answer_for(id: &MessageId) -> String {
        serde_json::json!({
            "gist": "Canary stays at 5% until the dashboard is quiet.",
            "ask": {
                "summary": "confirm who owns the rollout check",
                "msg_id": id.to_string(),
                "quote": ASK_SENTENCE,
            }
        })
        .to_string()
    }

    fn gist(response: ResponseData) -> ThreadGistData {
        match response {
            ResponseData::ThreadGist { gist } => gist,
            other => panic!("unexpected response {other:?}"),
        }
    }

    #[tokio::test]
    async fn gist_is_cached_per_newest_message_and_reports_provenance() {
        let state = AppState::in_memory().await.unwrap();
        let (thread_id, message_id) = seed_thread(&state).await;
        let llm = ScriptedLlm::new(answer_for(&message_id));
        state.llm.replace(llm.clone());

        let first = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert_eq!(first.status, ThreadGistStatusData::Ready);
        assert!(!first.from_cache);
        let ask = first.ask.clone().unwrap();
        assert_eq!(
            ask.quote,
            Some(VerifiedQuoteData {
                message_id: message_id.clone(),
                text: ASK_SENTENCE.into()
            })
        );
        let provenance = first.provenance.clone().unwrap();
        assert_eq!(provenance.model, "qwen2.5:7b");
        assert_eq!(provenance.locality, AiLocalityData::Local);

        let second = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert!(second.from_cache);
        assert_eq!(second.gist, first.gist);
        assert_eq!(llm.calls.load(Ordering::SeqCst), 1, "second read is cached");

        // A new message invalidates the cache.
        let reply = TestEnvelopeBuilder::new()
            .account_id(state.default_account_id())
            .thread_id(thread_id.clone())
            .provider_id("gist-2")
            .sender_address("Maya Ortiz", "maya@example.com")
            .date(chrono::Utc::now() + chrono::Duration::minutes(5))
            .build();
        state
            .store
            .upsert_envelope_with_direction(&reply, MessageDirection::Inbound)
            .await
            .unwrap();
        let third = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert!(!third.from_cache);
        assert_eq!(llm.calls.load(Ordering::SeqCst), 2);

        // refresh skips the cache.
        gist(get_thread_gist(&state, &thread_id, true).await.unwrap());
        assert_eq!(llm.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn prompt_wraps_mail_as_untrusted_data() {
        let state = AppState::in_memory().await.unwrap();
        let (thread_id, message_id) = seed_thread(&state).await;
        let llm = ScriptedLlm::new(answer_for(&message_id));
        state.llm.replace(llm.clone());
        get_thread_gist(&state, &thread_id, false).await.unwrap();
        let prompt = llm.prompts.lock().unwrap()[0].clone();
        assert!(prompt.contains(mxr_llm::UNTRUSTED_MAIL_GUARD));
        // The guard in the system prompt names the markers too; look in the
        // user message only.
        let user = &prompt[prompt.find("Account owner addresses").unwrap()..];
        let begin = user.find(mxr_llm::UNTRUSTED_MAIL_BEGIN).unwrap();
        let end = user.find(mxr_llm::UNTRUSTED_MAIL_END).unwrap();
        let body = user.find(ASK_SENTENCE).unwrap();
        assert!(begin < body && body < end);
    }

    #[tokio::test]
    async fn a_cloud_model_without_opt_in_gets_this_thread_only() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.llm.base_url = "https://api.example-cloud.com/v1".into();
        config.llm.allow_cloud_relationship_data = false;
        state.set_config_for_test(config.clone()).await;
        let (thread_id, message_id) = seed_thread(&state).await;
        let llm = ScriptedLlm::at(
            answer_for(&message_id),
            Some("https://api.example-cloud.com/v1"),
        );
        state.llm.replace(llm.clone());

        let cloud = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        let provenance = cloud.provenance.unwrap();
        assert_eq!(provenance.locality, AiLocalityData::Cloud);
        assert_eq!(provenance.sources, vec![AiSourceData::ThisThread]);
        assert!(!llm.prompts.lock().unwrap()[0].contains("History with"));

        // With the opt-in, the same thread carries the history line.
        config.llm.allow_cloud_relationship_data = true;
        state.set_config_for_test(config).await;
        state.llm.replace(llm.clone());
        let shared = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert!(
            !shared.from_cache,
            "the privacy setting is part of the cache key"
        );
        assert_eq!(
            shared.provenance.unwrap().sources,
            vec![AiSourceData::ThisThread, AiSourceData::RelationshipHistory]
        );
        assert!(llm.prompts.lock().unwrap()[1].contains("History with maya@example.com"));
    }

    #[tokio::test]
    async fn a_gist_written_from_snippets_is_redone_once_bodies_sync() {
        let state = AppState::in_memory().await.unwrap();
        let thread_id = ThreadId::new();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(state.default_account_id())
            .thread_id(thread_id.clone())
            .provider_id("snippet-1")
            .sender_address("Maya Ortiz", "maya@example.com")
            .snippet("Can you confirm who owns it?")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        let llm = ScriptedLlm::new(answer_for(&envelope.id));
        state.llm.replace(llm.clone());

        gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        let again = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert!(again.from_cache, "no body yet: the snippet gist stands");
        assert_eq!(llm.calls.load(Ordering::SeqCst), 1);

        state
            .store
            .insert_body(&MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some(format!("Hi Maya here.\n{ASK_SENTENCE}")),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            })
            .await
            .unwrap();
        let fresh = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert!(!fresh.from_cache, "the body arrived: regenerate");
        assert_eq!(llm.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            fresh.ask.and_then(|ask| ask.quote).map(|quote| quote.text),
            Some(ASK_SENTENCE.to_string())
        );
    }

    /// What the prompt may carry follows the endpoint actually called, not
    /// the config: a cloud provider behind a config that still says local
    /// gets no history, and moving the same model from local to cloud never
    /// serves the cached local gist.
    #[tokio::test]
    async fn the_called_endpoint_decides_history_and_the_cache() {
        let state = AppState::in_memory().await.unwrap();
        let (thread_id, message_id) = seed_thread(&state).await;
        let local = ScriptedLlm::at(answer_for(&message_id), Some("http://localhost:11434/v1"));
        state.llm.replace(local.clone());
        let first = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert_eq!(first.provenance.unwrap().locality, AiLocalityData::Local);

        // Config untouched (local by default, no cloud opt-in); the runtime
        // now serves the same model from the cloud.
        let cloud = ScriptedLlm::at(
            answer_for(&message_id),
            Some("https://api.example-cloud.com/v1"),
        );
        state.llm.replace(cloud.clone());
        let second = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert!(!second.from_cache, "the endpoint is part of the cache key");
        let provenance = second.provenance.unwrap();
        assert_eq!(provenance.locality, AiLocalityData::Cloud);
        assert_eq!(provenance.sources, vec![AiSourceData::ThisThread]);
        assert!(!cloud.prompts.lock().unwrap()[0].contains("History with"));
    }

    #[tokio::test]
    async fn no_model_is_a_status_not_an_error() {
        let state = AppState::in_memory().await.unwrap();
        let (thread_id, _) = seed_thread(&state).await;
        let answer = gist(get_thread_gist(&state, &thread_id, false).await.unwrap());
        assert_eq!(answer.status, ThreadGistStatusData::Disabled);
        assert_eq!(answer.gist, None);
    }
}
