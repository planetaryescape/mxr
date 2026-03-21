use serde::{Deserialize, Serialize};

/// YAML frontmatter for compose files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeFrontmatter {
    pub to: String,
    #[serde(default)]
    pub cc: String,
    #[serde(default)]
    pub bcc: String,
    pub subject: String,
    pub from: String,
    #[serde(
        default,
        rename = "in-reply-to",
        skip_serializing_if = "Option::is_none"
    )]
    pub in_reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    #[serde(default)]
    pub attach: Vec<String>,
}

const FRONTMATTER_DELIMITER: &str = "---";
const CONTEXT_MARKER: &str = "# --- context (stripped before sending) ---";

/// Parse a compose file into frontmatter + body.
/// Strips the context block (everything after the context marker).
pub fn parse_compose_file(content: &str) -> Result<(ComposeFrontmatter, String), ComposeError> {
    let content = content.trim_start();

    if !content.starts_with(FRONTMATTER_DELIMITER) {
        return Err(ComposeError::MissingFrontmatter);
    }

    let after_first = &content[FRONTMATTER_DELIMITER.len()..];
    let end_pos = after_first
        .find(&format!("\n{FRONTMATTER_DELIMITER}"))
        .ok_or(ComposeError::MissingFrontmatter)?;

    let yaml_str = &after_first[..end_pos];
    let rest = &after_first[end_pos + 1 + FRONTMATTER_DELIMITER.len()..];

    let frontmatter: ComposeFrontmatter = serde_yaml::from_str(yaml_str)
        .map_err(|e| ComposeError::InvalidFrontmatter(e.to_string()))?;

    // Strip context block
    let body = if let Some(ctx_pos) = rest.find(CONTEXT_MARKER) {
        rest[..ctx_pos].to_string()
    } else {
        rest.to_string()
    };

    let body = body.trim().to_string();

    Ok((frontmatter, body))
}

/// Generate a compose file string from frontmatter + body + optional context.
pub fn render_compose_file(
    frontmatter: &ComposeFrontmatter,
    body: &str,
    context: Option<&str>,
) -> Result<String, ComposeError> {
    let yaml = serde_yaml::to_string(frontmatter)
        .map_err(|e| ComposeError::InvalidFrontmatter(e.to_string()))?;

    let mut output = format!("---\n{yaml}---\n\n{body}");

    if let Some(ctx) = context {
        output.push_str("\n\n");
        output.push_str(CONTEXT_MARKER);
        output.push('\n');
        for line in ctx.lines() {
            output.push_str(&format!("# {line}\n"));
        }
    }

    Ok(output)
}

#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    #[error("Missing YAML frontmatter delimiters (---)")]
    MissingFrontmatter,
    #[error("Invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    #[error("Attachment not found: {0}")]
    AttachmentNotFound(String),
    #[error("No recipients specified")]
    NoRecipients,
    #[error("Editor failed: {0}")]
    EditorFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_frontmatter() {
        let content =
            "---\nto: alice@example.com\nsubject: Hello\nfrom: me@example.com\n---\n\nBody here.";
        let (fm, body) = parse_compose_file(content).unwrap();
        assert_eq!(fm.to, "alice@example.com");
        assert_eq!(fm.subject, "Hello");
        assert_eq!(fm.from, "me@example.com");
        assert_eq!(body, "Body here.");
    }

    #[test]
    fn context_block_stripped() {
        let content = "---\nto: alice@example.com\nsubject: test\nfrom: me@example.com\n---\n\nHello!\n\n# --- context (stripped before sending) ---\n# Some context here\n# More context";
        let (_, body) = parse_compose_file(content).unwrap();
        assert_eq!(body, "Hello!");
        assert!(!body.contains("context"));
    }

    #[test]
    fn missing_frontmatter_errors() {
        let content = "No frontmatter here.";
        assert!(parse_compose_file(content).is_err());
    }

    #[test]
    fn roundtrip_frontmatter() {
        let fm = ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Test Subject".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            references: Vec::new(),
            attach: Vec::new(),
        };
        let rendered = render_compose_file(&fm, "Hello!", None).unwrap();
        let (parsed_fm, parsed_body) = parse_compose_file(&rendered).unwrap();
        assert_eq!(parsed_fm.to, "alice@example.com");
        assert_eq!(parsed_fm.subject, "Test Subject");
        assert_eq!(parsed_body, "Hello!");
    }

    #[test]
    fn roundtrip_with_context() {
        let fm = ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Re: Meeting".into(),
            from: "me@example.com".into(),
            in_reply_to: Some("<msg-123@example.com>".into()),
            references: vec!["<root@example.com>".into(), "<msg-123@example.com>".into()],
            attach: Vec::new(),
        };
        let context = "From: alice@example.com\nDate: 2026-03-15\n\nOriginal message.";
        let rendered = render_compose_file(&fm, "My reply.", Some(context)).unwrap();
        let (parsed_fm, parsed_body) = parse_compose_file(&rendered).unwrap();
        assert_eq!(parsed_fm.subject, "Re: Meeting");
        assert_eq!(
            parsed_fm.references,
            vec![
                "<root@example.com>".to_string(),
                "<msg-123@example.com>".to_string()
            ]
        );
        assert_eq!(parsed_body, "My reply.");
        assert!(!parsed_body.contains("Original message"));
    }
}
