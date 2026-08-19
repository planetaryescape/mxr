//! Live progress rendering for long-running IPC calls.
//!
//! Long daemon operations (sync, rebuild-analytics, reindex-semantic) are
//! issued through [`crate::ipc_client::IpcClient::request_with_events`], which
//! waits without a deadline and forwards every `DaemonEvent` frame the daemon
//! emits while it works. `ProgressPrinter` turns those frames into something
//! a human can watch, and keeps a spinner running in between so the CLI never
//! looks hung.

use crate::ipc_client::IpcClient;
use mxr_protocol::{DaemonEvent, Request, Response};
use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex};
use tokio::time::{interval, Duration};

/// Issue a request that the daemon answers only after doing the work, on the
/// no-deadline IPC path, rendering its progress events while it runs.
///
/// Nothing in the protocol acks and continues in the background: a sync,
/// analytics rebuild, semantic reindex, or LLM cache rebuild all block
/// server-side until they finish. The 120s cap on [`IpcClient::request`] is
/// therefore a guaranteed failure on a large mailbox, and it kills the client
/// while the daemon keeps working — issue #179. Every long-running command
/// belongs on this path.
pub(crate) async fn request_with_progress(
    client: &mut IpcClient,
    json_mode: bool,
    label: &str,
    request: Request,
) -> anyhow::Result<Response> {
    let progress = ProgressPrinter::new(json_mode);
    progress.note(label);
    let response = client
        .request_with_events(request, progress.event_callback())
        .await;
    progress.finish();
    response
}

/// Prints a live spinner on stderr for long-running operations and
/// surfaces `OperationProgress` events as they arrive. Spinner runs
/// only on a TTY and only for human (non-JSON) output, so piped
/// output stays parseable. JSON callers get the events on stderr in
/// JSON-Lines so scripts can still see progress without polluting
/// stdout.
pub(crate) struct ProgressPrinter {
    json_mode: bool,
    tty: bool,
    label: Arc<Mutex<String>>,
    spinner_handle: tokio::task::JoinHandle<()>,
}

impl ProgressPrinter {
    pub(crate) fn new(json_mode: bool) -> Self {
        let tty = std::io::stderr().is_terminal();
        let label = Arc::new(Mutex::new("Working".to_string()));
        // The spinner only paints when we're on a TTY and not in
        // JSON mode. Otherwise the JoinHandle holds an idle task that
        // we abort on `finish` — cheap and avoids two code paths.
        let label_for_task = label.clone();
        let active = tty && !json_mode;
        let spinner_handle = tokio::spawn(async move {
            if !active {
                return;
            }
            let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut idx = 0usize;
            let mut tick = interval(Duration::from_millis(100));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let label = label_for_task.lock().map(|s| s.clone()).unwrap_or_default();
                let mut stderr = std::io::stderr();
                let _ = write!(stderr, "\r\x1b[K{} {}", frames[idx % frames.len()], label);
                let _ = stderr.flush();
                idx = idx.wrapping_add(1);
            }
        });
        Self {
            json_mode,
            tty,
            label,
            spinner_handle,
        }
    }

    pub(crate) fn event_callback(&self) -> impl FnMut(DaemonEvent) + 'static {
        let json_mode = self.json_mode;
        let tty = self.tty;
        let label = self.label.clone();
        move |event: DaemonEvent| {
            let mut stderr = std::io::stderr();
            // Clear the spinner line before printing so the event
            // doesn't end up appended to the spinner glyph.
            if tty && !json_mode {
                let _ = write!(stderr, "\r\x1b[K");
            }
            match &event {
                DaemonEvent::OperationStarted { message, .. } => {
                    if json_mode {
                        if let Ok(s) = serde_json::to_string(&event) {
                            let _ = writeln!(stderr, "{s}");
                        }
                    } else {
                        let _ = writeln!(stderr, "▶ {message}");
                    }
                    if let Ok(mut l) = label.lock() {
                        *l = message.clone();
                    }
                }
                DaemonEvent::OperationProgress {
                    current,
                    total,
                    message,
                    ..
                } => {
                    let total_str = total.map_or_else(|| "?".into(), |t| t.to_string());
                    if json_mode {
                        if let Ok(s) = serde_json::to_string(&event) {
                            let _ = writeln!(stderr, "{s}");
                        }
                    } else {
                        let _ = writeln!(stderr, "  [{current}/{total_str}] {message}");
                    }
                    if let Ok(mut l) = label.lock() {
                        *l = format!("[{current}/{total_str}] {message}");
                    }
                }
                DaemonEvent::OperationCompleted { message, .. } => {
                    if json_mode {
                        if let Ok(s) = serde_json::to_string(&event) {
                            let _ = writeln!(stderr, "{s}");
                        }
                    } else {
                        let _ = writeln!(stderr, "✓ {message}");
                    }
                }
                DaemonEvent::OperationFailed {
                    error, retryable, ..
                } => {
                    if json_mode {
                        if let Ok(s) = serde_json::to_string(&event) {
                            let _ = writeln!(stderr, "{s}");
                        }
                    } else {
                        let _ = writeln!(
                            stderr,
                            "✗ {error}{}",
                            if *retryable { " (retryable)" } else { "" }
                        );
                    }
                }
                _ => {}
            }
            let _ = stderr.flush();
        }
    }

    /// Report progress the client observed itself (a status poll), as opposed
    /// to a `DaemonEvent`. On a TTY it rewrites the spinner label in place so
    /// a poll loop doesn't scroll the terminal; elsewhere it prints one plain
    /// line so piped output still shows forward motion. Callers throttle.
    pub(crate) fn note(&self, message: &str) {
        if let Ok(mut label) = self.label.lock() {
            message.clone_into(&mut label);
        }
        if self.tty && !self.json_mode {
            return;
        }
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "  {message}");
        let _ = stderr.flush();
    }

    /// Stops the spinner and clears the spinner line so the final
    /// stdout output isn't preceded by a stale glyph. Idempotent.
    pub(crate) fn finish(&self) {
        self.spinner_handle.abort();
        if self.tty && !self.json_mode {
            let mut stderr = std::io::stderr();
            let _ = write!(stderr, "\r\x1b[K");
            let _ = stderr.flush();
        }
    }
}

impl Drop for ProgressPrinter {
    /// Also clears the spinner line, so a caller that bails mid-operation
    /// doesn't leave a half-painted glyph behind the error message.
    fn drop(&mut self) {
        self.finish();
    }
}

/// Render a count with thousands separators. Progress lines report totals in
/// the tens of thousands (`contacts_rows`, seeded demo messages, semantic
/// vectors), and `50,000` is far easier to read at a glance than `50000`.
pub(crate) fn format_thousands(n: impl std::fmt::Display) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_thousands;

    #[test]
    fn format_thousands_inserts_commas_every_three_digits() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(42), "42");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1_000), "1,000");
        assert_eq!(format_thousands(10_673), "10,673");
        assert_eq!(format_thousands(1_234_567), "1,234,567");
    }
}
