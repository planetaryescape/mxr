use serde::Serialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Default timeout for shell hooks.
const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// JSON payload piped to shell hook stdin.
#[derive(Debug, Serialize)]
pub struct ShellHookPayload {
    pub id: String,
    pub from: ShellHookAddress,
    pub subject: String,
    pub date: String,
    pub body_text: Option<String>,
    pub attachments: Vec<ShellHookAttachment>,
}

#[derive(Debug, Serialize)]
pub struct ShellHookAddress {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct ShellHookAttachment {
    pub filename: String,
    pub size_bytes: u64,
    pub local_path: Option<String>,
}

/// Execute a shell hook command with message data on stdin.
///
/// Returns Ok(()) on exit code 0, Err on non-zero or timeout.
pub async fn execute_shell_hook(
    command: &str,
    payload: &ShellHookPayload,
    hook_timeout: Option<Duration>,
) -> Result<(), ShellHookError> {
    let timeout_dur = hook_timeout.unwrap_or(DEFAULT_HOOK_TIMEOUT);

    let json = serde_json::to_string(payload)
        .map_err(|e| ShellHookError::SerializationFailed(e.to_string()))?;

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ShellHookError::SpawnFailed {
            command: command.to_string(),
            error: e.to_string(),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| ShellHookError::StdinWriteFailed(e.to_string()))?;
    }

    let result = timeout(timeout_dur, child.wait_with_output())
        .await
        .map_err(|_| ShellHookError::Timeout {
            command: command.to_string(),
            timeout: timeout_dur,
        })?
        .map_err(|e| ShellHookError::WaitFailed(e.to_string()))?;

    if result.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        Err(ShellHookError::NonZeroExit {
            command: command.to_string(),
            code: result.status.code(),
            stderr,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellHookError {
    #[error("Failed to serialize message to JSON: {0}")]
    SerializationFailed(String),
    #[error("Failed to spawn command '{command}': {error}")]
    SpawnFailed { command: String, error: String },
    #[error("Failed to write to command stdin: {0}")]
    StdinWriteFailed(String),
    #[error("Command '{command}' timed out after {timeout:?}")]
    Timeout { command: String, timeout: Duration },
    #[error("Failed to wait for command: {0}")]
    WaitFailed(String),
    #[error("Command '{command}' exited with code {code:?}: {stderr}")]
    NonZeroExit {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> ShellHookPayload {
        ShellHookPayload {
            id: "msg_123".into(),
            from: ShellHookAddress {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            },
            subject: "Invoice #2847".into(),
            date: "2026-03-17T10:30:00Z".into(),
            body_text: Some("Please find attached the invoice.".into()),
            attachments: vec![ShellHookAttachment {
                filename: "invoice.pdf".into(),
                size_bytes: 234_567,
                local_path: Some("/tmp/mxr/invoice.pdf".into()),
            }],
        }
    }

    #[tokio::test]
    async fn hook_success_exit_zero() {
        let result = execute_shell_hook("cat > /dev/null", &sample_payload(), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn hook_failure_exit_nonzero() {
        let result = execute_shell_hook("exit 1", &sample_payload(), None).await;
        assert!(matches!(result, Err(ShellHookError::NonZeroExit { .. })));
    }

    #[tokio::test]
    async fn hook_captures_stderr_on_failure() {
        let result = execute_shell_hook("echo 'oops' >&2; exit 1", &sample_payload(), None).await;
        match result {
            Err(ShellHookError::NonZeroExit { stderr, .. }) => {
                assert!(stderr.contains("oops"));
            }
            other => panic!("Expected NonZeroExit, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn hook_timeout() {
        let result = execute_shell_hook(
            "sleep 60",
            &sample_payload(),
            Some(Duration::from_millis(100)),
        )
        .await;
        assert!(matches!(result, Err(ShellHookError::Timeout { .. })));
    }

    #[tokio::test]
    async fn hook_receives_valid_json_on_stdin() {
        // Use python to validate JSON on stdin
        let result = execute_shell_hook(
            "python3 -c 'import sys, json; d = json.load(sys.stdin); assert d[\"id\"] == \"msg_123\"'",
            &sample_payload(),
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "Hook should receive valid JSON: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn hook_payload_contains_all_fields() {
        // Extract and verify specific fields from the JSON
        let result = execute_shell_hook(
            "python3 -c 'import sys, json; d = json.load(sys.stdin); assert d[\"from\"][\"email\"] == \"alice@example.com\"; assert d[\"subject\"] == \"Invoice #2847\"; assert len(d[\"attachments\"]) == 1'",
            &sample_payload(),
            None,
        )
        .await;
        assert!(result.is_ok(), "Payload field check failed: {:?}", result);
    }

    #[tokio::test]
    async fn hook_with_pipe_command() {
        // Test that shell pipes work
        let result =
            execute_shell_hook("cat | head -c 1 > /dev/null", &sample_payload(), None).await;
        assert!(result.is_ok());
    }
}
