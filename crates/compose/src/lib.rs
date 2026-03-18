pub mod attachments;
pub mod editor;
pub mod frontmatter;
pub mod render;

use crate::frontmatter::{ComposeError, ComposeFrontmatter};
use std::path::PathBuf;
use uuid::Uuid;

/// The kind of compose action.
pub enum ComposeKind {
    New,
    Reply {
        in_reply_to: String,
        to: String,
        cc: String,
        subject: String,
        /// Reader-mode-cleaned thread content for context block.
        thread_context: String,
    },
    Forward {
        subject: String,
        /// Reader-mode-cleaned original message for context block.
        original_context: String,
    },
}

/// Create a draft file on disk and return its path + the cursor line.
pub fn create_draft_file(kind: ComposeKind, from: &str) -> Result<(PathBuf, usize), ComposeError> {
    let draft_id = Uuid::now_v7();
    let path = std::env::temp_dir().join(format!("mxr-draft-{draft_id}.md"));

    let (fm, body, context) = match kind {
        ComposeKind::New => {
            let fm = ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: String::new(),
                from: from.to_string(),
                in_reply_to: None,
                attach: Vec::new(),
            };
            (fm, String::new(), None)
        }
        ComposeKind::Reply {
            in_reply_to,
            to,
            cc,
            subject,
            thread_context,
        } => {
            let fm = ComposeFrontmatter {
                to,
                cc,
                bcc: String::new(),
                subject: format!("Re: {subject}"),
                from: from.to_string(),
                in_reply_to: Some(in_reply_to),
                attach: Vec::new(),
            };
            (fm, String::new(), Some(thread_context))
        }
        ComposeKind::Forward {
            subject,
            original_context,
        } => {
            let fm = ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: format!("Fwd: {subject}"),
                from: from.to_string(),
                in_reply_to: None,
                attach: Vec::new(),
            };
            let body = "---------- Forwarded message ----------".to_string();
            (fm, body, Some(original_context))
        }
    };

    let content = frontmatter::render_compose_file(&fm, &body, context.as_deref())?;

    // Calculate cursor line: first empty line after frontmatter closing ---
    let cursor_line = content
        .lines()
        .enumerate()
        .skip(1)
        .find_map(|(i, line)| {
            if line == "---" {
                Some(i + 2) // line after ---, 1-indexed, +1 for blank line
            } else {
                None
            }
        })
        .unwrap_or(1);

    std::fs::write(&path, &content)?;

    Ok((path, cursor_line))
}

/// Validate a parsed draft before sending.
pub fn validate_draft(frontmatter: &ComposeFrontmatter, body: &str) -> Vec<ComposeValidation> {
    let mut issues = Vec::new();

    if frontmatter.to.trim().is_empty() {
        issues.push(ComposeValidation::Error(
            "No recipients (to: field is empty)".into(),
        ));
    }

    if frontmatter.subject.trim().is_empty() {
        issues.push(ComposeValidation::Warning("Subject is empty".into()));
    }

    if body.trim().is_empty() {
        issues.push(ComposeValidation::Warning("Message body is empty".into()));
    }

    // Validate email addresses
    for addr in frontmatter
        .to
        .split(',')
        .chain(frontmatter.cc.split(','))
        .chain(frontmatter.bcc.split(','))
    {
        let addr = addr.trim();
        if !addr.is_empty() && !addr.contains('@') {
            issues.push(ComposeValidation::Error(format!(
                "Invalid email address: {addr}"
            )));
        }
    }

    issues
}

#[derive(Debug)]
pub enum ComposeValidation {
    Error(String),
    Warning(String),
}

impl ComposeValidation {
    pub fn is_error(&self) -> bool {
        matches!(self, ComposeValidation::Error(_))
    }
}

impl std::fmt::Display for ComposeValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComposeValidation::Error(msg) => write!(f, "Error: {msg}"),
            ComposeValidation::Warning(msg) => write!(f, "Warning: {msg}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frontmatter::parse_compose_file;

    #[test]
    fn roundtrip_new_message() {
        let (path, _cursor) = create_draft_file(ComposeKind::New, "me@example.com").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();
        assert_eq!(fm.from, "me@example.com");
        assert!(fm.to.is_empty());
        assert!(body.is_empty());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn roundtrip_reply() {
        let (path, _) = create_draft_file(
            ComposeKind::Reply {
                in_reply_to: "<msg-123@example.com>".into(),
                to: "alice@example.com".into(),
                cc: "bob@example.com".into(),
                subject: "Deployment plan".into(),
                thread_context: "From: alice\nDate: 2026-03-15\n\nHey team?".into(),
            },
            "me@example.com",
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();
        assert_eq!(fm.subject, "Re: Deployment plan");
        assert_eq!(fm.to, "alice@example.com");
        assert!(fm.in_reply_to.is_some());
        assert!(!body.contains("Hey team?"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn roundtrip_forward() {
        let (path, _) = create_draft_file(
            ComposeKind::Forward {
                subject: "Important doc".into(),
                original_context: "The original message content.".into(),
            },
            "me@example.com",
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();
        assert_eq!(fm.subject, "Fwd: Important doc");
        assert!(body.contains("Forwarded message"));
        assert!(!body.contains("original message content"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn validates_missing_recipient() {
        let fm = ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Test".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            attach: Vec::new(),
        };
        let issues = validate_draft(&fm, "body");
        assert!(issues.iter().any(|i| i.is_error()));
    }

    #[test]
    fn validates_invalid_email() {
        let fm = ComposeFrontmatter {
            to: "not-an-email".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Test".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            attach: Vec::new(),
        };
        let issues = validate_draft(&fm, "body");
        assert!(issues.iter().any(|i| i.is_error()));
    }

    #[test]
    fn validates_empty_subject_warning() {
        let fm = ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            from: "me@example.com".into(),
            in_reply_to: None,
            attach: Vec::new(),
        };
        let issues = validate_draft(&fm, "body");
        assert!(!issues.iter().any(|i| i.is_error()));
        assert!(issues.iter().any(|i| !i.is_error()));
    }

    #[test]
    fn valid_draft_no_errors() {
        let fm = ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            attach: Vec::new(),
        };
        let issues = validate_draft(&fm, "Hello there!");
        assert!(!issues.iter().any(|i| i.is_error()));
    }
}
