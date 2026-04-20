use crate::app::App;
use crate::async_result::AsyncResult;
use crate::local_state;
use crate::runtime::{submit_task, AsyncResultTask};
use tokio::sync::mpsc;

async fn open_browser_file(
    message_id: mxr_core::MessageId,
    html: String,
) -> Result<std::path::PathBuf, String> {
    let dir = mxr_config::data_dir().join("browser");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| format!("failed to prepare browser dir: {error}"))?;

    let path = dir.join(format!("{}.html", message_id.as_str()));
    tokio::fs::write(&path, html)
        .await
        .map_err(|error| format!("failed to write browser file: {error}"))?;

    let browser_path = path.clone();
    tokio::task::spawn_blocking(move || open_path_in_browser(&browser_path))
        .await
        .map_err(|error| format!("browser opener task failed: {error}"))?
        .map_err(|error| format!("failed to open browser: {error}"))?;

    Ok(path)
}

fn open_path_in_browser(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let programs = ["open"];
    #[cfg(target_os = "linux")]
    let programs = ["xdg-open"];
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let programs = ["open"];

    for program in programs {
        let status = std::process::Command::new(program).arg(path).status();
        match status {
            Ok(status) if status.success() => return Ok(()),
            Ok(_) => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::other("no supported browser opener found"))
}

pub(crate) fn submit_pending_work(
    app: &mut App,
    local_io: &mpsc::UnboundedSender<AsyncResultTask>,
) {
    if let Some(pending) = app.pending_browser_open.take() {
        let _ = submit_task(local_io, async move {
            AsyncResult::BrowserOpened(open_browser_file(pending.message_id, pending.html).await)
        });
    }

    if app.pending_local_state_save {
        app.pending_local_state_save = false;
        let state = local_state::TuiLocalState {
            onboarding_seen: app.onboarding.seen,
        };
        let _ = submit_task(local_io, async move {
            AsyncResult::LocalStateSaved(
                local_state::save_async(state)
                    .await
                    .map_err(|error| error.to_string()),
            )
        });
    }

    for path in app.take_pending_draft_cleanup() {
        let _ = submit_task(local_io, async move {
            let result = mxr_compose::delete_draft_file_async(&path)
                .await
                .map_err(|error| error.to_string());
            AsyncResult::DraftCleanup { path, result }
        });
    }
}

pub(crate) fn submit_bug_report_write(
    local_io: &mpsc::UnboundedSender<AsyncResultTask>,
    content: String,
) {
    let _ = submit_task(local_io, async move {
        let filename = format!(
            "mxr-bug-report-{}.md",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );
        let path = std::env::temp_dir().join(filename);
        let result = tokio::fs::write(&path, &content)
            .await
            .map(|_| path)
            .map_err(|error| error.to_string());
        AsyncResult::BugReportSaved(result)
    });
}

pub(crate) fn handle_result(app: &mut App, result: AsyncResult) -> Option<AsyncResult> {
    match result {
        AsyncResult::LocalStateSaved(Ok(())) => None,
        AsyncResult::LocalStateSaved(Err(error)) => {
            app.status_message = Some(format!("Local state save failed: {error}"));
            None
        }
        AsyncResult::DraftCleanup { result: Ok(()), .. } => None,
        AsyncResult::DraftCleanup {
            path,
            result: Err(error),
        } => {
            app.status_message = Some(format!(
                "Draft cleanup failed for {}: {error}",
                path.display()
            ));
            None
        }
        AsyncResult::BugReportSaved(Ok(path)) => {
            app.diagnostics_page.status = Some(format!("Bug report saved to {}", path.display()));
            None
        }
        AsyncResult::BugReportSaved(Err(error)) => {
            app.diagnostics_page.status = Some(format!("Bug report write failed: {error}"));
            None
        }
        AsyncResult::BrowserOpened(Ok(path)) => {
            app.status_message = Some(format!("Opened in browser: {}", path.display()));
            None
        }
        AsyncResult::BrowserOpened(Err(error)) => {
            app.status_message = Some(format!("Open in browser failed: {error}"));
            None
        }
        other => Some(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, PendingSend, PendingSendMode};
    use crate::runtime::spawn_task_worker;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tokio::time::{timeout, Duration};

    fn test_pending_send(draft_path: std::path::PathBuf) -> PendingSend {
        PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Hello".into(),
                from: "me@example.com".into(),
                in_reply_to: None,
                references: vec![],
                attach: vec![],
            },
            body: "Body".into(),
            draft_path,
            mode: PendingSendMode::DraftOnlyNoRecipients,
        }
    }

    #[tokio::test]
    async fn discarded_draft_is_removed_via_local_io_worker() {
        let temp = std::env::temp_dir().join(format!(
            "mxr-local-io-discard-{}-{}.md",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&temp, "draft").expect("write temp draft");

        let mut app = App::new();
        app.pending_send_confirm = Some(test_pending_send(temp.clone()));

        let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(app.pending_send_confirm.is_none());
        assert!(
            temp.exists(),
            "draft stays on disk until local I/O worker runs"
        );
        assert_eq!(app.status_message.as_deref(), Some("Discarded"));

        let (result_tx, mut result_rx) = mpsc::unbounded_channel();
        let worker = spawn_task_worker(result_tx);
        submit_pending_work(&mut app, &worker);

        let result = timeout(Duration::from_secs(1), result_rx.recv())
            .await
            .expect("local I/O task should complete")
            .expect("local I/O task result");
        assert!(handle_result(&mut app, result).is_none());
        assert!(!temp.exists(), "worker removes the discarded draft");
    }

    #[tokio::test]
    async fn bug_report_write_runs_on_local_io_worker() {
        let (result_tx, mut result_rx) = mpsc::unbounded_channel();
        let worker = spawn_task_worker(result_tx);

        submit_bug_report_write(&worker, "# mxr bug report\n".into());

        let result = timeout(Duration::from_secs(1), result_rx.recv())
            .await
            .expect("bug report write should complete")
            .expect("bug report result");

        let saved_path = match &result {
            AsyncResult::BugReportSaved(Ok(path)) => path.clone(),
            _ => panic!("expected bug report save result"),
        };

        let mut app = App::new();
        let expected_status = format!("Bug report saved to {}", saved_path.display());
        assert!(handle_result(&mut app, result).is_none());
        assert!(saved_path.exists(), "bug report should be written to disk");
        assert_eq!(
            app.diagnostics_page.status.as_deref(),
            Some(expected_status.as_str())
        );

        let _ = std::fs::remove_file(saved_path);
    }

    #[test]
    fn browser_open_result_updates_status_message() {
        let path = std::env::temp_dir().join("mxr-browser-open-test.html");
        let mut app = App::new();

        assert!(handle_result(&mut app, AsyncResult::BrowserOpened(Ok(path.clone()))).is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some(format!("Opened in browser: {}", path.display()).as_str())
        );
    }
}
