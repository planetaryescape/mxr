mod attachments;
mod compose;
mod helpers;

pub use attachments::{attachments_download, attachments_list, attachments_open};
pub use compose::{compose, drafts, forward, reply, reply_all, send_draft, ComposeOptions};

use crate::ipc_client::IpcClient;
use helpers::{
    confirm_action, parse_message_id, parse_snooze_until, print_selection_preview,
    requires_confirmation, resolve_mutation_selection, run_simple_mutation, MutationRunOptions,
};
use mxr_protocol::*;

const BROWSER_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

// ---------------------------------------------------------------------------
// Simple mutations
// ---------------------------------------------------------------------------

pub async fn archive(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "archive",
            success_message: "Archived",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| Request::Mutation(MutationCommand::Archive { message_ids: ids }),
    )
    .await
}

pub async fn read_archive(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as read and archive",
            success_message: "Marked as read and archived",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| Request::Mutation(MutationCommand::ReadAndArchive { message_ids: ids }),
    )
    .await
}

pub async fn trash(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "trash",
            success_message: "Trashed",
            yes,
            dry_run,
            destructive: true,
        },
        |ids| Request::Mutation(MutationCommand::Trash { message_ids: ids }),
    )
    .await
}

pub async fn spam(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as spam",
            success_message: "Marked as spam",
            yes,
            dry_run,
            destructive: true,
        },
        |ids| Request::Mutation(MutationCommand::Spam { message_ids: ids }),
    )
    .await
}

pub async fn star(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "star",
            success_message: "Starred",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::Star {
                message_ids: ids,
                starred: true,
            })
        },
    )
    .await
}

pub async fn unstar(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "unstar",
            success_message: "Unstarred",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::Star {
                message_ids: ids,
                starred: false,
            })
        },
    )
    .await
}

pub async fn mark_read(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as read",
            success_message: "Marked as read",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: true,
            })
        },
    )
    .await
}

pub async fn unread(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as unread",
            success_message: "Marked as unread",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: false,
            })
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Label mutations
// ---------------------------------------------------------------------------

pub async fn label(
    name: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("add label '{name}'"),
            success_message: &format!("Added label '{name}'"),
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: ids,
                add: vec![name.clone()],
                remove: vec![],
            })
        },
    )
    .await
}

pub async fn unlabel(
    name: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("remove label '{name}'"),
            success_message: &format!("Removed label '{name}'"),
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: ids,
                add: vec![],
                remove: vec![name.clone()],
            })
        },
    )
    .await
}

pub async fn move_msg(
    target_label: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("move to '{target_label}'"),
            success_message: &format!("Moved to '{target_label}'"),
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::Move {
                message_ids: ids,
                target_label: target_label.clone(),
            })
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Snooze
// ---------------------------------------------------------------------------

