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
#[derive(Debug)]
pub enum UnsubscribeResult {
    Success(String),
    Failed {
        message: String,
        outcome: FailureOutcome,
    },
    NoMethod,
}

/// Whether a failed attempt could have had an effect at the sender.
///
/// The caller keeps the send claim for `Unknown` so the unsubscribe is never
/// re-sent, and releases it for `NotSent` so a retry can go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureOutcome {
    /// The request never left mxr, or the server answered with an error status
    /// and so did not apply the unsubscribe.
    NotSent,
    /// The request may have been received: a timeout or reset after it was
    /// written, or a response that could not be read.
    Unknown,
}

fn failed(message: String, outcome: FailureOutcome) -> UnsubscribeResult {
    UnsubscribeResult::Failed { message, outcome }
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
                Ok(resp) => failed(
                    format!("One-click POST returned {}", resp.status()),
                    FailureOutcome::NotSent,
                ),
                // Connect-phase errors (refused, DNS, bad URL) are raised before
                // the request body is written, so the server cannot have acted.
                Err(e) if e.is_connect() || e.is_builder() => failed(
                    format!("One-click unsubscribe failed: {e}"),
                    FailureOutcome::NotSent,
                ),
                Err(e) => failed(
                    format!("One-click unsubscribe failed: {e}"),
                    FailureOutcome::Unknown,
                ),
            }
        }

        UnsubscribeMethod::HttpLink { url } | UnsubscribeMethod::BodyLink { url } => {
            match open_in_browser(url) {
                Ok(()) => UnsubscribeResult::Success("Opened unsubscribe page in browser.".into()),
                Err(e) => failed(
                    format!("Failed to open browser: {e}"),
                    FailureOutcome::NotSent,
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
    async fn refused_connection_is_not_sent() {
        let result = execute_unsubscribe(
            &one_click("http://127.0.0.1:1/unsubscribe".into()),
            &Client::new(),
        )
        .await;
        assert!(
            matches!(
                result,
                UnsubscribeResult::Failed {
                    outcome: FailureOutcome::NotSent,
                    ..
                }
            ),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn server_error_status_is_not_sent() {
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
        assert!(
            matches!(
                result,
                UnsubscribeResult::Failed {
                    outcome: FailureOutcome::NotSent,
                    ..
                }
            ),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn timeout_after_the_request_is_unknown() {
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
        assert!(
            matches!(
                result,
                UnsubscribeResult::Failed {
                    outcome: FailureOutcome::Unknown,
                    ..
                }
            ),
            "got {result:?}"
        );
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
