//! The MCP tools for a caller inside the daemon, such as the in-app
//! assistant. The server runs over an in-memory pipe with the caller's own
//! requester, so the tools, their schemas and their preview-then-confirm
//! flows are exactly what an external MCP client gets.

use crate::{DaemonRequester, MxrMcpServer};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;
use std::sync::Arc;

/// Bytes buffered each way on the in-memory pipe.
const PIPE_CAPACITY: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallOutcome {
    /// The structured result as JSON text, or the error as mxr reported it.
    pub content: String,
    pub is_error: bool,
}

pub struct InProcessTools {
    client: RunningService<RoleClient, ()>,
    tools: Vec<ToolDescriptor>,
}

impl InProcessTools {
    /// Start the server and list its tools, sorted by name so the list is
    /// the same bytes every time.
    pub async fn start(requester: Arc<dyn DaemonRequester>) -> anyhow::Result<Self> {
        let (client_io, server_io) = tokio::io::duplex(PIPE_CAPACITY);
        let server = MxrMcpServer::from_requester(requester);
        tokio::spawn(async move {
            match server.serve(server_io).await {
                Ok(running) => {
                    if let Err(error) = running.waiting().await {
                        tracing::warn!(%error, "in-process MCP server stopped");
                    }
                }
                Err(error) => tracing::warn!(%error, "in-process MCP server failed to start"),
            }
        });
        let client = ().serve(client_io).await?;
        let mut tools: Vec<ToolDescriptor> = client
            .list_all_tools()
            .await?
            .into_iter()
            .map(|tool| ToolDescriptor {
                name: tool.name.to_string(),
                description: tool.description.map(|d| d.to_string()).unwrap_or_default(),
                input_schema: Value::Object((*tool.input_schema).clone()),
            })
            .collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { client, tools })
    }

    pub fn tools(&self) -> &[ToolDescriptor] {
        &self.tools
    }

    pub async fn call(&self, name: &str, arguments: Value) -> ToolCallOutcome {
        let arguments = match arguments {
            Value::Object(object) => object,
            Value::Null => serde_json::Map::new(),
            other => {
                return ToolCallOutcome {
                    content: format!("arguments must be a JSON object, got {other}"),
                    is_error: true,
                }
            }
        };
        let mut params = CallToolRequestParams::new(name.to_string());
        params.arguments = Some(arguments);
        match self.client.call_tool(params).await {
            Ok(result) => {
                let is_error = result.is_error.unwrap_or(false);
                let content = match result.structured_content {
                    Some(value) => value.to_string(),
                    None => result
                        .content
                        .iter()
                        .filter_map(|block| block.as_text().map(|text| text.text.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                ToolCallOutcome { content, is_error }
            }
            Err(error) => ToolCallOutcome {
                content: error.to_string(),
                is_error: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mxr_protocol::{Request, Response, ResponseData};

    #[derive(Debug)]
    struct PongRequester;

    #[async_trait]
    impl DaemonRequester for PongRequester {
        async fn request(&self, request: Request) -> anyhow::Result<Response> {
            Ok(match request {
                Request::ListRules => Response::error("rules are unavailable"),
                _ => Response::Ok {
                    data: ResponseData::Pong,
                },
            })
        }
    }

    #[tokio::test]
    async fn tools_list_sorted_and_calls_return_mxr_results_and_errors() {
        let tools = InProcessTools::start(Arc::new(PongRequester))
            .await
            .expect("tools start");
        let names: Vec<&str> = tools.tools().iter().map(|t| t.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert!(names.contains(&"mxr_search"));
        assert!(tools.tools().iter().all(|t| t.input_schema.is_object()));

        let ok = tools.call("mxr_status", Value::Null).await;
        assert!(!ok.is_error, "{}", ok.content);
        assert!(ok.content.contains("Pong"), "{}", ok.content);

        let failed = tools.call("mxr_rules", serde_json::json!({})).await;
        assert!(failed.is_error);
        assert!(
            failed.content.contains("rules are unavailable"),
            "{}",
            failed.content
        );

        assert!(
            tools
                .call("mxr_status", serde_json::json!([1]))
                .await
                .is_error
        );
    }
}
