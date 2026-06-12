use crate::pipeline::ReaderConfig;
use once_cell::sync::Lazy;
use regex::Regex;
use std::process::Command;

static COMMON_HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)</?(?:html|head|body|title|meta|style|script|main|article|section|div|span|p|br|table|thead|tbody|tfoot|tr|td|th|a|img|h[1-6]|ul|ol|li|strong|b|em|i|center|font)\b[^>]*>",
    )
    .expect("common HTML tag regex should compile")
});

/// Convert HTML to plain text.
/// Uses external command if configured, otherwise built-in html2text.
/// Falls back to built-in if the external command fails or is not found.
pub fn to_plain_text(html: &str, config: &ReaderConfig) -> String {
    if let Some(cmd) = &config.html_command {
        match run_external_command(cmd, html) {
            Ok(text) if looks_like_unrendered_html(&text) => {
                tracing::warn!(
                    "External html_command returned raw HTML, falling back to built-in renderer"
                );
            }
            Ok(text) => return text,
            Err(e) => {
                tracing::warn!("External html_command failed, falling back to built-in: {e}");
            }
        }
    }
    builtin_to_plain_text(html)
}

fn builtin_to_plain_text(html: &str) -> String {
    // plain_no_decorate keeps the pre-0.17 behavior: no markdown-style
    // `**bold**` / `*italic*` markers in the reader's plain text.
    html2text::config::plain_no_decorate()
        .string_from_read(html.as_bytes(), 80)
        .unwrap_or_default()
}

pub(crate) fn looks_like_html_document(input: &str) -> bool {
    let trimmed = input.trim_start_matches('\u{feff}').trim_start();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed
        .chars()
        .take(8192)
        .collect::<String>()
        .to_lowercase();
    [
        "<!doctype",
        "<html",
        "<head",
        "<body",
        "<table",
        "<div",
        "<article",
        "<section",
        "<main",
        "<center",
        "<p",
        "<span",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
        || ((lower.starts_with("<!--") || lower.contains("<html"))
            && COMMON_HTML_TAG_RE.find_iter(&lower).take(2).count() >= 2)
}

fn looks_like_unrendered_html(input: &str) -> bool {
    if looks_like_html_document(input) {
        return true;
    }
    COMMON_HTML_TAG_RE.find_iter(input).take(3).count() >= 2
}

fn run_external_command(cmd: &str, input: &str) -> Result<String, HtmlRenderError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let program = parts.first().ok_or(HtmlRenderError::EmptyCommand)?;

    // Check if the program exists before trying to spawn it
    if which::which(program).is_err() {
        return Err(HtmlRenderError::CommandNotFound {
            command: program.to_string(),
            suggestion: "Install it or remove html_command from config to use built-in renderer"
                .into(),
        });
    }

    let output = Command::new(program)
        .args(&parts[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(input.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(HtmlRenderError::ExecutionFailed)?;

    if !output.status.success() {
        return Err(HtmlRenderError::NonZeroExit {
            command: cmd.to_string(),
            code: output.status.code(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Debug)]
pub enum HtmlRenderError {
    EmptyCommand,
    CommandNotFound { command: String, suggestion: String },
    ExecutionFailed(std::io::Error),
    NonZeroExit { command: String, code: Option<i32> },
}

impl std::fmt::Display for HtmlRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCommand => write!(f, "html_command is empty"),
            Self::CommandNotFound {
                command,
                suggestion,
            } => write!(f, "Command '{command}' not found. {suggestion}"),
            Self::ExecutionFailed(e) => write!(f, "Command failed: {e}"),
            Self::NonZeroExit { command, code } => {
                write!(f, "Command '{command}' exited with code {code:?}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_html_to_text() {
        let html = "<html><body><p>Hello <b>world</b></p></body></html>";
        let config = ReaderConfig::default();
        let result = to_plain_text(html, &config);
        assert!(result.contains("Hello"));
        assert!(result.contains("world"));
    }

    #[test]
    fn handles_empty_html() {
        let config = ReaderConfig::default();
        let result = to_plain_text("", &config);
        assert!(result.trim().is_empty());
    }

    #[test]
    fn fallback_to_builtin_when_command_missing() {
        let config = ReaderConfig {
            html_command: Some("nonexistent_command_xyz_12345".into()),
            ..Default::default()
        };
        let result = to_plain_text("<p>Hello</p>", &config);
        // Should fall back to built-in and still produce output
        assert!(result.contains("Hello"));
    }

    #[test]
    fn fallback_to_builtin_when_command_fails() {
        let config = ReaderConfig {
            html_command: Some("false".into()), // `false` exits with code 1
            ..Default::default()
        };
        let result = to_plain_text("<p>Hello from fallback</p>", &config);
        assert!(result.contains("Hello from fallback"));
    }

    #[test]
    fn empty_command_string_falls_back() {
        let config = ReaderConfig {
            html_command: Some(String::new()),
            ..Default::default()
        };
        let result = to_plain_text("<p>Still works</p>", &config);
        assert!(result.contains("Still works"));
    }

    #[test]
    fn external_command_cat_falls_back_when_raw_html_passes_through() {
        // `cat` is universally available and passes stdin to stdout; reader mode
        // must still avoid returning raw markup when a renderer is misconfigured.
        let config = ReaderConfig {
            html_command: Some("cat".into()),
            ..Default::default()
        };
        let result = to_plain_text("<p>Raw HTML</p>", &config);
        assert!(result.contains("Raw HTML"));
        assert!(!result.contains("<p>"));
    }

    #[test]
    fn external_command_with_args() {
        // `wc -c` verifies arguments are passed through without returning HTML.
        let config = ReaderConfig {
            html_command: Some("wc -c".into()),
            ..Default::default()
        };
        let input = "<p>Hello World</p>";
        let result = to_plain_text(input, &config);
        assert_eq!(result.trim(), input.len().to_string());
    }

    #[test]
    fn no_html_command_uses_builtin() {
        let config = ReaderConfig {
            html_command: None,
            ..Default::default()
        };
        let result = to_plain_text("<h1>Title</h1><p>Body text</p>", &config);
        assert!(result.contains("Title"));
        assert!(result.contains("Body text"));
    }

    #[test]
    fn builtin_strips_html_tags() {
        let config = ReaderConfig::default();
        let html = "<div><a href='https://example.com'>Click here</a></div>";
        let result = to_plain_text(html, &config);
        assert!(result.contains("Click here"));
        assert!(!result.contains("<div>"));
    }

    #[test]
    fn command_not_found_error_has_suggestion() {
        let err = run_external_command("nonexistent_cmd_xyz", "test")
            .expect_err("unknown commands should fail");
        let msg = err.to_string();
        assert!(msg.contains("not found"));
        assert!(msg.contains("Install it"));
    }

    #[test]
    fn empty_command_error() {
        let err = run_external_command("", "test").expect_err("empty commands should fail");
        assert!(matches!(err, HtmlRenderError::EmptyCommand));
    }
}
