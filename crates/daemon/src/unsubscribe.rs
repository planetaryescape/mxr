#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use mxr_core::types::UnsubscribeMethod;
use reqwest::Client;

/// Result of an unsubscribe attempt.
#[derive(Debug)]
pub enum UnsubscribeResult {
    Success(String),
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
                Ok(resp) => {
                    UnsubscribeResult::Failed(format!("One-click POST returned {}", resp.status()))
                }
                Err(e) => UnsubscribeResult::Failed(format!("One-click unsubscribe failed: {e}")),
            }
        }

        UnsubscribeMethod::HttpLink { url } | UnsubscribeMethod::BodyLink { url } => {
            match open_in_browser(url) {
                Ok(()) => UnsubscribeResult::Success("Opened unsubscribe page in browser.".into()),
                Err(e) => UnsubscribeResult::Failed(format!("Failed to open browser: {e}")),
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
            other => panic!("Expected Success, got: {:?}", other),
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
