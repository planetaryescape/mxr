#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

pub mod attachments;
pub mod editor;
pub mod email;
pub mod frontmatter;
pub mod parse;
pub mod private_tmp;
pub mod render;

use crate::frontmatter::{ComposeError, ComposeFrontmatter};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The kind of compose action.
pub enum ComposeKind {
    New {
        to: String,
        subject: String,
    },
    Reply {
        reply_all: bool,
        in_reply_to: String,
        references: Vec<String>,
        /// Provider-native thread hint (e.g. Gmail thread id).
        thread_id: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeSignature {
    pub name: String,
    pub body: String,
}

/// Create a draft file on disk and return its path + the cursor line.
pub fn create_draft_file(kind: ComposeKind, from: &str) -> Result<(PathBuf, usize), ComposeError> {
    create_draft_file_with_signature(kind, from, None)
}

pub fn create_draft_file_with_signature(
    kind: ComposeKind,
    from: &str,
    signature: Option<&ComposeSignature>,
) -> Result<(PathBuf, usize), ComposeError> {
    let (path, cursor_line, content) = build_draft_file(kind, from, signature)?;
    private_tmp::write_private(&path, content.as_bytes())?;
    Ok((path, cursor_line))
}

/// Build the seed frontmatter for a compose kind without touching the filesystem.
/// Use this when the caller wants to skip $EDITOR (inline body, dry-run, etc.).
pub fn seed_frontmatter(kind: ComposeKind, from: &str) -> Result<ComposeFrontmatter, ComposeError> {
    seed_frontmatter_with_signature(kind, from, None)
}

pub fn seed_frontmatter_with_signature(
    kind: ComposeKind,
    from: &str,
    signature: Option<&ComposeSignature>,
) -> Result<ComposeFrontmatter, ComposeError> {
    let (_path, _cursor, content) = build_draft_file(kind, from, signature)?;
    let (frontmatter, _body) = frontmatter::parse_compose_file(&content)?;
    Ok(frontmatter)
}

pub async fn create_draft_file_async(
    kind: ComposeKind,
    from: &str,
) -> Result<(PathBuf, usize), ComposeError> {
    create_draft_file_async_with_signature(kind, from, None).await
}

pub async fn create_draft_file_async_with_signature(
    kind: ComposeKind,
    from: &str,
    signature: Option<&ComposeSignature>,
) -> Result<(PathBuf, usize), ComposeError> {
    let (path, cursor_line, content) = build_draft_file(kind, from, signature)?;
    private_tmp::write_private_async(&path, content.as_bytes()).await?;
    Ok((path, cursor_line))
}

pub fn read_draft_file(path: &Path) -> Result<String, ComposeError> {
    Ok(std::fs::read_to_string(path)?)
}

pub async fn read_draft_file_async(path: &Path) -> Result<String, ComposeError> {
    Ok(tokio::fs::read_to_string(path).await?)
}

pub fn write_draft_file(path: &Path, content: &str) -> Result<(), ComposeError> {
    Ok(std::fs::write(path, content)?)
}

pub async fn write_draft_file_async(path: &Path, content: &str) -> Result<(), ComposeError> {
    Ok(tokio::fs::write(path, content).await?)
}

pub fn delete_draft_file(path: &Path) -> Result<(), ComposeError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub async fn delete_draft_file_async(path: &Path) -> Result<(), ComposeError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn build_draft_file(
    kind: ComposeKind,
    from: &str,
    signature: Option<&ComposeSignature>,
) -> Result<(PathBuf, usize, String), ComposeError> {
    let draft_id = Uuid::now_v7();
    let path = private_tmp::private_scratch_dir()?.join(format!("mxr-draft-{draft_id}.md"));

    let (mut fm, mut body, context) = match kind {
        ComposeKind::New { to, subject } => {
            let fm = ComposeFrontmatter {
                to,
                subject,
                from: from.to_string(),
                ..Default::default()
            };
            (fm, String::new(), None)
        }
        ComposeKind::Reply {
            reply_all,
            in_reply_to,
            references,
            thread_id,
            to,
            cc,
            subject,
            thread_context,
        } => {
            let fm = ComposeFrontmatter {
                to,
                cc,
                subject: format!("Re: {subject}"),
                from: from.to_string(),
                in_reply_to: Some(in_reply_to),
                intent: if reply_all {
                    mxr_core::DraftIntent::ReplyAll
                } else {
                    mxr_core::DraftIntent::Reply
                },
                references,
                thread_id,
                ..Default::default()
            };
            (fm, String::new(), Some(thread_context))
        }
        ComposeKind::Forward {
            subject,
            original_context,
        } => {
            let fm = ComposeFrontmatter {
                subject: format!("Fwd: {subject}"),
                from: from.to_string(),
                intent: mxr_core::DraftIntent::Forward,
                ..Default::default()
            };
            let body = "---------- Forwarded message ----------".to_string();
            (fm, body, Some(original_context))
        }
    };

    if let Some(signature) = signature {
        fm.signature = Some(signature.name.clone());
        body = insert_signature_block(&body, &signature.body);
    }

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

    Ok((path, cursor_line, content))
}

pub fn append_signature_to_body(body: &str, signature_body: &str) -> String {
    let signature = signature_block(signature_body);
    if signature.is_empty() {
        return body.trim().to_string();
    }
    let body = body.trim();
    if body.is_empty() {
        signature
    } else {
        format!("{body}\n\n{signature}")
    }
}

fn insert_signature_block(body: &str, signature_body: &str) -> String {
    let signature = signature_block(signature_body);
    if signature.is_empty() {
        return body.to_string();
    }
    let body = body.trim();
    if body.is_empty() {
        signature
    } else if body.starts_with("---------- Forwarded message ----------") {
        format!("{signature}\n\n{body}")
    } else {
        format!("{body}\n\n{signature}")
    }
}

fn signature_block(signature_body: &str) -> String {
    let trimmed = signature_body.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed == "-- " || trimmed.starts_with("-- \n") {
        trimmed.to_string()
    } else {
        format!("-- \n{trimmed}")
    }
}

/// Validate a parsed draft before sending.
pub fn validate_draft(frontmatter: &ComposeFrontmatter, body: &str) -> Vec<ComposeValidation> {
    validate_draft_with_mode(frontmatter, body, ComposeValidationMode::Send)
}

pub fn validate_draft_for_save(
    frontmatter: &ComposeFrontmatter,
    body: &str,
) -> Vec<ComposeValidation> {
    validate_draft_with_mode(frontmatter, body, ComposeValidationMode::SaveDraft)
}

fn validate_draft_with_mode(
    frontmatter: &ComposeFrontmatter,
    body: &str,
    mode: ComposeValidationMode,
) -> Vec<ComposeValidation> {
    let mut issues = Vec::new();

    if matches!(mode, ComposeValidationMode::Send) && frontmatter.to.trim().is_empty() {
        issues.push(ComposeValidation::MissingRecipients);
    }

    if frontmatter.subject.trim().is_empty() {
        issues.push(ComposeValidation::Warning("Subject is empty".into()));
    }

    if body.trim().is_empty() {
        issues.push(ComposeValidation::Warning("Message body is empty".into()));
    }
    if matches!(mode, ComposeValidationMode::Send) {
        let vars = unresolved_template_vars(body);
        if !vars.is_empty() {
            issues.push(ComposeValidation::Warning(format!(
                "Unresolved snippet variable(s): {}",
                vars.join(", ")
            )));
        }
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

fn unresolved_template_vars(body: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else {
            break;
        };
        let name = rest[..end].trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            && !vars.iter().any(|existing| existing == name)
        {
            vars.push(name.to_string());
        }
        rest = &rest[end + 1..];
    }
    vars
}

#[derive(Debug)]
pub enum ComposeValidation {
    MissingRecipients,
    Error(String),
    Warning(String),
}

impl ComposeValidation {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::MissingRecipients | Self::Error(_))
    }

    pub fn is_missing_recipients(&self) -> bool {
        matches!(self, Self::MissingRecipients)
    }
}

impl std::fmt::Display for ComposeValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRecipients => {
                write!(f, "Error: No recipients (to: field is empty)")
            }
            Self::Error(msg) => write!(f, "Error: {msg}"),
            Self::Warning(msg) => write!(f, "Warning: {msg}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeValidationMode {
    Send,
    SaveDraft,
}

#[cfg(test)]
mod tests {
    use super::*;
    use frontmatter::parse_compose_file;

