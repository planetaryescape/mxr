//! Delivers a rendered thread export to the file the user named or to the
//! system clipboard. Errors name the path or program, never the content, and
//! nothing here logs the bytes.

use crate::app::{ExportDestination, PendingExport};
use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::id::ThreadId;
use mxr_core::ExportFormat;
use mxr_protocol::{Request, Response, ResponseData};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tokio::sync::mpsc;

/// Picker order. The keys below match docs/blueprint/11-export.md.
pub const FORMAT_CHOICES: [ExportFormat; 4] = [
    ExportFormat::Markdown,
    ExportFormat::Json,
    ExportFormat::Mbox,
    ExportFormat::LlmContext,
];

pub fn format_label(format: &ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "Markdown",
        ExportFormat::Json => "JSON",
        ExportFormat::Mbox => "Mbox",
        ExportFormat::LlmContext => "LLM context",
    }
}

pub fn format_key(format: &ExportFormat) -> char {
    match format {
        ExportFormat::Markdown => 'm',
        ExportFormat::Json => 'j',
        ExportFormat::Mbox => 'x',
        ExportFormat::LlmContext => 'l',
    }
}

/// Follows what the exporter emits. LLM context is plain text, not Markdown.
pub fn format_extension(format: &ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Json => "json",
        ExportFormat::Mbox => "mbox",
        ExportFormat::LlmContext => "txt",
    }
}

/// The prefilled save path. `~` is expanded when the user confirms.
pub fn default_export_path(thread_id: &ThreadId, format: &ExportFormat) -> String {
    format!(
        "~/mxr/exports/mxr-thread-{thread_id}.{}",
        format_extension(format)
    )
}

fn export_request(pending: &PendingExport) -> Request {
    Request::ExportThread {
        thread_id: pending.thread_id.clone(),
        format: pending.format.clone(),
    }
}

/// Asks the daemon for the rendered thread and delivers it. The returned
/// string is the status line; it names the destination, not the content.
pub(crate) async fn export_thread(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    pending: PendingExport,
) -> Result<String, String> {
    let label = format_label(&pending.format);
    let content = match ipc_call(bg, export_request(&pending)).await {
        Ok(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => content,
        Ok(Response::Error { message, .. }) => return Err(message),
        Err(error) => return Err(error.to_string()),
        Ok(_) => return Err("unexpected response to ExportThread".into()),
    };

    match pending.destination {
        ExportDestination::Clipboard => {
            tokio::task::spawn_blocking(move || copy_to_clipboard(&content))
                .await
                .map_err(|error| format!("clipboard task failed: {error}"))??;
            Ok(format!("Copied {label} export to clipboard"))
        }
        ExportDestination::Save { path, overwrite_ok } => {
            let shown = path.display().to_string();
            tokio::task::spawn_blocking(move || write_export_file(&path, &content, overwrite_ok))
                .await
                .map_err(|error| format!("write task failed: {error}"))??;
            Ok(format!("Exported {label} to {shown}"))
        }
    }
}

/// Writes the exact bytes. Without `overwrite_ok` the file must not exist:
/// `create_new` makes a file that appeared after the user's last look fail
/// instead of being replaced.
pub fn write_export_file(path: &Path, content: &str, overwrite_ok: bool) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if overwrite_ok {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{} already exists", path.display()),
        _ => format!("could not write {}: {error}", path.display()),
    })?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn copy_to_clipboard(content: &str) -> Result<(), String> {
    let (program, args) = clipboard_command()?;
    run_clipboard_command(program, args, content)
}

fn clipboard_command() -> Result<(&'static str, &'static [&'static str]), String> {
    const XCLIP_ARGS: &[&str] = &["-selection", "clipboard"];
    if cfg!(target_os = "macos") {
        return Ok(("pbcopy", &[]));
    }
    if cfg!(target_os = "linux") {
        return Ok(if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            ("wl-copy", &[])
        } else {
            ("xclip", XCLIP_ARGS)
        });
    }
    Err("no clipboard command is known for this platform".into())
}

/// Pipes `content` to `program`'s stdin and reports only how it ended. The
/// program's output is discarded so nothing from it reaches the terminal or
/// the error text.
pub fn run_clipboard_command(program: &str, args: &[&str], content: &str) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => format!("{program} is not installed"),
            _ => format!("could not start {program}: {error}"),
        })?;

    // Dropping stdin at the end of the arm closes the pipe so the program sees
    // EOF. The write error is held until the child is reaped.
    let write_result = match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(content.as_bytes()),
        None => Ok(()),
    };
    let status = child
        .wait()
        .map_err(|error| format!("could not wait for {program}: {error}"))?;
    write_result.map_err(|error| format!("could not write to {program}: {error}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_writes_the_exact_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("thread.md");
        let content = "# Thread: héllo\r\n\r\nno trailing newline";
        write_export_file(&path, content, false).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), content.as_bytes());
    }

    #[test]
    fn save_refuses_an_existing_file_unless_overwrite_is_confirmed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("thread.md");
        std::fs::write(&path, "keep me").unwrap();

        let error = write_export_file(&path, "new", false).unwrap_err();
        assert!(error.contains("already exists"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");

        write_export_file(&path, "new", true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("exports/nested/thread.json");
        write_export_file(&path, "{}", false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn save_reports_a_parent_that_is_a_file() {
        let dir = tempdir().unwrap();
        let blocker = dir.path().join("exports");
        std::fs::write(&blocker, "a file").unwrap();
        let error = write_export_file(&blocker.join("thread.md"), "x", false).unwrap_err();
        assert!(error.starts_with("could not create"), "{error}");
    }

    #[test]
    fn clipboard_program_receives_the_content_on_stdin() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("clipboard.txt");
        let out_arg = out.display().to_string();
        run_clipboard_command("sh", &["-c", "cat > \"$0\"", &out_arg], "Thread: a\n---\n").unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "Thread: a\n---\n");
    }

    #[test]
    fn clipboard_program_exiting_nonzero_is_an_error() {
        let error =
            run_clipboard_command("sh", &["-c", "cat >/dev/null; exit 1"], "content").unwrap_err();
        assert!(error.contains("exited with"), "{error}");
    }

    #[test]
    fn missing_clipboard_program_is_an_error() {
        let error =
            run_clipboard_command("mxr-no-such-clipboard-program", &[], "content").unwrap_err();
        assert_eq!(error, "mxr-no-such-clipboard-program is not installed");
    }

    #[test]
    fn every_format_is_sent_in_the_request() {
        let thread_id = ThreadId::new();
        for format in FORMAT_CHOICES {
            let pending = PendingExport {
                thread_id: thread_id.clone(),
                format: format.clone(),
                destination: ExportDestination::Clipboard,
            };
            match export_request(&pending) {
                Request::ExportThread {
                    thread_id: sent_id,
                    format: sent_format,
                } => {
                    assert_eq!(sent_id, thread_id);
                    assert_eq!(sent_format, format);
                }
                other => panic!("unexpected request {other:?}"),
            }
        }
    }

    #[test]
    fn default_path_uses_the_extension_the_exporter_emits() {
        let thread_id = ThreadId::new();
        assert_eq!(
            default_export_path(&thread_id, &ExportFormat::LlmContext),
            format!("~/mxr/exports/mxr-thread-{thread_id}.txt")
        );
        assert!(default_export_path(&thread_id, &ExportFormat::Mbox).ends_with(".mbox"));
    }
}
