use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use mxr_core::id::MessageId;
use mxr_protocol::{
    ClientKind, IpcCodec, IpcMessage, IpcPayload, MutationCommand, Request, Response,
};
use rmcp::{
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json as McpJson, Parameters},
    },
    model::{ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, str::FromStr, sync::Arc};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

#[async_trait]
pub trait DaemonRequester: Send + Sync + std::fmt::Debug + 'static {
    async fn request(&self, request: Request) -> anyhow::Result<Response>;
}

#[derive(Debug, Clone)]
pub struct UnixDaemonRequester {
    socket_path: std::path::PathBuf,
}

impl UnixDaemonRequester {
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }
}

#[async_trait]
impl DaemonRequester for UnixDaemonRequester {
    async fn request(&self, request: Request) -> anyhow::Result<Response> {
        request_over_ipc(&self.socket_path, request).await
    }
}

async fn request_over_ipc(socket_path: &Path, request: Request) -> anyhow::Result<Response> {
    let stream = UnixStream::connect(socket_path).await.map_err(|error| {
        anyhow::anyhow!(
            "Cannot connect to mxr daemon at {}: {}. Start it with: mxr daemon",
            socket_path.display(),
            error
        )
    })?;
    let mut framed = Framed::new(stream, IpcCodec::new());
    framed
        .send(IpcMessage {
            id: 1,
            source: ClientKind::Mcp,
            payload: IpcPayload::Request(request),
        })
        .await?;

    while let Some(frame) = framed.next().await {
        let msg = frame?;
        if msg.id != 1 {
            continue;
        }
        if let IpcPayload::Response(response) = msg.payload {
            return Ok(response);
        }
    }
    anyhow::bail!("mxr daemon closed the IPC connection before responding")
}

#[derive(Debug, Clone)]
pub struct MxrMcpServer {
    requester: Arc<dyn DaemonRequester>,
    tool_router: ToolRouter<Self>,
}

impl MxrMcpServer {
    pub fn new<R: DaemonRequester>(requester: R) -> Self {
        Self::from_requester(Arc::new(requester))
    }

    pub fn from_requester(requester: Arc<dyn DaemonRequester>) -> Self {
        Self {
            requester,
            tool_router: Self::tool_router(),
        }
    }

    async fn daemon_json(&self, request: Request) -> Result<McpJson<Value>, ErrorData> {
        let response = self.requester.request(request).await.map_err(mcp_error)?;
        response_to_json(response).map(McpJson)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MxrMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("First-party mxr MCP server. All tools call the mxr daemon over IPC with source=mcp, so daemon account scoping, agent permissions, activity, dry-run, and send gates still apply. Tools return structured JSON.")
    }
}

