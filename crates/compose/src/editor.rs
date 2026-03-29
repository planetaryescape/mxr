use crate::mxr_compose::frontmatter::ComposeError;
use std::path::Path;
use tokio::process::Command;

/// Resolve which editor to use.
/// Priority: $EDITOR -> $VISUAL -> config_editor -> vi
pub fn resolve_editor(config_editor: Option<&str>) -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            config_editor
                .map(|s| s.to_string())
                .unwrap_or_else(|| "vi".to_string())
        })
}

/// Spawn the editor and wait for it to exit.
/// For vim/neovim, positions cursor at the given line number.
pub async fn spawn_editor(
    editor: &str,
    file_path: &Path,
    cursor_line: Option<usize>,
) -> Result<bool, ComposeError> {
    let mut cmd = Command::new(editor);

    // Position cursor for vim/neovim/vi
    if let Some(line) = cursor_line {
        let editor_lower = editor.to_lowercase();
        if editor_lower.contains("vim") || editor_lower == "vi" || editor_lower.contains("nvim") {
            cmd.arg(format!("+{line}"));
        } else if editor_lower.contains("hx") || editor_lower.contains("helix") {
            let path_str = format!("{}:{line}", file_path.display());
            let status = Command::new(editor)
                .arg(&path_str)
                .status()
                .await
                .map_err(|e| ComposeError::EditorFailed(e.to_string()))?;
            return Ok(status.success());
        }
    }

    cmd.arg(file_path);

    let status = cmd
        .status()
        .await
        .map_err(|e| ComposeError::EditorFailed(e.to_string()))?;

    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_editor_env_var() {
        let result = temp_env::with_var("EDITOR", Some("nvim"), || resolve_editor(None));

        assert_eq!(result, "nvim");
    }

    #[test]
    fn resolve_editor_fallback() {
        let result =
            temp_env::with_vars([("EDITOR", None::<&str>), ("VISUAL", None::<&str>)], || {
                resolve_editor(None)
            });

        assert_eq!(result, "vi");
    }

    #[test]
    fn resolve_editor_config() {
        let result =
            temp_env::with_vars([("EDITOR", None::<&str>), ("VISUAL", None::<&str>)], || {
                resolve_editor(Some("nano"))
            });

        assert_eq!(result, "nano");
    }
}
