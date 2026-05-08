//! Draft assist: generate a reply grounded on the thread context plus
//! a hand-tuned system prompt. Future improvement: retrieve top-K
//! similar prior sent messages from the user's corpus and inject as
//! few-shot examples to ground the generated voice. The current
//! version keeps it simple — thread context + the user's instruction.

use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError};
use mxr_protocol::ResponseData;

const SYSTEM_PROMPT: &str = "You draft email replies for a busy professional. Given the thread \
context and the user's intent, produce just the reply body — no \
greeting line if the thread is mid-conversation, no signature, no \
subject line. Be direct, concise, plain prose. Match the formality \
and length of the thread you're replying to. If the user's instruction \
is short, lean toward shorter replies. Never add commentary about \
what you're doing — just write the reply.";

const PROMPT_BUDGET_CHARS: usize = 24_000;

pub(super) async fn draft_assist(
    state: &AppState,
    thread_id: &ThreadId,
    instruction: &str,
) -> HandlerResult {
    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    if envelopes.is_empty() {
        return Err(format!("Thread {} has no messages to reply to", thread_id));
    }

    // Build a transcript for the model. Most recent last so the LLM's
    // attention is closest to the message it's actually replying to.
    let mut transcript = String::new();
    for env in &envelopes {
        let from = env.from.name.as_deref().unwrap_or(env.from.email.as_str());
        let body = match state.store.get_body(&env.id).await {
            Ok(Some(body)) => body
                .text_plain
                .or(body.text_html)
                .unwrap_or_else(|| env.snippet.clone()),
            _ => env.snippet.clone(),
        };
        transcript.push_str(&format!("--- {from} ---\n{body}\n\n"));
        if transcript.len() > PROMPT_BUDGET_CHARS {
            transcript.truncate(PROMPT_BUDGET_CHARS);
            transcript.push_str("\n[...thread truncated...]\n");
            break;
        }
    }

    let user_message = format!(
        "Thread so far:\n\n{transcript}\n\
         Now draft my reply. Instruction: {}",
        instruction.trim()
    );

    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(user_message),
        ],
        max_tokens: Some(600),
        temperature: Some(0.4),
    };

    match state.llm.complete(request).await {
        Ok(response) => Ok(ResponseData::DraftSuggestion {
            body: response.content.trim().to_string(),
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
