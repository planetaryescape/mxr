use crate::mxr_config::load_config;
use crate::mxr_core::MxrError;
use crate::mxr_tui::app::{self, App};

pub(crate) fn edit_tui_config(app: &mut App) -> Result<String, MxrError> {
    let config_path = crate::mxr_config::config_file_path();
    let current_config = load_config().map_err(|error| MxrError::Ipc(error.to_string()))?;

    if !config_path.exists() {
        crate::mxr_config::save_config(&current_config)
            .map_err(|error| MxrError::Ipc(error.to_string()))?;
    }

    let editor =
        crate::mxr_compose::editor::resolve_editor(current_config.general.editor.as_deref());
    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok("Config edit cancelled".into());
    }

    let reloaded = load_config().map_err(|error| MxrError::Ipc(error.to_string()))?;
    app.apply_runtime_config(&reloaded);
    app.accounts_page.refresh_pending = true;
    app.pending_status_refresh = true;

    Ok("Config reloaded. Restart daemon for account/provider changes.".into())
}

pub(crate) fn open_tui_log_file() -> Result<String, MxrError> {
    let log_path = crate::mxr_config::data_dir().join("logs").join("mxr.log");
    if !log_path.exists() {
        return Err(MxrError::Ipc(format!(
            "log file not found at {}",
            log_path.display()
        )));
    }

    let editor = load_config()
        .ok()
        .and_then(|config| config.general.editor)
        .map_or_else(|| crate::mxr_compose::editor::resolve_editor(None), |editor| crate::mxr_compose::editor::resolve_editor(Some(editor.as_str())));
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
    let path = std::env::temp_dir().join(format!(
        "mxr-{}-{}.txt",
        name,
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, content)
        .map_err(|error| MxrError::Ipc(format!("failed to write temp file: {error}")))?;

    let editor = load_config()
        .ok()
        .and_then(|config| config.general.editor)
        .map_or_else(|| crate::mxr_compose::editor::resolve_editor(None), |editor| crate::mxr_compose::editor::resolve_editor(Some(editor.as_str())));
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|error| MxrError::Ipc(format!("failed to launch editor: {error}")))?;

    if !status.success() {
        return Ok(format!(
            "Diagnostics detail open cancelled ({})",
            path.display()
        ));
    }

    Ok(format!("Opened diagnostics details at {}", path.display()))
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
    };
    let content = crate::mxr_tui::ui::diagnostics_page::pane_details_text(state, pane);
    open_temp_text_buffer(name, &content)
}
