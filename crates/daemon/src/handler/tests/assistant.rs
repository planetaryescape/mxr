//! The assistant end to end: real handlers, the real MCP tools in-process,
//! and a scripted model. Changes wait for the user and run only on approval.

use super::desk::{request, Fixture, ME};
use super::*;
use async_trait::async_trait;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_llm::{
    CallKind, CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
    ToolCall, ToolChatRequest, ToolChatResponse,
};
use mxr_protocol::{
    AssistantCallStateData, AssistantConversationData, AssistantDecisionData,
    AssistantDecisionKindData, AssistantItemData, AssistantPendingData, AssistantStatusData,
    AssistantViewContextData,
};
use mxr_provider_fake::RecordedMutation;
use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;

struct ScriptedLlm {
    replies: StdMutex<VecDeque<ToolChatResponse>>,
}

impl ScriptedLlm {
    fn new(replies: Vec<ToolChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            replies: StdMutex::new(replies.into()),
        })
    }
}

#[async_trait]
impl LlmProvider for ScriptedLlm {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::Other("not scripted".into()))
    }
    async fn chat_as(
        &self,
        _req: ToolChatRequest,
        _kind: CallKind,
    ) -> Result<ToolChatResponse, LlmError> {
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| LlmError::Other("script ran out".into()))
    }
    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 100_000,
            supports_streaming: false,
        }
    }
    fn model_name(&self) -> &str {
        "scripted"
    }
}

fn reply(text: &str, calls: Vec<(&str, &str, serde_json::Value)>) -> ToolChatResponse {
    ToolChatResponse {
        text: text.into(),
        tool_calls: calls
            .into_iter()
            .map(|(id, name, arguments)| ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            })
            .collect(),
        model: "scripted".into(),
        finish_reason: None,
        usage: None,
        provider_turn: None,
    }
}

