use crate::app::{self, App};
use mxr_config::load_config;
use mxr_core::MxrError;

pub(crate) fn edit_tui_config(app: &mut App) -> Result<String, MxrError> {
    let config_path = mxr_config::config_file_path();
    let current_config = load_config().map_err(|error| MxrError::Ipc(error.to_string()))?;

    if !config_path.exists() {
        mxr_config::update_config(|_| Ok::<_, mxr_config::ConfigError>(()))
            .map_err(|error| MxrError::Ipc(error.to_string()))?;
    }

    let editor = mxr_compose::editor::resolve_editor(current_config.general.editor.as_deref());
    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok("Config edit cancelled".into());
    }

    let reloaded = load_config().map_err(|error| MxrError::Ipc(error.to_string()))?;
    app.apply_runtime_config(&reloaded);
    app.accounts.page.refresh_pending = true;
    app.diagnostics.pending_status_refresh = true;

    Ok("Config reloaded. Restart daemon for account/provider changes.".into())
}

pub(crate) fn open_tui_log_file() -> Result<String, MxrError> {
    let log_path = mxr_config::data_dir().join("logs").join("mxr.log");
    if !log_path.exists() {
        return Err(MxrError::Ipc(format!(
            "log file not found at {}",
            log_path.display()
        )));
    }

    let editor = load_config()
        .ok()
        .and_then(|config| config.general.editor)
        .map_or_else(
            || mxr_compose::editor::resolve_editor(None),
            |editor| mxr_compose::editor::resolve_editor(Some(editor.as_str())),
        );
    let status = std::process::Command::new(&editor)
        .arg(&log_path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok("Log open cancelled".into());
    }

    Ok(format!("Opened logs at {}", log_path.display()))
}

pub(crate) fn open_temp_text_buffer(name: &str, content: &str) -> Result<String, MxrError> {
    let scratch = mxr_compose::private_tmp::private_scratch_dir()
        .map_err(|error| MxrError::Ipc(format!("failed to prepare scratch dir: {error}")))?;
    let path = scratch.join(format!(
        "mxr-{}-{}.txt",
        name,
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    mxr_compose::private_tmp::write_private(&path, content.as_bytes())
        .map_err(|error| MxrError::Ipc(format!("failed to write temp file: {error}")))?;

    let editor = load_config()
        .ok()
        .and_then(|config| config.general.editor)
        .map_or_else(
            || mxr_compose::editor::resolve_editor(None),
            |editor| mxr_compose::editor::resolve_editor(Some(editor.as_str())),
        );
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")));

    let _ = std::fs::remove_file(&path);

    let status = status?;
    if !status.success() {
        return Ok("Diagnostics detail open cancelled".into());
    }

    Ok("Opened diagnostics details".into())
}

pub(crate) fn open_diagnostics_pane_details(
    state: &app::DiagnosticsPageState,
    pane: app::DiagnosticsPaneKind,
) -> Result<String, MxrError> {
    if pane == app::DiagnosticsPaneKind::Logs {
        return open_tui_log_file();
    }

    let name = match pane {
        app::DiagnosticsPaneKind::Status => "doctor",
        app::DiagnosticsPaneKind::Data => "storage",
        app::DiagnosticsPaneKind::Sync => "sync-health",
        app::DiagnosticsPaneKind::Events => "events",
        app::DiagnosticsPaneKind::Logs => "logs",
        app::DiagnosticsPaneKind::Jobs => "jobs",
        app::DiagnosticsPaneKind::Activity => "activity",
    };
    let content = crate::ui::diagnostics_page::pane_details_text(state, pane);
    open_temp_text_buffer(name, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_temp_text_buffer_deletes_file_after_cancelled_editor() {
        temp_env::with_var("EDITOR", Some("false"), || {
            let result = temp_env::with_var("VISUAL", None::<&str>, || {
                open_temp_text_buffer("test-diag", "diagnostics content")
            });
            assert!(result.is_ok(), "expected Ok, got: {result:?}");
            let msg = result.unwrap();
            assert!(
                msg.contains("cancelled") || msg.contains("Opened"),
                "unexpected message: {msg}"
            );
        });

        let scratch = mxr_compose::private_tmp::private_scratch_dir().expect("private scratch dir");
        let leftover: Vec<_> = std::fs::read_dir(&scratch)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("mxr-test-diag-")
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected no leftover test-diag files, found: {leftover:?}"
        );
    }
}