#[tool_router(router = tool_router)]
impl MxrMcpServer {
    #[tool(
        name = "mxr_status",
        description = "Return daemon status, accounts, message counts, health, and protocol metadata."
    )]
    pub async fn status(&self) -> Result<McpJson<Value>, ErrorData> {
        self.daemon_json(Request::GetStatus).await
    }

    #[tool(
        name = "mxr_list_messages",
        description = "List message envelopes without body content. Use mxr_read_message with include_body=true to explicitly read bodies."
    )]
    pub async fn list_messages(
        &self,
        Parameters(input): Parameters<ListMessagesInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        self.daemon_json(Request::ListEnvelopes {
            label_id: None,
            account_id: parse_optional_id(input.account_id)?,
            limit: input.limit.unwrap_or(25).min(100),
            offset: input.offset.unwrap_or(0),
        })
        .await
    }

    #[tool(
        name = "mxr_search",
        description = "Search local mail and return result metadata/snippets without full message bodies."
    )]
    pub async fn search(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        self.daemon_json(Request::Search {
            query: input.query,
            limit: input.limit.unwrap_or(25).min(100),
            offset: input.offset.unwrap_or(0),
            account_id: parse_optional_id(input.account_id)?,
            mode: None,
            sort: None,
            explain: input.explain.unwrap_or(false),
        })
        .await
    }

    #[tool(
        name = "mxr_read_message",
        description = "Read one message envelope, and only include body content when include_body is true."
    )]
    pub async fn read_message(
        &self,
        Parameters(input): Parameters<ReadMessageInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        let message_id = parse_id::<MessageId>(&input.message_id)?;
        if input.include_body.unwrap_or(false) {
            self.daemon_json(Request::GetBody { message_id }).await
        } else {
            self.daemon_json(Request::GetEnvelope { message_id }).await
        }
    }

    #[tool(
        name = "mxr_read_thread",
        description = "Read a thread summary/envelopes. This does not return full bodies."
    )]
    pub async fn read_thread(
        &self,
        Parameters(input): Parameters<ReadThreadInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        self.daemon_json(Request::GetThread {
            thread_id: parse_id(&input.thread_id)?,
        })
        .await
    }

    #[tool(
        name = "mxr_draft_assist",
        description = "Generate a draft reply suggestion for a thread through the daemon LLM/draft-assist workflow. It is never sent automatically."
    )]
    pub async fn draft_assist(
        &self,
        Parameters(input): Parameters<DraftAssistInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        self.daemon_json(Request::DraftAssist {
            thread_id: parse_id(&input.thread_id)?,
            instruction: input.instruction,
        })
        .await
    }

    #[tool(
        name = "mxr_save_draft",
        description = "Persist a draft object through the daemon. The draft must match mxr's structured Draft JSON schema."
    )]
    pub async fn save_draft(
        &self,
        Parameters(input): Parameters<SaveDraftInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        let draft = serde_json::from_value(input.draft).map_err(|error| {
            ErrorData::invalid_params(format!("invalid draft JSON: {error}"), None)
        })?;
        self.daemon_json(Request::SaveDraft { draft }).await
    }

    #[tool(
        name = "mxr_mutation_preview",
        description = "Dry-run/preview a message mutation selection. This resolves the exact message IDs and envelope preview without mutating mail."
    )]
    pub async fn mutation_preview(
        &self,
        Parameters(input): Parameters<MutationPreviewInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        let ids = parse_message_ids(&input.message_ids)?;
        let preview = self
            .daemon_json(Request::ListEnvelopesByIds {
                message_ids: ids.clone(),
            })
            .await?;
        Ok(McpJson(json!({
            "dry_run": true,
            "action": input.action,
            "message_ids": ids.iter().map(MessageId::as_str).collect::<Vec<_>>(),
            "preview": preview.0
        })))
    }

    #[tool(
        name = "mxr_mutate",
        description = "Apply a previously previewed message mutation. Requires confirm=true; otherwise this returns a send-safe/destructive-safe block response without mutating."
    )]
    pub async fn mutate(
        &self,
        Parameters(input): Parameters<MutateInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        if !input.confirm.unwrap_or(false) {
            return Ok(McpJson(
                json!({"blocked": true, "reason": "confirm=true is required; call mxr_mutation_preview first"}),
            ));
        }
        let mutation = build_mutation(input.action, parse_message_ids(&input.message_ids)?)?;
        self.daemon_json(Request::Mutation {
            mutation,
            client_correlation_id: input.client_correlation_id,
        })
        .await
    }

    #[tool(
        name = "mxr_send_draft",
        description = "Send a stored draft only when confirm=true. Daemon MCP profile send gates and draft safety checks still apply."
    )]
    pub async fn send_draft(
        &self,
        Parameters(input): Parameters<SendDraftInput>,
    ) -> Result<McpJson<Value>, ErrorData> {
        if !input.confirm.unwrap_or(false) {
            return Ok(McpJson(
                json!({"blocked": true, "reason": "confirm=true is required before sending a draft"}),
            ));
        }
        self.daemon_json(Request::SendStoredDraft {
            draft_id: parse_id(&input.draft_id)?,
            override_safety_token: input.override_safety_token,
        })
        .await
    }
}

pub async fn serve_stdio() -> anyhow::Result<()> {
    let socket = default_socket_path()?;
    let server = MxrMcpServer::new(UnixDaemonRequester::new(socket));
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn default_socket_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(mxr_config::socket_path())
}

fn response_to_json(response: Response) -> Result<Value, ErrorData> {
    match response {
        Response::Ok { data } => serde_json::to_value(data).map_err(mcp_error),
        Response::Error { message, code, .. } => Err(ErrorData::internal_error(
            format!("daemon error {code}: {message}"),
            None,
        )),
    }
}

