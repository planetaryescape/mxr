#![cfg_attr(
    test,
    expect(
        clippy::panic,
        reason = "tests panic with diagnostic context for direct failures"
    )
)]

use mxr_core::types::UnsubscribeMethod;
use reqwest::Client;

/// Result of an unsubscribe attempt.
///
/// Failure text is written for the user and the logs. It never contains the
/// unsubscribe URL or an address: those carry recipient ids and tokens.
#[derive(Debug)]
pub enum UnsubscribeResult {
    Success(String),
    /// The sender's unsubscribe page was handed to the browser. The page may
    /// never load, and the sender may never act on it, so this is not a
    /// confirmed send: the caller releases its claim and reports the handoff.
    BrowserOpened,
    Failed(String),
    NoMethod,
}

/// Execute an unsubscribe action.
pub async fn execute_unsubscribe(method: &UnsubscribeMethod, client: &Client) -> UnsubscribeResult {
    match method {
        UnsubscribeMethod::OneClick { url } => {
            match client
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    UnsubscribeResult::Success("Unsubscribed via one-click.".into())
                }
                Ok(resp) => UnsubscribeResult::Failed(format!(
                    "the sender's server answered {}",
                    resp.status()
                )),
                Err(error) => UnsubscribeResult::Failed(format!(
                    "could not complete the request to the sender's server ({})",
                    request_failure_kind(&error)
                )),
            }
        }

        UnsubscribeMethod::HttpLink { url } | UnsubscribeMethod::BodyLink { url } => {
            match open_in_browser(url) {
                Ok(()) => UnsubscribeResult::BrowserOpened,
                Err(_) => UnsubscribeResult::Failed(
                    "could not open the unsubscribe page in a browser".into(),
                ),
            }
        }

        UnsubscribeMethod::Mailto { address, .. } => {
            // In full implementation, auto-send unsubscribe email.
            // For now, inform user.
            UnsubscribeResult::Success(format!("Send an email to {address} to unsubscribe."))
        }

        UnsubscribeMethod::None => UnsubscribeResult::NoMethod,
    }
}

/// The kind of a failed request, without the error's text. reqwest's Display
/// includes the full URL.
fn request_failure_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timed out"
    } else if error.is_connect() {
        "could not connect"
    } else if error.is_builder() {
        "invalid request"
    } else {
        "request failed"
    }
}

fn open_in_browser(url: &str) -> Result<(), std::io::Error> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let cmd = "open";

    std::process::Command::new(cmd).arg(url).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_method_returns_no_method() {
        let client = Client::new();
        let result = execute_unsubscribe(&UnsubscribeMethod::None, &client).await;
        assert!(matches!(result, UnsubscribeResult::NoMethod));
    }

    fn one_click(url: String) -> UnsubscribeMethod {
        UnsubscribeMethod::OneClick { url }
    }

    #[tokio::test]
    async fn failures_never_name_the_url() {
        let url = "http://127.0.0.1:1/unsubscribe?recipient=secret-token-123";
        let result = execute_unsubscribe(&one_click(url.into()), &Client::new()).await;
        let UnsubscribeResult::Failed(message) = result else {
            panic!("expected a failure, got {result:?}");
        };
        assert!(!message.contains("secret-token-123"), "{message}");
        assert!(!message.contains("127.0.0.1"), "{message}");
        assert!(message.contains("could not connect"), "{message}");
    }

    #[tokio::test]
    async fn server_error_status_is_reported_without_the_url() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let result = execute_unsubscribe(
            &one_click(format!("{}/unsubscribe", server.uri())),
            &Client::new(),
        )
        .await;
        let UnsubscribeResult::Failed(message) = result else {
            panic!("expected a failure, got {result:?}");
        };
        assert!(message.contains("500"), "{message}");
        assert!(!message.contains(&server.uri()), "{message}");
    }

    #[tokio::test]
    async fn timeout_is_a_failure() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(204).set_delay(std::time::Duration::from_secs(3)),
            )
            .mount(&server)
            .await;
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(200))
            .build()
            .unwrap();
        let result =
            execute_unsubscribe(&one_click(format!("{}/unsubscribe", server.uri())), &client).await;
        let UnsubscribeResult::Failed(message) = result else {
            panic!("expected a failure, got {result:?}");
        };
        assert!(message.contains("timed out"), "{message}");
    }

    #[tokio::test]
    async fn mailto_returns_success() {
        let client = Client::new();
        let result = execute_unsubscribe(
            &UnsubscribeMethod::Mailto {
                address: "unsub@example.com".into(),
                subject: Some("unsubscribe".into()),
            },
            &client,
        )
        .await;
        match result {
            UnsubscribeResult::Success(msg) => {
                assert!(
                    msg.contains("unsub@example.com"),
                    "Success message should contain the address, got: {msg}"
                );
            }
            other => panic!("Expected Success, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn one_click_posts_rfc_8058_form_body() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::header(
                "content-type",
                "application/x-www-form-urlencoded",
            ))
            .and(wiremock::matchers::body_string(
                "List-Unsubscribe=One-Click",
            ))
            .respond_with(wiremock::ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let client = Client::new();
        let result = execute_unsubscribe(
            &UnsubscribeMethod::OneClick {
                url: format!("{}/unsubscribe", server.uri()),
            },
            &client,
        )
        .await;

        assert!(
            matches!(result, UnsubscribeResult::Success(_)),
            "Expected successful one-click unsubscribe, got: {result:?}"
        );
        server.verify().await;
    }
}
