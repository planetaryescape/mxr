use super::remote_gateway;
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};

async fn gateway() -> (TempDir, String, JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("missing.sock");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, remote_gateway::router(socket))
            .await
            .unwrap();
    });
    (dir, format!("http://{address}"), task)
}

fn websocket_request(client: &reqwest::Client, url: String) -> reqwest::RequestBuilder {
    client
        .get(url)
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
}

#[tokio::test]
async fn gateway_exposes_only_pairing_cleanup_and_authenticated_mail_routes() {
    let (_dir, base, server) = gateway().await;
    let client = reqwest::Client::new();

    let missing_proof = websocket_request(&client, format!("{base}/api/v1/remote/ipc"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_proof.status(), reqwest::StatusCode::UNAUTHORIZED);

    let browser_origin = websocket_request(&client, format!("{base}/api/v1/remote/ipc"))
        .header("origin", "https://example.test")
        .header("authorization", format!("Bearer client:{}", "a".repeat(64)))
        .send()
        .await
        .unwrap();
    assert_eq!(browser_origin.status(), reqwest::StatusCode::FORBIDDEN);

    let admin_pairing_request = client
        .post(format!("{base}/api/v1/remote/pairings"))
        .json(&serde_json::json!({"action":"Clients"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        admin_pairing_request.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    let admin_cleanup_request = client
        .post(format!("{base}/api/v1/remote/self/deregister"))
        .json(&serde_json::json!({"action":"Status"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        admin_cleanup_request.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    for path in [
        "/api/v1/auth/local-token",
        "/api/v1/files/config",
        "/api/v1/admin/remote",
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND, "{path}");
    }

    server.abort();
}