async fn settled(fx: &Fixture, conversation_id: &str) -> AssistantConversationData {
    for _ in 0..200 {
        let ResponseData::AssistantConversation { conversation } = request(
            fx,
            Request::GetAssistantConversation {
                conversation_id: conversation_id.to_string(),
            },
        )
        .await
        else {
            panic!("expected a conversation");
        };
        if conversation.status != AssistantStatusData::Running {
            return conversation;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the turn never settled");
}

fn stars(fx: &Fixture) -> usize {
    fx.fake
        .mutations()
        .iter()
        .filter(|m| matches!(m, RecordedMutation::StarredSet { .. }))
        .count()
}

async fn send(fx: &Fixture, message: &str) -> String {
    match request(
        fx,
        Request::AssistantSend {
            conversation_id: None,
            message: message.into(),
            context: AssistantViewContextData::default(),
        },
    )
    .await
    {
        ResponseData::AssistantConversation { conversation } => conversation.id,
        other => panic!("expected a conversation, got {other:?}"),
    }
}

async fn decide(
    fx: &Fixture,
    conversation_id: &str,
    call_id: &str,
    decision: AssistantDecisionKindData,
) {
    let response = request(
        fx,
        Request::AssistantDecide {
            conversation_id: conversation_id.into(),
            decisions: vec![AssistantDecisionData {
                call_id: call_id.into(),
                decision,
            }],
        },
    )
    .await;
    assert!(matches!(
        response,
        ResponseData::AssistantConversation { .. }
    ));
}

#[tokio::test]
async fn a_change_waits_for_approval_then_runs_exactly_what_was_shown() {
    let fx = Fixture::new().await;
    let message = fx
        .message(
            &ThreadId::new(),
            "bank@example.com",
            ME,
            Duration::hours(2),
            None,
        )
        .await;
    let star = serde_json::json!({
        "action": "star",
        "message_ids": [message.id.as_str()],
        "confirm": true,
        "reason": "Star the bank email you asked about."
    });
    fx.state.llm.replace(ScriptedLlm::new(vec![
        reply("", vec![("c1", "mxr_mutate", star)]),
        reply("Starred it.", vec![]),
    ]));

    let id = send(&fx, "star the bank email").await;
    let waiting = settled(&fx, &id).await;
    assert_eq!(waiting.status, AssistantStatusData::AwaitingUser);
    assert_eq!(stars(&fx), 0, "nothing runs before approval");
    let AssistantPendingData::Approval {
        call_id,
        tool,
        reason,
        preview,
        ..
    } = &waiting.pending[0]
    else {
        panic!("expected an approval, got {:?}", waiting.pending);
    };
    assert_eq!(tool, "mxr_mutate");
    assert_eq!(reason, "Star the bank email you asked about.");
    let preview = preview.as_ref().expect("a preview of the messages");
    assert!(
        preview.to_string().contains(&message.id.as_str()),
        "{preview}"
    );

    decide(&fx, &id, call_id, AssistantDecisionKindData::Approve).await;
    let done = settled(&fx, &id).await;
    assert_eq!(done.status, AssistantStatusData::Idle);
    assert_eq!(stars(&fx), 1);
    assert!(done.pending.is_empty());
    assert!(done.items.iter().any(|item| matches!(
        item,
        AssistantItemData::ToolCall {
            state: AssistantCallStateData::Approved,
            ..
        }
    )));
    assert!(matches!(
        done.items.last(),
        Some(AssistantItemData::Assistant { text, .. }) if text == "Starred it."
    ));
    assert!(matches!(
        done.items.first(),
        Some(AssistantItemData::User { text, .. }) if text == "star the bank email"
    ));

    // The same approval can't be replayed.
    let replay = handle_request(
        &fx.state,
        &IpcMessage {
            expected_workspace: None,
            id: 9,
            source: ClientKind::Web,
            payload: IpcPayload::Request(Request::AssistantDecide {
                conversation_id: id.clone(),
                decisions: vec![AssistantDecisionData {
                    call_id: call_id.clone(),
                    decision: AssistantDecisionKindData::Approve,
                }],
            }),
        },
    )
    .await;
    assert!(matches!(
        replay.payload,
        IpcPayload::Response(Response::Error { .. })
    ));
    assert_eq!(stars(&fx), 1);
}

#[tokio::test]
async fn a_rejected_change_never_reaches_the_provider() {
    let fx = Fixture::new().await;
    let message = fx
        .message(
            &ThreadId::new(),
            "news@example.com",
            ME,
            Duration::hours(1),
            None,
        )
        .await;
    fx.state.llm.replace(ScriptedLlm::new(vec![
        reply(
            "",
            vec![(
                "c1",
                "mxr_mutate",
                serde_json::json!({"action": "star", "message_ids": [message.id.as_str()], "confirm": true}),
            )],
        ),
        reply("Left it alone.", vec![]),
    ]));
    let id = send(&fx, "star it").await;
    settled(&fx, &id).await;
    decide(
        &fx,
        &id,
        "c1",
        AssistantDecisionKindData::Reject {
            reason: Some("not that one".into()),
        },
    )
    .await;
    let done = settled(&fx, &id).await;
    assert_eq!(stars(&fx), 0);
    assert!(done.items.iter().any(|item| matches!(
        item,
        AssistantItemData::ToolCall {
            state: AssistantCallStateData::Declined,
            ..
        }
    )));
}

#[tokio::test]
async fn agent_clients_cannot_drive_the_assistant() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .insert("mcp".into(), mxr_config::AgentProfileConfig::default());
    fx.state.set_config_for_test(config).await;
    let response = handle_request(
        &fx.state,
        &IpcMessage {
            expected_workspace: None,
            id: 3,
            source: ClientKind::Mcp,
            payload: IpcPayload::Request(Request::AssistantSend {
                conversation_id: None,
                message: "archive everything".into(),
                context: AssistantViewContextData::default(),
            }),
        },
    )
    .await;
    let IpcPayload::Response(Response::Error { message, .. }) = response.payload else {
        panic!("expected a refusal, got {:?}", response.payload);
    };
    assert!(
        message.contains("can't drive the in-app assistant"),
        "{message}"
    );
}

#[tokio::test]
async fn without_a_model_nothing_is_written() {
    let fx = Fixture::new().await;
    let response = handle_request(
        &fx.state,
        &IpcMessage {
            expected_workspace: None,
            id: 4,
            source: ClientKind::Web,
            payload: IpcPayload::Request(Request::AssistantSend {
                conversation_id: None,
                message: "hello".into(),
                context: AssistantViewContextData::default(),
            }),
        },
    )
    .await;
    assert!(matches!(
        response.payload,
        IpcPayload::Response(Response::Error { .. })
    ));
    let ResponseData::AssistantConversations { conversations } =
        request(&fx, Request::ListAssistantConversations { limit: None }).await
    else {
        panic!("expected a list");
    };
    assert!(conversations.is_empty());
}

#[tokio::test]
async fn a_trusted_conversation_stars_without_asking() {
    let fx = Fixture::new().await;
    let message = fx
        .message(
            &ThreadId::new(),
            "news@example.com",
            ME,
            Duration::hours(1),
            None,
        )
        .await;
    let star = || serde_json::json!({"action": "star", "message_ids": [message.id.as_str()], "confirm": true});
    fx.state.llm.replace(ScriptedLlm::new(vec![
        reply("", vec![("c1", "mxr_mutate", star())]),
        reply("Left it.", vec![]),
        reply("", vec![("c2", "mxr_mutate", star())]),
        reply("Starred.", vec![]),
    ]));
    let ResponseData::AssistantConversation { conversation } = request(
        &fx,
        Request::AssistantSend {
            conversation_id: None,
            message: "hello".into(),
            context: AssistantViewContextData::default(),
        },
    )
    .await
    else {
        panic!("expected a conversation");
    };
    let id = conversation.id;
    // The first turn starts before trust is set, so it asks.
    let first = settled(&fx, &id).await;
    assert_eq!(first.status, AssistantStatusData::AwaitingUser);
    decide(
        &fx,
        &id,
        "c1",
        AssistantDecisionKindData::Reject { reason: None },
    )
    .await;
    settled(&fx, &id).await;

    let ResponseData::AssistantConversation { conversation } = request(
        &fx,
        Request::SetAssistantAutoApprove {
            conversation_id: id.clone(),
            enabled: true,
        },
    )
    .await
    else {
        panic!("expected a conversation");
    };
    assert!(conversation.auto_approve);
    request(
        &fx,
        Request::AssistantSend {
            conversation_id: Some(id.clone()),
            message: "star it".into(),
            context: AssistantViewContextData::default(),
        },
    )
    .await;
    let done = settled(&fx, &id).await;
    assert_eq!(stars(&fx), 1, "a reversible change ran without asking");
    assert!(done.items.iter().any(|item| matches!(
        item,
        AssistantItemData::ToolCall {
            state: AssistantCallStateData::AutoApproved,
            ..
        }
    )));
}

/// Seeds a conversation whose one step (`call1`) stored a search result,
/// and whose second step (`call2`) is still waiting for its result.
async fn seeded_step(fx: &Fixture, id: &str, listed: usize) {
    let store = &fx.state.store;
    store
        .create_assistant_conversation(id, "steps")
        .await
        .unwrap();
    let call = |call_id: &str| ToolCall {
        id: call_id.into(),
        name: "mxr_search".into(),
        arguments: serde_json::json!({}),
    };
    let results: Vec<_> = (0..listed)
        .map(|n| serde_json::json!({"subject": format!("Mail {n}"), "thread_id": format!("t{n}")}))
        .collect();
    let messages = [
        mxr_llm::ToolChatMessage::Assistant {
            text: String::new(),
            tool_calls: vec![call("call1"), call("call2")],
            provider_turn: None,
        },
        mxr_llm::ToolChatMessage::Tool {
            call_id: "call1".into(),
            content: serde_json::json!({"kind": "Search", "results": results, "total": 14})
                .to_string(),
            is_error: false,
        },
    ];
    for message in messages {
        store
            .append_assistant_message(id, &serde_json::to_string(&message).unwrap(), None)
            .await
            .unwrap();
    }
}

async fn step_rows(fx: &Fixture, conversation_id: &str, call_id: &str) -> Response {
    let reply = handle_request(
        &fx.state,
        &IpcMessage {
            expected_workspace: None,
            id: 3,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::GetAssistantStepRows {
                conversation_id: conversation_id.into(),
                call_id: call_id.into(),
            }),
        },
    )
    .await;
    match reply.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

#[tokio::test]
async fn a_step_hands_over_every_row_its_result_lists() {
    let fx = Fixture::new().await;
    seeded_step(&fx, "asst_steps", 12).await;

    let Response::Ok {
        data:
            ResponseData::AssistantStepRows {
                rows,
                listed,
                total,
            },
    } = step_rows(&fx, "asst_steps", "call1").await
    else {
        panic!("expected the step's rows");
    };
    assert_eq!(rows.len(), 12);
    assert_eq!(rows[0].title, "Mail 0");
    assert_eq!(rows[11].title, "Mail 11");
    assert_eq!(listed, 12);
    assert_eq!(total, Some(14));

    // A step still running has no result yet, so it has nothing to show.
    let Response::Ok {
        data:
            ResponseData::AssistantStepRows {
                rows,
                listed,
                total,
            },
    } = step_rows(&fx, "asst_steps", "call2").await
    else {
        panic!("expected an empty step");
    };
    assert!(rows.is_empty());
    assert_eq!(listed, 0);
    assert_eq!(total, None);
}

#[tokio::test]
async fn an_unknown_step_or_conversation_is_an_error() {
    let fx = Fixture::new().await;
    seeded_step(&fx, "asst_steps", 3).await;

    assert!(matches!(
        step_rows(&fx, "asst_steps", "call9").await,
        Response::Error { .. }
    ));
    assert!(matches!(
        step_rows(&fx, "asst_missing", "call1").await,
        Response::Error { .. }
    ));
}

#[tokio::test]
async fn a_step_hands_over_at_most_500_rows() {
    let fx = Fixture::new().await;
    seeded_step(&fx, "asst_big", 600).await;

    let Response::Ok {
        data:
            ResponseData::AssistantStepRows {
                rows,
                listed,
                total,
            },
    } = step_rows(&fx, "asst_big", "call1").await
    else {
        panic!("expected the step's rows");
    };
    assert_eq!(rows.len(), 500);
    assert_eq!(listed, 500);
    assert_eq!(total, Some(600));
}
