use serde::Serialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Instant};

/// Default timeout for shell hooks.
const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to keep collecting stderr after a failed shell has exited.
///
/// The shell's own output is already buffered in the pipe, so reading it takes
/// microseconds; 250 ms is only the ceiling for a descendant that keeps the pipe
/// open. It is long enough to survive a loaded scheduler and short enough that
/// a failing hook with a lingering background process is not noticeably slow.
const STDERR_GRACE_AFTER_EXIT: Duration = Duration::from_millis(250);

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
    let deadline = Instant::now() + timeout_dur;

    let json = serde_json::to_string(payload)
        .map_err(|e| ShellHookError::SerializationFailed(e.to_string()))?;
    if Instant::now() >= deadline {
        return Err(ShellHookError::Timeout {
            command: command.to_string(),
            timeout: timeout_dur,
        });
    }

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ShellHookError::SpawnFailed {
            command: command.to_string(),
            error: e.to_string(),
        })?;

    let stdin = child.stdin.take();
    let mut stderr = child.stderr.take();
    // Drain stderr while writing: a hook can fill its stderr pipe before it
    // reads stdin. Keep ownership of the child outside the timed future so a
    // timeout can kill and reap it, rather than merely dropping the wait.
    let execution = async {
        let write = async {
            if let Some(mut stdin) = stdin {
                match stdin.write_all(json.as_bytes()).await {
                    Ok(()) => None,
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => None,
                    Err(error) => Some(error.to_string()),
                }
            } else {
                None
            }
        };
        tokio::pin!(write);
        let mut write_error = None;
        let mut write_done = false;
        let mut stderr_bytes = Vec::new();
        let mut stderr_open = stderr.is_some();
        // Wait on the shell itself, not on pipe EOF: a backgrounded descendant
        // inherits stderr and keeps it open long after the shell exits.
        let status = loop {
            tokio::select! {
                status = child.wait() => {
                    break status.map_err(|e| ShellHookError::WaitFailed(e.to_string()))?;
                }
                result = &mut write, if !write_done => {
                    write_error = result;
                    write_done = true;
                }
                read = read_chunk(&mut stderr, &mut stderr_bytes), if stderr_open => {
                    match read {
                        Ok(0) => stderr_open = false,
                        Ok(_) => {}
                        Err(e) => return Err(ShellHookError::WaitFailed(e.to_string())),
                    }
                }
            }
        };
        // An unfinished stdin write is abandoned here (dropping `write` closes
        // our end): a descendant holding the read end open would otherwise block
        // a large payload forever, and the shell no longer wants it.
        //
        // Stderr is only reported for a failed hook, so a successful one returns
        // without draining. For a failure, the shell's own output is already in
        // the pipe buffer, so a short bounded read collects it; whatever a
        // descendant writes later is not part of the hook's result.
        if !status.success() && stderr_open {
            let grace_end = (Instant::now() + STDERR_GRACE_AFTER_EXIT).min(deadline);
            let _ = tokio::time::timeout_at(grace_end, async {
                // A read error here only ends collection; the exit status is the result.
                while let Ok(n) = read_chunk(&mut stderr, &mut stderr_bytes).await {
                    if n == 0 {
                        break;
                    }
                }
            })
            .await;
        }
        Ok::<_, ShellHookError>((write_error, stderr_bytes, status))
    };
    let (stdin_write_error, stderr, status) =
        match tokio::time::timeout_at(deadline, execution).await {
            Ok(result) => result?,
            Err(_) => {
                let _ = child.start_kill();
                // Reaping is bounded independently; kill_on_drop is the fallback
                // for cancellation or a platform wait that does not finish.
                let _ = timeout(Duration::from_secs(1), child.wait()).await;
                return Err(ShellHookError::Timeout {
                    command: command.to_string(),
                    timeout: timeout_dur,
                });
            }
        };

    if let Some(error) = stdin_write_error {
        return Err(ShellHookError::StdinWriteFailed(error));
    }

    if status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&stderr).to_string();
        Err(ShellHookError::NonZeroExit {
            command: command.to_string(),
            code: status.code(),
            stderr,
        })
    }
}

