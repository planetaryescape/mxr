//! Thread summarisation. Pulls a thread + bodies from the store,
//! constructs a chat-style prompt, and asks the configured LLM for a
//! 2-3 sentence summary focused on what's actionable.

use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError};
use mxr_protocol::ResponseData;

const SYSTEM_PROMPT: &str =
    "You summarise email threads in 2 to 3 short sentences for a busy reader. \
Focus on what's actionable: who is asking what, when something is due, \
what the user needs to decide. Skip pleasantries. Refer to the user as \
\"you\". Use plain language; no bullet points unless the original thread \
already structures the content that way.";

/// Maximum thread-content length we feed into the prompt. The
/// configured model's context window bounds this; if a thread is
/// longer we truncate and add a [...] marker rather than refusing.
const PROMPT_BUDGET_CHARS: usize = 24_000;

pub(super) async fn summarize_thread(state: &AppState, thread_id: &ThreadId) -> HandlerResult {
    let thread = state
        .store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Thread {} not found", thread_id))?;
    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    if envelopes.is_empty() {
        return Err(format!("Thread {} has no messages", thread_id));
    }

    let mut prompt_body = format!("Subject: {}\n\n", thread.subject);
    for env in &envelopes {
        let from = env.from.name.as_deref().unwrap_or(env.from.email.as_str());
        let date = env
            .date
            .with_timezone(&chrono::Local)
            .format("%a %b %e %H:%M");
        // Body is optional; fall back to the snippet when not yet stored.
        let body = match state.store.get_body(&env.id).await {
            Ok(Some(body)) => body
                .text_plain
                .or(body.text_html)
                .unwrap_or_else(|| env.snippet.clone()),
            _ => env.snippet.clone(),
        };
        prompt_body.push_str(&format!("--- {from} ({date}) ---\n{body}\n\n"));
        if prompt_body.len() > PROMPT_BUDGET_CHARS {
            prompt_body.truncate(PROMPT_BUDGET_CHARS);
            prompt_body.push_str("\n[...thread truncated...]\n");
            break;
        }
    }

    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(prompt_body),
        ],
        max_tokens: Some(220),
        temperature: Some(0.2),
    };

    match state.llm.complete(request).await {
        Ok(response) => Ok(ResponseData::ThreadSummary {
            text: response.content.trim().to_string(),
            model: response.model,
        }),
        Err(LlmError::Disabled) => Err(
            "LLM is disabled. Enable it in [llm] in your config and configure a model \
             (Ollama / LM Studio / OpenAI). See `mxr config`."
                .to_string(),
        ),
        Err(e) => Err(format!("LLM error: {e}")),
    }
}
