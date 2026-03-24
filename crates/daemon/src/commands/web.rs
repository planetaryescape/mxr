use crate::mxr_web::WebServerConfig;
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;
use uuid::Uuid;

pub async fn run(host: String, port: u16, print_url: bool) -> anyhow::Result<()> {
    let host: IpAddr = host
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid host `{host}`: {error}"))?;
    let listener = TcpListener::bind(SocketAddr::new(host, port)).await?;
    let addr = listener.local_addr()?;
    let auth_token =
        std::env::var("MXR_WEB_BRIDGE_TOKEN").unwrap_or_else(|_| Uuid::new_v4().to_string());
    let config = WebServerConfig::new(crate::mxr_config::socket_path(), auth_token.clone());
    let bridge_url = format!("http://{addr}?token={auth_token}");

    if print_url {
        println!("{bridge_url}");
    } else {
        eprintln!("mxr web bridge listening on {bridge_url}");
    }

    crate::mxr_web::serve(listener, config)
        .await
        .map_err(anyhow::Error::from)
}