/// Cancel-safe single read into `bytes`; `Ok(0)` is EOF.
async fn read_chunk(
    stderr: &mut Option<tokio::process::ChildStderr>,
    bytes: &mut Vec<u8>,
) -> std::io::Result<usize> {
    use tokio::io::AsyncReadExt;
    match stderr {
        Some(pipe) => pipe.read_buf(bytes).await,
        None => Ok(0),
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
    #![expect(
        clippy::panic,
        reason = "tests panic with diagnostic context for direct failures"
    )]

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
    async fn hook_timeout_includes_stalled_large_stdin_and_reaps_owned_child() {
        let mut payload = sample_payload();
        payload.body_text = Some("x".repeat(2 * 1024 * 1024));
        let pid_file = std::env::temp_dir().join(format!("mxr-hook-{}.pid", uuid::Uuid::now_v7()));
        // The sleep outlives the deadline, so only the timeout can end the child,
        // and still bounds the red test when stdin has no timeout.
        let command = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());
        // The deadline must outlast spawning `sh` and running `echo` on a loaded
        // machine, or the child dies before recording its pid. 200 ms lost that
        // race under load.
        let deadline = Duration::from_secs(2);
        let started = std::time::Instant::now();
        let result = execute_shell_hook(&command, &payload, Some(deadline)).await;
        let elapsed = started.elapsed();
        let pid = std::fs::read_to_string(&pid_file).unwrap_or_else(|error| {
            panic!("hook did not record its pid before the {deadline:?} deadline: {error}")
        });
        std::fs::remove_file(pid_file).expect("remove owned fixture pid file");
        let alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .expect("probe owned pid")
            .success();
        assert!(!alive, "owned hook child {pid} survived timeout");
        assert!(
            matches!(result, Err(ShellHookError::Timeout { .. })),
            "{result:?}"
        );
        // Deadline plus the hook's 1 s reap bound, with slack. A stdin write the
        // deadline missed would block until the 30 s sleep ends.
        assert!(
            elapsed < deadline + Duration::from_secs(3),
            "stdin exceeded the hook deadline: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn cancelling_hook_kills_owned_child() {
        let pid_file = std::env::temp_dir().join(format!("mxr-hook-{}.pid", uuid::Uuid::now_v7()));
        // The sleep outlives the waits below, so only the abort can end the child.
        let command = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());
        let task =
            tokio::spawn(
                async move { execute_shell_hook(&command, &sample_payload(), None).await },
            );
        // Generous waits: `sh` can take over a second to start on a loaded
        // machine, and a passing run returns as soon as the poll succeeds.
        let pid = timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(pid) = std::fs::read_to_string(&pid_file) {
                    if !pid.trim().is_empty() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("hook started");
        task.abort();
        let _ = task.await;
        let gone = timeout(Duration::from_secs(10), async {
            loop {
                let alive = std::process::Command::new("kill")
                    .args(["-0", pid.trim()])
                    .stderr(Stdio::null())
                    .status()
                    .expect("probe owned pid")
                    .success();
                if !alive {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        std::fs::remove_file(pid_file).expect("remove owned fixture pid file");
        assert!(gone.is_ok(), "cancelled child {pid} survived");
    }

    #[tokio::test]
    async fn hook_success_exit_zero() {
        execute_shell_hook("cat > /dev/null", &sample_payload(), None)
            .await
            .expect("exit 0 hook should succeed");
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
            other => panic!("Expected NonZeroExit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn hook_timeout() {
        let result = execute_shell_hook(
            "exec sleep 2",
            &sample_payload(),
            Some(Duration::from_millis(100)),
        )
        .await;
        assert!(matches!(result, Err(ShellHookError::Timeout { .. })));
    }

    #[tokio::test]
    async fn hook_receives_valid_json_on_stdin() {
        // Use python to validate JSON on stdin
        execute_shell_hook(
            "python3 -c 'import sys, json; d = json.load(sys.stdin); assert d[\"id\"] == \"msg_123\"'",
            &sample_payload(),
            None,
        )
        .await
        .expect("hook should receive valid JSON");
    }

    #[tokio::test]
    async fn hook_payload_contains_all_fields() {
        // Extract and verify specific fields from the JSON
        execute_shell_hook(
            "python3 -c 'import sys, json; d = json.load(sys.stdin); assert d[\"from\"][\"email\"] == \"alice@example.com\"; assert d[\"subject\"] == \"Invoice #2847\"; assert len(d[\"attachments\"]) == 1'",
            &sample_payload(),
            None,
        )
        .await
        .expect("payload field check should pass");
    }

    #[tokio::test]
    async fn hook_with_pipe_command() {
        // Test that shell pipes work
        execute_shell_hook("cat | head -c 1 > /dev/null", &sample_payload(), None)
            .await
            .expect("piped shell hook should succeed");
    }

    fn kill_pid_file(pid_file: &std::path::Path) {
        if let Ok(pid) = std::fs::read_to_string(pid_file) {
            let _ = std::process::Command::new("kill")
                .arg(pid.trim())
                .stderr(Stdio::null())
                .status();
        }
        let _ = std::fs::remove_file(pid_file);
    }

    fn pid_file() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mxr-hook-{}.pid", uuid::Uuid::now_v7()))
    }

    // The 20 s sleeps outlive every deadline here, so only the fix can make
    // these return before the hook timeout; the bounds are loose on purpose.
    #[tokio::test]
    async fn backgrounded_child_holding_stderr_is_not_a_timeout() {
        let pid_file = pid_file();
        let command = format!("sleep 20 & echo $! > '{}'; exit 0", pid_file.display());
        let result =
            execute_shell_hook(&command, &sample_payload(), Some(Duration::from_secs(10))).await;
        kill_pid_file(&pid_file);
        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn backgrounded_child_holding_stdin_does_not_block_large_payload() {
        let mut payload = sample_payload();
        payload.body_text = Some("x".repeat(2 * 1024 * 1024));
        let pid_file = pid_file();
        // `<&0` keeps the stdin read end open without ever reading it.
        let command = format!("sleep 20 <&0 & echo $! > '{}'; exit 0", pid_file.display());
        let result = execute_shell_hook(&command, &payload, Some(Duration::from_secs(10))).await;
        kill_pid_file(&pid_file);
        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn failed_hook_keeps_stderr_written_before_exit_while_descendant_holds_pipe() {
        let pid_file = pid_file();
        let command = format!(
            "echo partial >&2; sleep 20 & echo $! > '{}'; exit 3",
            pid_file.display()
        );
        let result =
            execute_shell_hook(&command, &sample_payload(), Some(Duration::from_secs(10))).await;
        kill_pid_file(&pid_file);
        match result {
            Err(ShellHookError::NonZeroExit { code, stderr, .. }) => {
                assert_eq!(code, Some(3));
                assert!(stderr.contains("partial"), "{stderr:?}");
            }
            other => panic!("Expected NonZeroExit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn large_stderr_is_fully_read_when_shell_exits() {
        // Well over a pipe buffer, so it only completes if stderr is drained
        // while the shell runs and the remainder is read after it exits.
        let command = "yes x | head -c 300000 >&2; exit 2";
        let result = execute_shell_hook(command, &sample_payload(), None).await;
        match result {
            Err(ShellHookError::NonZeroExit { stderr, .. }) => {
                assert_eq!(stderr.len(), 300_000);
            }
            other => panic!("Expected NonZeroExit, got {other:?}"),
        }
    }
}
