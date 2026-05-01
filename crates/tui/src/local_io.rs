use crate::app::App;
use crate::async_result::AsyncResult;
use crate::local_state;
use crate::runtime::{submit_task, AsyncResultTask};
use tokio::sync::mpsc;

const BROWSER_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

async fn open_browser_file(
    message_id: mxr_core::MessageId,
    document: String,
) -> Result<std::path::PathBuf, String> {
    let dir = browser_cache_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| format!("failed to prepare browser dir: {error}"))?;

    let path = ensure_browser_cache_file(&dir, &message_id, &document).await?;

    let browser_dir = dir.clone();
    let browser_path = path.clone();
    tokio::task::spawn_blocking(move || {
        open_path_in_browser(&browser_path)?;
        maybe_cleanup_browser_cache(&browser_dir, &browser_path);
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|error| format!("browser opener task failed: {error}"))?
    .map_err(|error| format!("failed to open browser: {error}"))?;

    Ok(path)
}

fn browser_cache_dir() -> std::path::PathBuf {
    mxr_config::data_dir().join("browser")
}

fn browser_cache_path(
    dir: &std::path::Path,
    message_id: &mxr_core::MessageId,
) -> std::path::PathBuf {
    dir.join(format!("{}.html", message_id.as_str()))
}

async fn ensure_browser_cache_file(
    dir: &std::path::Path,
    message_id: &mxr_core::MessageId,
    document: &str,
) -> Result<std::path::PathBuf, String> {
    let path = browser_cache_path(dir, message_id);
    if tokio::fs::try_exists(&path)
        .await
        .map_err(|error| format!("failed to inspect browser cache: {error}"))?
    {
        return Ok(path);
    }

    tokio::fs::write(&path, document)
        .await
        .map_err(|error| format!("failed to write browser file: {error}"))?;
    Ok(path)
}

fn maybe_cleanup_browser_cache(dir: &std::path::Path, keep_path: &std::path::Path) {
    match cleanup_browser_cache_dir(dir, keep_path, std::time::SystemTime::now()) {
        Ok(removed) if removed > 0 => {
            tracing::debug!(removed, keep = %keep_path.display(), "browser cache cleanup removed stale files");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::debug!(
                dir = %dir.display(),
                keep = %keep_path.display(),
                error = %error,
                "browser cache cleanup failed"
            );
        }
    }
}

fn cleanup_browser_cache_dir(
    dir: &std::path::Path,
    keep_path: &std::path::Path,
    now: std::time::SystemTime,
) -> std::io::Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::debug!(dir = %dir.display(), error = %error, "browser cache cleanup skipped unreadable entry");
                continue;
            }
        };

        let path = entry.path();
        if path == keep_path {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("html") {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::debug!(path = %path.display(), error = %error, "browser cache cleanup skipped entry without metadata");
                continue;
            }
        };
        if !is_browser_cache_stale(&metadata, now) {
            continue;
        }

        match std::fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(error) => {
                tracing::debug!(path = %path.display(), error = %error, "browser cache cleanup failed to remove stale file");
            }
        }
    }
    Ok(removed)
}

fn is_browser_cache_stale(metadata: &std::fs::Metadata, now: std::time::SystemTime) -> bool {
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(age) = now.duration_since(modified) else {
        return false;
    };
    age > BROWSER_CACHE_TTL
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
    if let Some(pending) = app.mailbox.pending_browser_open.take() {
        let _ = submit_task(local_io, async move {
            AsyncResult::BrowserOpened(
                open_browser_file(pending.message_id, pending.document).await,
            )
        });
    }

    if app.pending_local_state_save {
        app.pending_local_state_save = false;
        let state = local_state::TuiLocalState {
            onboarding_seen: app.modals.onboarding.seen,
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
            app.diagnostics.page.status = Some(format!("Bug report saved to {}", path.display()));
            None
        }
        AsyncResult::BugReportSaved(Err(error)) => {
            app.diagnostics.page.status = Some(format!("Bug report write failed: {error}"));
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
    use std::time::{Duration as StdDuration, SystemTime};
    use tokio::time::{timeout, Duration};

    fn test_pending_send(draft_path: std::path::PathBuf) -> PendingSend {
        PendingSend {
            account_id: mxr_core::AccountId::new(),
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
        app.compose.pending_send_confirm = Some(test_pending_send(temp.clone()));

        let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(app.compose.pending_send_confirm.is_none());
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
            app.diagnostics.page.status.as_deref(),
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

    #[tokio::test]
    async fn ensure_browser_cache_file_reuses_existing_file_without_overwriting() {
        let dir = unique_browser_test_dir("reuse");
        std::fs::create_dir_all(&dir).expect("create browser cache dir");
        let message_id = mxr_core::MessageId::new();
        let path = browser_cache_path(&dir, &message_id);
        std::fs::write(&path, "cached").expect("write cached browser file");

        let reused = ensure_browser_cache_file(&dir, &message_id, "new content")
            .await
            .expect("reuse cached browser file");

        assert_eq!(reused, path);
        assert_eq!(
            std::fs::read_to_string(&path).expect("read cached browser file"),
            "cached"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_browser_cache_dir_removes_stale_html_only() {
        let dir = unique_browser_test_dir("cleanup");
        std::fs::create_dir_all(&dir).expect("create browser cache dir");

        let keep_path = dir.join("keep.html");
        let stale_html = dir.join("stale.html");
        let fresh_html = dir.join("fresh.html");
        let stale_text = dir.join("stale.txt");
        for path in [&keep_path, &stale_html, &fresh_html, &stale_text] {
            std::fs::write(path, path.display().to_string()).expect("write browser cache fixture");
        }

        let now = SystemTime::now();
        set_modified_time(
            &keep_path,
            now - BROWSER_CACHE_TTL - StdDuration::from_secs(60),
        );
        set_modified_time(
            &stale_html,
            now - BROWSER_CACHE_TTL - StdDuration::from_secs(60),
        );
        set_modified_time(&fresh_html, now - StdDuration::from_secs(60));
        set_modified_time(
            &stale_text,
            now - BROWSER_CACHE_TTL - StdDuration::from_secs(60),
        );

        let removed =
            cleanup_browser_cache_dir(&dir, &keep_path, now).expect("cleanup browser cache dir");

        assert_eq!(removed, 1, "only one stale html file should be removed");
        assert!(
            keep_path.exists(),
            "currently opened file must be preserved"
        );
        assert!(!stale_html.exists(), "stale html should be removed");
        assert!(fresh_html.exists(), "fresh html should be kept");
        assert!(stale_text.exists(), "non-html files should be ignored");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unique_browser_test_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mxr-browser-cache-{label}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn set_modified_time(path: &std::path::Path, modified: SystemTime) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open browser cache fixture");
        let times = std::fs::FileTimes::new().set_modified(modified);
        file.set_times(times)
            .expect("set browser cache fixture mtime");
    }
}