pub async fn snooze(
    message_id: Option<String>,
    until: String,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let wake_at = parse_snooze_until(&until)?;
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_selection_preview(
            &format!("snooze until {}", wake_at.to_rfc3339()),
            &selection,
        );
        return Ok(());
    }
    if requires_confirmation(false, selection.used_search, selection.ids.len(), yes) {
        confirm_action(
            &format!("snooze until {}", wake_at.to_rfc3339()),
            &selection,
        )?;
    }
    for id in &selection.ids {
        let resp = client
            .request(Request::Snooze {
                message_id: id.clone(),
                wake_at,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {}
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    println!(
        "Snoozed {} message(s) until {}",
        selection.ids.len(),
        wake_at.to_rfc3339()
    );
    Ok(())
}

pub async fn unsnooze(message_id: Option<String>, all: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    if all {
        let resp = client.request(Request::ListSnoozed).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            } => {
                if snoozed.is_empty() {
                    println!("No snoozed messages");
                    return Ok(());
                }
                for s in &snoozed {
                    let resp = client
                        .request(Request::Unsnooze {
                            message_id: s.message_id.clone(),
                        })
                        .await?;
                    match resp {
                        Response::Ok {
                            data: ResponseData::Ack,
                        } => {}
                        Response::Error { message } => {
                            eprintln!("Failed to unsnooze {}: {message}", s.message_id);
                        }
                        _ => {}
                    }
                }
                println!("Unsnoozed {} message(s)", snoozed.len());
            }
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    } else {
        let id_str = message_id.ok_or_else(|| anyhow::anyhow!("Provide a message ID or --all"))?;
        let id = parse_message_id(&id_str)?;
        let resp = client.request(Request::Unsnooze { message_id: id }).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => println!("Unsnoozed"),
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    Ok(())
}

pub async fn snoozed() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListSnoozed).await?;
    match resp {
        Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        } => {
            if snoozed.is_empty() {
                println!("No snoozed messages");
            } else {
                println!(
                    "{:<38} {:<25} {:<25}",
                    "MESSAGE ID", "SNOOZED AT", "WAKE AT"
                );
                println!("{}", "-".repeat(88));
                for s in &snoozed {
                    println!(
                        "{:<38} {:<25} {:<25}",
                        s.message_id.as_str(),
                        s.snoozed_at.to_rfc3339(),
                        s.wake_at.to_rfc3339(),
                    );
                }
                println!("\n{} snoozed message(s)", snoozed.len());
            }
        }
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unsubscribe / Open
// ---------------------------------------------------------------------------

pub async fn unsubscribe(
    message_id: Option<String>,
    yes: bool,
    search: Option<String>,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_selection_preview("unsubscribe", &selection);
        return Ok(());
    }
    if requires_confirmation(true, selection.used_search, selection.ids.len(), yes) {
        confirm_action("unsubscribe", &selection)?;
    }
    for id in &selection.ids {
        let resp = client
            .request(Request::Unsubscribe {
                message_id: id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {}
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    println!("Unsubscribed from {} message(s)", selection.ids.len());
    Ok(())
}

pub async fn open_in_browser(message_id: String) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let dir = browser_cache_dir();
    let path = browser_cache_path(&dir, &id);
    if path.exists() {
        open_browser_path(&path)?;
        maybe_cleanup_browser_cache(&dir, &path);
        println!("Opened in browser: {}", path.display());
        return Ok(());
    }

    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetBody {
            message_id: id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => {
            let Some(document) = browser_document(&body) else {
                anyhow::bail!("No readable body available for {}", id.as_str());
            };
            ensure_browser_cache_file(&dir, &id, &document)?;
            open_browser_path(&path)?;
            maybe_cleanup_browser_cache(&dir, &path);
            println!("Opened in browser: {}", path.display());
        }
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
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

fn ensure_browser_cache_file(
    dir: &std::path::Path,
    message_id: &mxr_core::MessageId,
    document: &str,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = browser_cache_path(dir, message_id);
    if path.exists() {
        return Ok(path);
    }

    std::fs::write(&path, document)?;
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

fn open_browser_path(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let program = "open";

    let status = std::process::Command::new(program).arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("{program} exited with status {status}")
    }
}

fn browser_document(body: &mxr_core::MessageBody) -> Option<String> {
    body.text_html
        .clone()
        .or_else(|| {
            body.text_plain
                .as_deref()
                .map(render_plain_text_browser_document)
        })
        .or_else(|| {
            body.best_effort_readable_summary()
                .map(|text| render_plain_text_browser_document(&text))
        })
}

fn render_plain_text_browser_document(text: &str) -> String {
    let escaped = htmlescape::encode_minimal(text);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>mxr message</title><style>body{{margin:2rem;font:16px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:#fafafa;color:#111;}}pre{{white-space:pre-wrap;word-break:break-word;}}</style></head><body><pre>{escaped}</pre></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration as StdDuration, SystemTime};

    #[test]
    fn browser_document_prefers_html_when_available() {
        let body = mxr_core::MessageBody {
            message_id: mxr_core::MessageId::new(),
            text_plain: Some("plain".into()),
            text_html: Some("<p>html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        };

        assert_eq!(browser_document(&body).as_deref(), Some("<p>html</p>"));
    }

    #[test]
    fn browser_document_wraps_plain_text_for_browser_rendering() {
        let body = mxr_core::MessageBody {
            message_id: mxr_core::MessageId::new(),
            text_plain: Some("hello <world>\nline 2".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        };

        let document = browser_document(&body).expect("plain text fallback");
        assert!(document.contains("<pre>hello &lt;world&gt;\nline 2</pre>"));
        assert!(document.contains("<!doctype html>"));
    }

    #[test]
    fn browser_document_wraps_best_effort_fallback_for_browser_rendering() {
        let body = mxr_core::MessageBody {
            message_id: mxr_core::MessageId::new(),
            text_plain: None,
            text_html: None,
            attachments: vec![mxr_core::AttachmentMeta {
                id: mxr_core::AttachmentId::new(),
                message_id: mxr_core::MessageId::new(),
                filename: "invite.ics".into(),
                mime_type: "text/calendar".into(),
                disposition: mxr_core::AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::MessageMetadata {
                calendar: Some(mxr_core::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Demo call".into()),
                }),
                ..Default::default()
            },
        };

        let document = browser_document(&body).expect("best-effort fallback");
        assert!(document.contains("Calendar invite"));
        assert!(document.contains("Summary: Demo call"));
        assert!(document.contains("<!doctype html>"));
    }

    #[test]
    fn ensure_browser_cache_file_reuses_existing_file_without_overwriting() {
        let dir = unique_browser_test_dir("reuse");
        std::fs::create_dir_all(&dir).expect("create browser cache dir");
        let message_id = mxr_core::MessageId::new();
        let path = browser_cache_path(&dir, &message_id);
        std::fs::write(&path, "cached").expect("write cached browser file");

        let reused = ensure_browser_cache_file(&dir, &message_id, "new content")
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
            "mxr-daemon-browser-cache-{label}-{}-{}",
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
