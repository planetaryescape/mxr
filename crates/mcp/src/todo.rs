//! Inputs for the existing daemon task surface.

use mxr_protocol::{TodoStateActionData, TodoStateData};
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Open,
    Done,
    Dismissed,
    Expired,
}

impl From<State> for TodoStateData {
    fn from(state: State) -> Self {
        match state {
            State::Open => Self::Open,
            State::Done => Self::Done,
            State::Dismissed => Self::Dismissed,
            State::Expired => Self::Expired,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Done,
    Dismiss,
    Undo,
}

impl From<Action> for TodoStateActionData {
    fn from(action: Action) -> Self {
        match action {
            Action::Done => Self::Done,
            Action::Dismiss => Self::Dismiss,
            Action::Undo => Self::Undo,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoListInput {
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub state: Option<State>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoGetInput {
    pub todo_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoStateInput {
    pub todo_ids: Vec<String>,
    pub action: Action,
    #[serde(default)]
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionAction {
    Accept,
    Dismiss,
    Undo,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoResolutionInput {
    pub todo_id: String,
    pub suggestion_id: String,
    pub action: ResolutionAction,
    #[serde(default)]
    pub confirm: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DaemonRequester, MxrMcpServer};
    use mxr_protocol::{Request, Response, ResponseData, TodoResolutionActionData};
    use rmcp::handler::server::wrapper::Parameters;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Default)]
    struct Requester(Mutex<Vec<Request>>);

    #[async_trait::async_trait]
    impl DaemonRequester for Requester {
        async fn request(&self, request: Request) -> anyhow::Result<Response> {
            self.0.lock().expect("requests").push(request);
            Ok(Response::Ok {
                data: ResponseData::Ack,
            })
        }
    }

    #[tokio::test]
    async fn state_tools_preview_unless_confirmed_and_preserve_action() {
        let requester = Arc::new(Requester::default());
        let server = MxrMcpServer::from_requester(requester.clone());
        for confirm in [None, Some(false), Some(true)] {
            server
                .todo_set_state(Parameters(TodoStateInput {
                    todo_ids: vec!["todo_cycle".into()],
                    action: Action::Dismiss,
                    confirm,
                }))
                .await
                .expect("tool");
        }
        let requests = requester.0.lock().expect("requests");
        assert!(matches!(
            &requests[0],
            Request::SetTodoState {
                dry_run: true,
                action: TodoStateActionData::Dismiss,
                ..
            }
        ));
        assert!(matches!(
            &requests[1],
            Request::SetTodoState { dry_run: true, .. }
        ));
        assert!(matches!(
            &requests[2],
            Request::SetTodoState { dry_run: false, .. }
        ));
        assert!(serde_json::from_str::<TodoStateInput>(
            r#"{"todo_ids":["a"],"action":"dismiss","confirm":"true"}"#
        )
        .is_err());
    }

    #[tokio::test]
    async fn resolution_tool_preserves_preview_action_and_confirmation_state() {
        let requester = Arc::new(Requester::default());
        let server = MxrMcpServer::from_requester(requester.clone());
        for action in [
            ResolutionAction::Accept,
            ResolutionAction::Dismiss,
            ResolutionAction::Undo,
        ] {
            for confirm in [None, Some(false), Some(true)] {
                server
                    .todo_resolve(Parameters(TodoResolutionInput {
                        todo_id: "todo_cycle".into(),
                        suggestion_id: "offer_cycle".into(),
                        action,
                        confirm,
                    }))
                    .await
                    .expect("tool");
            }
        }
        let requests = requester.0.lock().expect("requests");
        let actions = [
            TodoResolutionActionData::Accept,
            TodoResolutionActionData::Dismiss,
            TodoResolutionActionData::Undo,
        ];
        for (index, expected_action) in actions.into_iter().enumerate() {
            let offset = index * 3;
            for (request, dry_run) in [
                (&requests[offset], true),
                (&requests[offset + 1], true),
                (&requests[offset + 2], false),
            ] {
                assert!(matches!(
                    request,
                    Request::ResolveTodoSuggestion { action, dry_run: actual, .. }
                        if *action == expected_action && *actual == dry_run
                ));
            }
        }
    }
}
