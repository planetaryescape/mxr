//! The in-app assistant inside the daemon: one background task per running
//! turn, the tools it calls (mxr's MCP tools, in-process), and the store
//! that keeps every step.
//!
//! The loop and its gate live in `mxr-assistant`; this module supplies the
//! model, the tools and the transcript, and tells clients what changed.

mod host;
mod links;
mod models;
mod readout;
mod titles;
mod view;

pub(crate) use models::{models, set_model};
pub(crate) use view::{conversation_view, summary_view};

use crate::state::AppState;
use async_trait::async_trait;
use mxr_assistant::{
    prompt, Agent, ChatModel, Decision, Limits, MessageMeta, PendingItem, Progress, Summary,
    TurnOutcome, TurnSink, ViewContext,
};
use mxr_llm::{LlmError, LlmFeature, ToolChatMessage, ToolChatRequest, ToolChatResponse};
use mxr_protocol::{
    AssistantProgressData, AssistantStatusData, AssistantViewContextData, DaemonEvent,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often the reply being written is sent to clients. Fast enough to
/// read as typing, slow enough not to flood the event channel.
const WRITING_EVERY: Duration = Duration::from_millis(120);
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct AssistantRuntime {
    /// Conversations with a turn running now, and how to stop each.
    running: Mutex<HashMap<String, CancellationToken>>,
    /// Started on first use; the tool list is fixed for the daemon's life.
    tools: tokio::sync::OnceCell<Arc<host::McpHost>>,
}

impl AssistantRuntime {
    pub(crate) fn is_running(&self, conversation_id: &str) -> bool {
        self.lock().contains_key(conversation_id)
    }

    /// Ask a running turn to stop. False when none is running.
    pub(crate) fn cancel(&self, conversation_id: &str) -> bool {
        match self.lock().get(conversation_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn begin(&self, conversation_id: &str) -> Result<CancellationToken, String> {
        let mut running = self.lock();
        if running.contains_key(conversation_id) {
            return Err(
                "the assistant is still working on this conversation; stop it first".into(),
            );
        }
        let token = CancellationToken::new();
        running.insert(conversation_id.to_string(), token.clone());
        Ok(token)
    }

    fn finish(&self, conversation_id: &str) {
        self.lock().remove(conversation_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn tools(&self, state: &Arc<AppState>) -> Result<Arc<host::McpHost>, String> {
        self.tools
            .get_or_try_init(|| host::McpHost::start(state))
            .await
            .cloned()
    }
}

/// How a turn starts.
pub(crate) enum TurnStart {
    Send {
        message: String,
        context: AssistantViewContextData,
    },
    Decide {
        decisions: HashMap<String, Decision>,
    },
}

/// Mark the conversation running and run the turn in the background.
/// `pending` is what was waiting before this turn; it is cleared in the
/// store first, so a crash mid-turn can't leave an approval that would run
/// a second time.
pub(crate) async fn start_turn(
    state: &Arc<AppState>,
    conversation_id: &str,
    pending: Vec<PendingItem>,
    start: TurnStart,
) -> Result<(), String> {
    let cancel = state.assistant.begin(conversation_id)?;
    if let Err(error) = state
        .store
        .set_assistant_state(conversation_id, "running", None, None, None)
        .await
    {
        state.assistant.finish(conversation_id);
        return Err(error.to_string());
    }
    emit(state, conversation_id, AssistantStatusData::Running, None);

    let state = state.clone();
    let conversation_id = conversation_id.to_string();
    tokio::spawn(async move {
        let outcome = run_turn(&state, &conversation_id, pending, start, cancel).await;
        let settled = settle(&state, &conversation_id, &outcome).await;
        state.assistant.finish(&conversation_id);
        emit(&state, &conversation_id, settled, None);
    });
    Ok(())
}

async fn run_turn(
    state: &Arc<AppState>,
    conversation_id: &str,
    pending: Vec<PendingItem>,
    start: TurnStart,
    cancel: CancellationToken,
) -> TurnOutcome {
    let mut history = match load_history(state, conversation_id).await {
        Ok(history) => history,
        Err(error) => return TurnOutcome::Failed(error),
    };
    let tools = match state.assistant.tools(state).await {
        Ok(tools) => tools,
        Err(error) => return TurnOutcome::Failed(format!("mxr's tools didn't start: {error}")),
    };
    let row = match state
        .store
        .get_assistant_conversation(conversation_id)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return TurnOutcome::Failed("the conversation is gone".to_string()),
        Err(error) => return TurnOutcome::Failed(error.to_string()),
    };
    // Message seqs start at 1 and have no gaps, so the summary that covers
    // through seq N replaces the first N messages.
    let summary = match (row.summary, row.summary_through_seq) {
        (Some(text), Some(through)) => usize::try_from(through)
            .ok()
            .filter(|through| *through <= history.len())
            .map(|through| Summary { text, through }),
        _ => None,
    };
    let model = PinnedModel(state.llm.for_feature(LlmFeature::Assistant).pin());
    let sink = StoreSink {
        state,
        conversation_id,
        last_writing: Mutex::new(None),
    };
    let agent = Agent {
        model: &model,
        host: tools.as_ref(),
        sink: &sink,
        cancel,
        limits: Limits::default(),
        auto_approve: row.auto_approve,
        summary: std::sync::Mutex::new(summary),
    };
    let outcome = match start {
        TurnStart::Send { message, context } => {
            let now = chrono::Local::now().to_rfc3339();
            let content = prompt::user_message(&message, &now, &view_context(context));
            agent.send(&mut history, &pending, content).await
        }
        TurnStart::Decide { decisions } => agent
            .resume(&mut history, pending, decisions)
            .await
            .unwrap_or_else(TurnOutcome::Failed),
    };
    if let TurnOutcome::Failed(error) = &outcome {
        tracing::warn!(conversation_id, %error, "assistant turn failed");
    }
    outcome
}

/// Record how the turn ended and return the conversation's new status.
async fn settle(
    state: &AppState,
    conversation_id: &str,
    outcome: &TurnOutcome,
) -> AssistantStatusData {
    let (status, last_outcome, error, pending) = match outcome {
        TurnOutcome::Finished => (AssistantStatusData::Idle, Some("finished"), None, None),
        TurnOutcome::AwaitingUser(pending) => (
            AssistantStatusData::AwaitingUser,
            None,
            None,
            serde_json::to_string(pending).ok(),
        ),
        TurnOutcome::StepLimit => (AssistantStatusData::Idle, Some("step_limit"), None, None),
        TurnOutcome::Cancelled => (AssistantStatusData::Idle, Some("cancelled"), None, None),
        TurnOutcome::Failed(error) => (
            AssistantStatusData::Idle,
            Some("failed"),
            Some(error.as_str()),
            None,
        ),
    };
    let status_key = match status {
        AssistantStatusData::Idle => "idle",
        AssistantStatusData::Running => "running",
        AssistantStatusData::AwaitingUser => "awaiting_user",
    };
    if let Err(store_error) = state
        .store
        .set_assistant_state(
            conversation_id,
            status_key,
            last_outcome,
            error,
            pending.as_deref(),
        )
        .await
    {
        tracing::error!(conversation_id, error = %store_error, "could not record assistant turn outcome");
    }
    status
}

/// Close calls that were waiting without running them or calling the
/// model: the user stopped the conversation while it waited.
pub(crate) async fn abandon_pending(
    state: &Arc<AppState>,
    conversation_id: &str,
    pending: &[PendingItem],
) -> Result<(), String> {
    for item in pending {
        let message = mxr_assistant::cancelled_result(item.call_id.clone());
        let json = serde_json::to_string(&message).map_err(|error| error.to_string())?;
        state
            .store
            .append_assistant_message(conversation_id, &json, None)
            .await
            .map_err(|error| error.to_string())?;
    }
    state
        .store
        .set_assistant_state(conversation_id, "idle", Some("cancelled"), None, None)
        .await
        .map_err(|error| error.to_string())?;
    emit(state, conversation_id, AssistantStatusData::Idle, None);
    Ok(())
}

pub(crate) async fn load_history(
    state: &AppState,
    conversation_id: &str,
) -> Result<Vec<ToolChatMessage>, String> {
    state
        .store
        .assistant_messages(conversation_id)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|row| {
            serde_json::from_str(&row.message_json)
                .map_err(|error| format!("message {} is unreadable: {error}", row.seq))
        })
        .collect()
}

pub(crate) fn parse_pending(pending_json: Option<&str>) -> Vec<PendingItem> {
    pending_json
        .and_then(|json| match serde_json::from_str(json) {
            Ok(pending) => Some(pending),
            Err(error) => {
                tracing::warn!(%error, "unreadable pending assistant calls");
                None
            }
        })
        .unwrap_or_default()
}

fn view_context(context: AssistantViewContextData) -> ViewContext {
    ViewContext {
        view: context.view,
        open_thread_id: context.open_thread_id,
        open_message_id: context.open_message_id,
        selected_message_ids: context.selected_message_ids,
        search_query: context.search_query,
    }
}

pub(crate) fn emit(
    state: &AppState,
    conversation_id: &str,
    status: AssistantStatusData,
    progress: Option<AssistantProgressData>,
) {
    crate::chimes::emit_daemon_event(
        state,
        DaemonEvent::AssistantUpdated {
            conversation_id: conversation_id.to_string(),
            status,
            progress,
        },
    );
}

struct PinnedModel(mxr_llm::PinnedLlm);

#[async_trait]
impl ChatModel for PinnedModel {
    async fn chat(
        &self,
        request: ToolChatRequest,
        on_text: &(dyn for<'t> Fn(&'t str) + Send + Sync),
    ) -> Result<ToolChatResponse, LlmError> {
        self.0.chat_stream(request, on_text).await
    }

    fn context_window(&self) -> Option<u32> {
        Some(self.0.context_window()).filter(|window| *window > 0)
    }

    fn leaves_machine(&self) -> Option<bool> {
        Some(self.0.leaves_machine())
    }
}

struct StoreSink<'a> {
    state: &'a Arc<AppState>,
    conversation_id: &'a str,
    /// When the reply being written was last sent to clients.
    last_writing: Mutex<Option<Instant>>,
}

#[async_trait]
impl TurnSink for StoreSink<'_> {
    async fn append(&self, message: &ToolChatMessage, meta: &MessageMeta) -> Result<(), String> {
        let json = serde_json::to_string(message).map_err(|error| error.to_string())?;
        let meta_json = if *meta == MessageMeta::default() {
            None
        } else {
            Some(serde_json::to_string(meta).map_err(|error| error.to_string())?)
        };
        self.state
            .store
            .append_assistant_message(self.conversation_id, &json, meta_json.as_deref())
            .await
            .map_err(|error| format!("could not save the conversation: {error}"))?;
        emit(
            self.state,
            self.conversation_id,
            AssistantStatusData::Running,
            None,
        );
        Ok(())
    }

    async fn progress(&self, progress: Progress) {
        let progress = match progress {
            Progress::Thinking { step } => AssistantProgressData::Thinking { step },
            Progress::Calling {
                tool,
                reason,
                arguments,
            } => AssistantProgressData::Calling {
                title: Some(titles::running_title(&tool, &arguments)),
                tool,
                reason,
            },
            Progress::Summarizing => AssistantProgressData::Summarizing,
        };
        emit(
            self.state,
            self.conversation_id,
            AssistantStatusData::Running,
            Some(progress),
        );
    }

    fn writing(&self, text: &str) {
        {
            let mut last = self
                .last_writing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if last.is_some_and(|at| at.elapsed() < WRITING_EVERY) {
                return;
            }
            *last = Some(Instant::now());
        }
        emit(
            self.state,
            self.conversation_id,
            AssistantStatusData::Running,
            Some(AssistantProgressData::Writing {
                text: text.to_string(),
            }),
        );
    }

    async fn summarized(&self, summary: &Summary) -> Result<(), String> {
        let through = i64::try_from(summary.through).map_err(|error| error.to_string())?;
        self.state
            .store
            .set_assistant_summary(self.conversation_id, &summary.text, through)
            .await
            .map_err(|error| format!("could not save the summary: {error}"))
    }
}
