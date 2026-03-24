#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mxr::run_cli(std::env::args().collect()).await
}
