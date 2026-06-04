#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mxr_mcp::serve_stdio().await
}
