use super::*;
use tokio::net::TcpListener;

async fn gateway() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(
        std::path::PathBuf::from("/missing-browser-test.sock"),
        "https://mail.example.test",
    )
    .unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), task)
}

#[test]
fn origin_configuration_refuses_non_https_and_credential_urls() {
    for origin in [
        "http://mail.test",
        "https://user:pw@mail.test",
        "https://mail.test/path",
        "https://mail.test/?x=1",
        "https://mail.test/#token=secret",
    ] {
        assert!(router("missing.sock".into(), origin).is_err(), "{origin}");
    }
}

#[tokio::test]
async fn browser_has_no_loopback_bootstrap_or_cross_origin_mutation_path() {
    let (base, task) = gateway().await;
    let client = reqwest::Client::new();
    let root = client
        .get(format!("{base}/"))
        .header("host", "mail.example.test")
        .send()
        .await
        .unwrap();
    assert_eq!(root.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        root.headers().get("referrer-policy").unwrap(),
        "same-origin"
    );
    assert!(root.text().await.unwrap().contains("single-use code"));
    let launch = client
        .get(format!("{base}/api/v1/remote/browser-launch"))
        .header("host", "mail.example.test")
        .header(
            "cookie",
            format!(
                "{COOKIE}=browser:{}:{}",
                uuid::Uuid::now_v7(),
                "a".repeat(64)
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(launch.status(), reqwest::StatusCode::OK);
    assert!(launch.text().await.unwrap().contains("single-use code"));
    let bootstrap = client
        .get(format!("{base}/api/v1/auth/local-token"))
        .header("host", "mail.example.test")
        .send()
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), reqwest::StatusCode::UNAUTHORIZED);
    for origin in [None, Some("null"), Some("https://evil.test")] {
        let mut request = client
            .post(format!("{base}/api/v1/remote/browser-session"))
            .header("host", "mail.example.test")
            .form(&[("code", "a".repeat(64))]);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        assert_eq!(
            request.send().await.unwrap().status(),
            reqwest::StatusCode::FORBIDDEN
        );
    }
    let spoof = client
        .get(format!("{base}/"))
        .header("host", "evil.test")
        .header("x-forwarded-host", "mail.example.test")
        .send()
        .await
        .unwrap();
    assert_eq!(spoof.status(), reqwest::StatusCode::FORBIDDEN);
    task.abort();
}

#[test]
fn duplicate_or_malformed_cookies_cannot_select_another_principal() {
    let secret = "a".repeat(64);
    let id = uuid::Uuid::now_v7();
    let valid = format!("{COOKIE}=browser:{id}:{secret}");
    let request = Request::builder()
        .header(header::COOKIE, &valid)
        .body(axum::body::Body::empty())
        .unwrap();
    assert!(cookie(&request).is_some());
    let duplicate = Request::builder()
        .header(header::COOKIE, format!("{valid}; {valid}"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert!(cookie(&duplicate).is_none());
    let malformed = Request::builder()
        .header(header::COOKIE, format!("{COOKIE}=native:{id}:{secret}"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert!(cookie(&malformed).is_none());
}

#[test]
fn json_media_types_cannot_skip_compose_ownership_validation() {
    for content_type in [
        "application/json",
        "APPLICATION/JSON",
        "application/vnd.mxr+json; charset=utf-8",
    ] {
        assert!(is_json(content_type));
    }
}