    fn issue_messages(issues: &[ComposeValidation]) -> Vec<String> {
        issues.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn roundtrip_new_message() {
        let (path, _cursor) = create_draft_file(
            ComposeKind::New {
                to: String::new(),
                subject: String::new(),
            },
            "me@example.com",
        )
        .unwrap();
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
                reply_all: false,
                in_reply_to: "<msg-123@example.com>".into(),
                references: vec!["<root@example.com>".into(), "<msg-123@example.com>".into()],
                thread_id: None,
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
        assert_eq!(fm.intent, mxr_core::DraftIntent::Reply);
        assert_eq!(fm.references.len(), 2);
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
        assert_eq!(fm.intent, mxr_core::DraftIntent::Forward);
        assert!(body.contains("Forwarded message"));
        assert!(!body.contains("original message content"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn creates_new_message_with_signature_block() {
        let signature = ComposeSignature {
            name: "Work".into(),
            body: "Alice".into(),
        };
        let (path, cursor) = create_draft_file_with_signature(
            ComposeKind::New {
                to: String::new(),
                subject: String::new(),
            },
            "me@example.com",
            Some(&signature),
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();

        assert_eq!(fm.signature.as_deref(), Some("Work"));
        assert_eq!(body, "-- \nAlice");
        assert!(cursor > 1);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn append_signature_preserves_existing_delimiter() {
        let body = append_signature_to_body("Hello", "-- \nAlice");

        assert_eq!(body, "Hello\n\n-- \nAlice");
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
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft(&fm, "body");
        assert_eq!(
            issue_messages(&issues),
            vec!["Error: No recipients (to: field is empty)"]
        );
    }

    #[test]
    fn save_draft_allows_missing_recipient() {
        let fm = ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Test".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft_for_save(&fm, "body");
        assert!(issues.is_empty());
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
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft(&fm, "body");
        assert_eq!(
            issue_messages(&issues),
            vec!["Error: Invalid email address: not-an-email"]
        );
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
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft(&fm, "body");
        assert_eq!(issue_messages(&issues), vec!["Warning: Subject is empty"]);
    }

    #[test]
    fn roundtrip_new_with_to() {
        let (path, _cursor) = create_draft_file(
            ComposeKind::New {
                to: "alice@example.com".into(),
                subject: String::new(),
            },
            "me@example.com",
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();
        assert_eq!(fm.from, "me@example.com");
        assert_eq!(fm.to, "alice@example.com");
        assert!(fm.subject.is_empty());
        assert!(body.is_empty());
        std::fs::remove_file(path).ok();
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
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft(&fm, "Hello there!");
        assert!(issues.is_empty());
    }

    #[test]
    fn save_draft_keeps_empty_subject_as_warning_only() {
        let fm = ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft_for_save(&fm, "Hello there!");
        assert_eq!(issue_messages(&issues), vec!["Warning: Subject is empty"]);
    }

    #[test]
    fn save_draft_still_rejects_invalid_email() {
        let fm = ComposeFrontmatter {
            to: "not-an-email".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft_for_save(&fm, "Hello there!");
        assert_eq!(
            issue_messages(&issues),
            vec![
                "Warning: Subject is empty",
                "Error: Invalid email address: not-an-email",
            ]
        );
    }

    #[test]
    fn send_validation_warns_about_unresolved_snippet_vars() {
        let fm = ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };
        let issues = validate_draft(&fm, "Hi {first_name}, see {date} and {first_name}.");

        assert_eq!(
            issue_messages(&issues),
            vec!["Warning: Unresolved snippet variable(s): first_name, date"]
        );
    }

    #[tokio::test]
    async fn async_draft_helpers_round_trip_content_and_cleanup() {
        let (path, _cursor) = create_draft_file_async(
            ComposeKind::New {
                to: "alice@example.com".into(),
                subject: "Hello".into(),
            },
            "me@example.com",
        )
        .await
        .unwrap();

        let original = read_draft_file_async(&path).await.unwrap();
        assert!(original.contains("to: alice@example.com"));
        assert!(original.contains("subject: Hello"));

        let updated = original.replace("Hello", "Updated subject");
        write_draft_file_async(&path, &updated).await.unwrap();
        let reread = read_draft_file_async(&path).await.unwrap();
        assert!(reread.contains("subject: Updated subject"));

        delete_draft_file_async(&path).await.unwrap();
        assert!(!path.exists());
    }

    /// Plan 005: draft files land in the private scratch dir with 0600 perms.
    #[cfg(unix)]
    #[test]
    fn create_draft_file_with_signature_uses_private_dir_and_0600_perms() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = private_tmp::private_scratch_dir().expect("private scratch dir");
        let (path, _cursor) = create_draft_file_with_signature(
            ComposeKind::New {
                to: "alice@example.com".into(),
                subject: "Test".into(),
            },
            "me@example.com",
            None,
        )
        .unwrap();

        // File should be inside the private scratch dir.
        assert!(
            path.starts_with(&scratch),
            "draft path {path:?} should be inside private scratch dir {scratch:?}"
        );

        // File should have 0600 permissions.
        let meta = std::fs::metadata(&path).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "draft file mode should be 0600, got {mode:04o}");

        std::fs::remove_file(path).ok();
    }
}
