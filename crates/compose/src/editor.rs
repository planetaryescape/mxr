use crate::frontmatter::ComposeError;
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
/// `editor` may be a bare program (`vim`) or a shell command (`code --wait`,
/// `flatpak run org.gnome.gedit`); we split on whitespace honoring quotes.
/// For vim/neovim, positions cursor at the given line number.
pub async fn spawn_editor(
    editor: &str,
    file_path: &Path,
    cursor_line: Option<usize>,
) -> Result<bool, ComposeError> {
    let parts = shell_words::split(editor)
        .map_err(|e| ComposeError::EditorFailed(format!("invalid $EDITOR `{editor}`: {e}")))?;
    let (program, prefix_args) = parts
        .split_first()
        .ok_or_else(|| ComposeError::EditorFailed("$EDITOR is empty".into()))?;

    let program_lower = program.to_lowercase();
    let mut cmd = Command::new(program);
    for arg in prefix_args {
        cmd.arg(arg);
    }

    if let Some(line) = cursor_line {
        if program_lower.contains("vim")
            || program_lower == "vi"
            || program_lower.contains("nvim")
        {
            cmd.arg(format!("+{line}"));
        } else if program_lower.contains("hx") || program_lower.contains("helix") {
            cmd.arg(format!("{}:{line}", file_path.display()));
            let status = cmd
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

    #[tokio::test]
    async fn spawn_editor_handles_shell_command_string() {
        // `true file` exits 0; we just want to prove command-string parsing succeeds.
        let path = std::env::temp_dir().join("mxr-editor-shell-string.tmp");
        std::fs::write(&path, "x").unwrap();
        let ok = spawn_editor("/usr/bin/true file", &path, None).await.unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(ok);
    }

    #[tokio::test]
    async fn spawn_editor_rejects_empty_editor() {
        let path = std::env::temp_dir().join("mxr-editor-empty.tmp");
        let err = spawn_editor("", &path, None).await.unwrap_err();
        assert!(err.to_string().contains("$EDITOR is empty"));
    }
}
