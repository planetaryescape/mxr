use crate::pipeline::ReaderConfig;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::cell::Cell;
use std::panic::AssertUnwindSafe;
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
    html_to_plain_text(html, "reader")
}

/// Largest colspan/rowspan html2text is given. Real mail stays far below
/// this; anything above it is a malformed or hostile table.
pub const MAX_TABLE_SPAN: u64 = 1000;

static TABLE_SPAN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b(colspan|rowspan)(\s*=\s*["']?\s*)(\d+)"#)
        .expect("table span regex should compile")
});

/// Clamps `colspan` and `rowspan` attribute values to `MAX_TABLE_SPAN`.
///
/// html2text 0.17.4 overflows its column arithmetic on huge spans (a colspan
/// near `usize::MAX` panics), so clamping keeps the common case in the
/// renderer itself. Values that don't fit a u64 are clamped too. Values at or
/// under the cap come back as the same text.
pub fn clamp_table_spans(html: &str) -> Cow<'_, str> {
    TABLE_SPAN_RE.replace_all(html, |caps: &Captures<'_>| {
        let value = caps[3]
            .parse::<u64>()
            .map_or(MAX_TABLE_SPAN, |n| n.min(MAX_TABLE_SPAN));
        format!("{}{}{value}", &caps[1], &caps[2])
    })
}

thread_local! {
    // Set while a guarded html2text call runs on this thread. A panic hook
    // runs on the panicking thread, before unwinding reaches the guard, so a
    // thread-local is the right scope for "this panic is about to be caught".
    static IN_GUARDED_RENDER: Cell<bool> = const { Cell::new(false) };
}

/// True while the current thread is inside `guard_html2text`. A panic hook
/// checks this to stay quiet for a panic the guard is about to catch.
pub fn in_guarded_render() -> bool {
    IN_GUARDED_RENDER.with(Cell::get)
}

/// Restores the previous flag value on drop, so nested guards and unwinding
/// both leave the thread in the state they found it.
struct GuardedRenderFlag {
    previous: bool,
}

impl GuardedRenderFlag {
    fn enter() -> Self {
        let previous = IN_GUARDED_RENDER.with(|flag| flag.replace(true));
        Self { previous }
    }
}

impl Drop for GuardedRenderFlag {
    fn drop(&mut self) {
        IN_GUARDED_RENDER.with(|flag| flag.set(self.previous));
    }
}

/// Runs an html2text call and returns `None` if it panics.
///
/// html2text 0.17.4 panics on some malformed tables (a colspan near
/// `usize::MAX` overflows its column arithmetic), and an unwinding panic
/// would take down whichever TUI, CLI or daemon handler is rendering the
/// message. `site` names the call site in the warning; the panic payload and
/// the message body are never logged.
pub fn guard_html2text<T>(site: &'static str, render: impl FnOnce() -> T) -> Option<T> {
    let _flag = GuardedRenderFlag::enter();
    match std::panic::catch_unwind(AssertUnwindSafe(render)) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::warn!(
                site,
                "html2text panicked on malformed HTML, using plain-text fallback"
            );
            None
        }
    }
}

/// Plain text for HTML mail at the reader's width, with the default
/// (undecorated) renderer. Never panics: when html2text fails, the tag strip
/// below stands in so the message still shows its text.
pub fn html_to_plain_text(html: &str, site: &'static str) -> String {
    // plain_no_decorate keeps the pre-0.17 behavior: no markdown-style
    // `**bold**` / `*italic*` markers in the reader's plain text.
    let clamped = clamp_table_spans(html);
    guard_html2text(site, || {
        html2text::config::plain_no_decorate().string_from_read(clamped.as_bytes(), 80)
    })
    .and_then(Result::ok)
    .unwrap_or_else(|| strip_html_tags(html))
}

static STYLE_OR_SCRIPT_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    // An unclosed <style> or <script> runs to the end of the input, so it
    // can't leak its CSS or JS as text.
    Regex::new(r"(?is)<(?:style|script)\b[^>]*>.*?(?:</(?:style|script)\s*>|\z)")
        .expect("style/script block regex should compile")
});

