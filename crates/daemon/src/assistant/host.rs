//! The assistant's tools: mxr's MCP server, run in-process. Its requests
//! go through the daemon's normal dispatch as `ClientKind::Assistant`, so
//! the global safety policy, workspace ownership checks and the activity
//! log apply exactly as they do for any client.

use crate::state::AppState;
use async_trait::async_trait;
use mxr_assistant::{ToolHost, ToolOutput};
use mxr_llm::ToolSpec;
use mxr_mcp::{DaemonRequester, InProcessTools};
use mxr_protocol::{ArchiveWorkspaceData, ClientKind, IpcMessage, IpcPayload, Request, Response};
use std::sync::{Arc, Weak};

pub(crate) struct McpHost {
    tools: InProcessTools,
    specs: Vec<ToolSpec>,
}

impl McpHost {
    pub(super) async fn start(state: &Arc<AppState>) -> Result<Arc<Self>, String> {
        Self::start_scoped(state, None).await
    }

    pub(super) async fn start_scoped(
        state: &Arc<AppState>,
        proof: Option<crate::handler::remote::AdmissionProof>,
    ) -> Result<Arc<Self>, String> {
        let requester = Arc::new(InProcessRequester {
            state: Arc::downgrade(state),
            proof,
        });
        let tools = InProcessTools::start(requester)
            .await
            .map_err(|error| error.to_string())?;
        let specs = tools
            .tools()
            .iter()
            .map(|tool| ToolSpec {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            })
            .collect();
        Ok(Arc::new(Self { tools, specs }))
    }
}

#[async_trait]
impl ToolHost for McpHost {
    fn tools(&self) -> &[ToolSpec] {
        &self.specs
    }

    async fn call(&self, name: &str, arguments: serde_json::Value) -> ToolOutput {
        let outcome = self.tools.call(name, arguments).await;
        ToolOutput {
            content: outcome.content,
            is_error: outcome.is_error,
        }
    }
}

/// Holds the daemon weakly: the runtime that owns this lives in the state.
#[derive(Debug)]
struct InProcessRequester {
    state: Weak<AppState>,
    proof: Option<crate::handler::remote::AdmissionProof>,
}

impl InProcessRequester {
    async fn dispatch(
        &self,
        request: Request,
        expected_workspace: Option<ArchiveWorkspaceData>,
    ) -> anyhow::Result<Response> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("the mxr daemon is shutting down"))?;
        let request = match &self.proof {
            Some(proof) => Request::RemoteRead {
                expected_workspace: expected_workspace.clone(),
                client_id: proof.client_id.clone(),
                mail_secret: proof.mail_secret.clone(),
                request: Box::new(request),
            },
            None => request,
        };
        let message = IpcMessage {
            expected_workspace,
            id: 0,
            source: ClientKind::Assistant,
            payload: IpcPayload::Request(request),
        };
        match crate::handler::handle_request(&state, &message)
            .await
            .payload
        {
            IpcPayload::Response(response) => Ok(response),
            _ => anyhow::bail!("the daemon answered with something other than a response"),
        }
    }
}

#[async_trait]
impl DaemonRequester for InProcessRequester {
    async fn request(&self, request: Request) -> anyhow::Result<Response> {
        self.dispatch(request, None).await
    }

    async fn request_owned(
        &self,
        request: Request,
        owner: ArchiveWorkspaceData,
    ) -> anyhow::Result<Response> {
        self.dispatch(request, Some(owner)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Unreachable;

    #[async_trait]
    impl DaemonRequester for Unreachable {
        async fn request(&self, _request: Request) -> anyhow::Result<Response> {
            anyhow::bail!("no daemon in this test")
        }
    }

    #[tokio::test]
    async fn remote_assistant_tools_recheck_the_original_registration_and_never_gain_local_admin() {
        use mxr_core::{RemoteCapability, RemoteSecret};
        use sha2::{Digest, Sha256};
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mail = RemoteSecret::new("a".repeat(64));
        let cleanup = RemoteSecret::new("b".repeat(64));
        let verifier = |secret: &RemoteSecret| {
            RemoteSecret::new(base16ct::lower::encode_string(&Sha256::digest(
                secret.expose().as_bytes(),
            )))
        };
        let now = chrono::Utc::now().timestamp();
        let pairing = state
            .store
            .start_remote_pairing(
                "assistant test",
                &[RemoteCapability::MailRead, RemoteCapability::MailAi],
                &verifier(&mail),
                &verifier(&cleanup),
                now,
            )
            .await
            .unwrap();
        state
            .store
            .approve_remote_pairing(&pairing.request_id, false, now)
            .await
            .unwrap();
        let client = state
            .store
            .claim_remote_pairing(&pairing.request_id, &mail, &cleanup, now)
            .await
            .unwrap();
        let requester = InProcessRequester {
            state: Arc::downgrade(&state),
            proof: Some(crate::handler::remote::AdmissionProof {
                client_id: client.client_id.clone(),
                owner_client_id: client.client_id.clone(),
                mail_secret: mail,
            }),
        };
        assert!(matches!(
            requester.request(Request::Ping).await.unwrap(),
            Response::Ok { .. }
        ));
        for request in [
            Request::Shutdown,
            Request::GetLlmConfig,
            Request::Mutation {
                mutation: mxr_protocol::MutationCommand::SetRead {
                    message_ids: vec![],
                    read: true,
                },
                client_correlation_id: None,
            },
        ] {
            assert!(matches!(
                requester.request(request).await.unwrap(),
                Response::Error { .. }
            ));
        }
        state
            .store
            .revoke_remote_client(&client.client_id, false, now)
            .await
            .unwrap();
        assert!(matches!(
            requester.request(Request::Ping).await.unwrap(),
            Response::Error { .. }
        ));
    }

    /// A tool added to the MCP server must get an explicit gate rule: until
    /// it does, the assistant asks before every call to it, and this fails.
    #[tokio::test]
    async fn every_mcp_tool_has_a_gate_rule() {
        let tools = InProcessTools::start(Arc::new(Unreachable)).await.unwrap();
        let unclassified: Vec<&str> = tools
            .tools()
            .iter()
            .map(|tool| tool.name.as_str())
            .filter(|name| !mxr_assistant::gate::is_classified(name))
            .collect();
        assert!(
            unclassified.is_empty(),
            "add these to RULES in crates/assistant/src/gate.rs: {unclassified:?}"
        );
        assert!(tools.tools().len() > 50);
    }

    /// A tool added to the MCP server must get its own plain-language
    /// titles: until it does, its steps read "Using <tool name>", and this
    /// fails.
    #[tokio::test]
    async fn every_mcp_tool_has_titles() {
        let tools = InProcessTools::start(Arc::new(Unreachable)).await.unwrap();
        let untitled: Vec<&str> = tools
            .tools()
            .iter()
            .map(|tool| tool.name.as_str())
            .filter(|name| !crate::assistant::titles::has_titles(name))
            .collect();
        assert!(
            untitled.is_empty(),
            "add these to `phrases` in crates/daemon/src/assistant/titles.rs: {untitled:?}"
        );
    }
}