fn mcp_error(error: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

fn parse_id<T>(value: &str) -> Result<T, ErrorData>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| ErrorData::invalid_params(format!("invalid id `{value}`: {error}"), None))
}

fn parse_optional_id<T>(value: Option<String>) -> Result<Option<T>, ErrorData>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.as_deref().map(parse_id).transpose()
}

fn parse_message_ids(values: &[String]) -> Result<Vec<MessageId>, ErrorData> {
    if values.is_empty() {
        return Err(ErrorData::invalid_params(
            "message_ids must not be empty",
            None,
        ));
    }
    values.iter().map(|value| parse_id(value)).collect()
}

fn build_mutation(
    action: MutationAction,
    message_ids: Vec<MessageId>,
) -> Result<MutationCommand, ErrorData> {
    Ok(match action {
        MutationAction::Archive => MutationCommand::Archive { message_ids },
        MutationAction::ReadAndArchive => MutationCommand::ReadAndArchive { message_ids },
        MutationAction::Trash => MutationCommand::Trash { message_ids },
        MutationAction::Spam => MutationCommand::Spam { message_ids },
        MutationAction::MarkRead => MutationCommand::SetRead {
            message_ids,
            read: true,
        },
        MutationAction::MarkUnread => MutationCommand::SetRead {
            message_ids,
            read: false,
        },
        MutationAction::Star => MutationCommand::Star {
            message_ids,
            starred: true,
        },
        MutationAction::Unstar => MutationCommand::Star {
            message_ids,
            starred: false,
        },
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListMessagesInput {
    pub account_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchInput {
    pub query: String,
    pub account_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub explain: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadMessageInput {
    pub message_id: String,
    pub include_body: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadThreadInput {
    pub thread_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DraftAssistInput {
    pub thread_id: String,
    pub instruction: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveDraftInput {
    pub draft: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MutationPreviewInput {
    pub action: MutationAction,
    pub message_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MutateInput {
    pub action: MutationAction,
    pub message_ids: Vec<String>,
    pub confirm: Option<bool>,
    pub client_correlation_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendDraftInput {
    pub draft_id: String,
    pub confirm: Option<bool>,
    pub override_safety_token: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MutationAction {
    Archive,
    ReadAndArchive,
    Trash,
    Spam,
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::DraftId;
    use mxr_protocol::ResponseData;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FakeRequester {
        requests: Mutex<Vec<Request>>,
    }

    #[async_trait]
    impl DaemonRequester for FakeRequester {
        async fn request(&self, request: Request) -> anyhow::Result<Response> {
            self.requests.lock().expect("requests lock").push(request);
            Ok(Response::Ok {
                data: ResponseData::Pong,
            })
        }
    }

    #[tokio::test]
    async fn lists_stable_mxr_tools_over_mcp() {
        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let server = MxrMcpServer::new(FakeRequester::default());
        let server_task = tokio::spawn(async move {
            let service = server.serve(server_transport).await.expect("serve server");
            service.waiting().await.expect("server wait");
        });

        let client = ().serve(client_transport).await.expect("serve client");
        let tools = client.peer().list_tools(None).await.expect("list tools");
        let names = tools
            .tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();

        assert!(names.contains(&"mxr_status"));
        assert!(names.contains(&"mxr_read_message"));
        assert!(names.contains(&"mxr_mutation_preview"));
        assert!(names.contains(&"mxr_send_draft"));

        drop(client);
        server_task.abort();
    }

    #[tokio::test]
    async fn send_draft_blocks_without_confirmation() {
        let server = MxrMcpServer::new(FakeRequester::default());
        let result = server
            .send_draft(Parameters(SendDraftInput {
                draft_id: DraftId::new().as_str(),
                confirm: None,
                override_safety_token: None,
            }))
            .await
            .expect("tool result");
        assert_eq!(result.0["blocked"], true);
    }

    #[tokio::test]
    async fn status_uses_daemon_requester() {
        let server = MxrMcpServer::new(FakeRequester::default());
        let result = server.status().await.expect("tool result");
        assert_eq!(
            result.0,
            serde_json::to_value(ResponseData::Pong).expect("pong JSON")
        );
    }
}
