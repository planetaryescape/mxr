use crate::pipeline::ReaderConfig;
use std::process::Command;

/// Convert HTML to plain text.
/// Uses external command if configured, otherwise built-in html2text.
pub fn to_plain_text(html: &str, config: &ReaderConfig) -> String {
    if let Some(cmd) = &config.html_command {
        match run_external_command(cmd, html) {
            Ok(text) => return text,
            Err(e) => {
                tracing::warn!("External html_command failed, falling back to built-in: {e}");
            }
        }
    }
    html2text::from_read(html.as_bytes(), 80).unwrap_or_default()
}

fn run_external_command(cmd: &str, input: &str) -> Result<String, std::io::Error> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let output = Command::new(parts[0])
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
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
}
