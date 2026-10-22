//! The assistant's tools: mxr's MCP server, run in-process. Its requests
//! go through the daemon's normal dispatch as `ClientKind::Assistant`, so
//! the global safety policy, workspace ownership checks and the activity
//! log apply exactly as they do for any client.

use crate::state::AppState;
use async_trait::async_trait;
use mxr_assistant::{ToolHost, ToolOutput};
use mxr_llm::trace::{Kind, Observation};
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
        let requester = Arc::new(InProcessRequester {
            state: Arc::downgrade(state),
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
        let span = Observation::start(name.to_string(), Kind::Tool);
        span.input(|| arguments.clone());
        let outcome = span.instrument(self.tools.call(name, arguments)).await;
        span.output(|| mxr_llm::trace::text_or_json(&outcome.content));
        if outcome.is_error {
            span.fail(&outcome.content);
        }
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