static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| {
    // Requiring a letter, `/` or `!` after `<` keeps "a < b" as text.
    Regex::new(r"<[a-zA-Z/!][^>]*>").expect("html tag regex should compile")
});

/// Best-effort text from HTML with no renderer: drops `<style>`/`<script>`
/// bodies and tags, decodes the common entities and collapses whitespace.
/// Used when html2text can't render a document.
pub fn strip_html_tags(html: &str) -> String {
    let without_blocks = STYLE_OR_SCRIPT_BLOCK_RE.replace_all(html, " ");
    let without_tags = HTML_TAG_RE.replace_all(&without_blocks, " ");
    let decoded = without_tags
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        // Decode &amp; last so it can't manufacture another entity.
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
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
    fn malformed_table_span_returns_text_instead_of_panicking() {
        // html2text 0.17.4 overflows on this colspan; a panic here would crash
        // the TUI, `mxr cat`, or a daemon handler for a single hostile email.
        let html = r#"<table><tr><td colspan="18446744073709551615">A</td><td>B</td></tr></table>"#;
        let output = crate::clean(None, Some(html), &ReaderConfig::default());
        assert!(output.content.contains('A'));
        assert!(output.content.contains('B'));
    }

    #[test]
    fn strip_fallback_drops_style_and_script_and_decodes_entities() {
        let html = "<style>p{color:red}</style><p>Tom &amp; Jerry &lt;3</p><script>x()</script>";
        assert_eq!(strip_html_tags(html), "Tom & Jerry <3");
    }

    #[test]
    fn strip_fallback_drops_an_unclosed_style_block_to_the_end() {
        assert_eq!(strip_html_tags("<p>ok</p><style>a{}"), "ok");
        assert_eq!(strip_html_tags("a < b and c"), "a < b and c");
    }

    #[test]
    fn huge_colspan_is_clamped_and_html2text_renders_it_without_fallback() {
        let html = r#"<table><tr><td colspan="18446744073709551615">A</td><td>B</td></tr></table>"#;
        let clamped = clamp_table_spans(html);
        // Call html2text directly: an Err or panic here would mean the clamp
        // did not keep the renderer away from the overflow.
        let text = html2text::config::plain_no_decorate()
            .string_from_read(clamped.as_bytes(), 80)
            .expect("clamped table should render");
        assert!(text.contains('A') && text.contains('B'), "got {text:?}");
    }

    #[test]
    fn normal_spans_are_left_untouched() {
        let html = r#"<table><tr><td colspan="2">A</td><td rowspan='3'>B</td></tr></table>"#;
        assert_eq!(clamp_table_spans(html), html);
    }

    #[test]
    fn spans_above_the_cap_are_clamped_to_it() {
        let html = format!(r#"<td colspan="{}">x</td>"#, MAX_TABLE_SPAN + 5);
        assert_eq!(
            clamp_table_spans(&html),
            format!(r#"<td colspan="{MAX_TABLE_SPAN}">x</td>"#)
        );
    }

    #[test]
    fn guarded_render_flag_is_set_only_inside_the_guard() {
        assert!(!in_guarded_render());
        let inside = guard_html2text("test", in_guarded_render);
        assert_eq!(inside, Some(true));
        assert!(!in_guarded_render());
    }

    #[test]
    #[allow(clippy::panic, reason = "the test needs a real panic for the guard to catch")]
    fn guarded_render_flag_is_reset_after_a_caught_panic() {
        let result: Option<()> = guard_html2text("test", || panic!("probe"));
        assert!(result.is_none());
        assert!(!in_guarded_render());
    }

    #[test]
    fn style_only_body_yields_no_css_text() {
        let html = "<html><head><style>p { color: red; }</style></head><body></body></html>";
        let output = crate::clean(None, Some(html), &ReaderConfig::default());
        assert!(!output.content.contains("color"));
    }

    #[test]
    fn empty_command_error() {
        let err = run_external_command("", "test").expect_err("empty commands should fail");
        assert!(matches!(err, HtmlRenderError::EmptyCommand));
    }
}
