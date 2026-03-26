use crate::ipc_client::IpcClient;
use crate::mxr_protocol::*;
use std::path::PathBuf;

use super::helpers::parse_message_id;

pub struct ComposeOptions {
    pub to: Option<String>,
    pub cc: Option<String>,
    pub bcc: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub body_stdin: bool,
    pub attach: Vec<PathBuf>,
    pub from: Option<String>,
    pub dry_run: bool,
}

pub(super) fn resolve_compose_from_address(explicit_from: Option<String>) -> String {
    if let Some(from) = explicit_from {
        return from;
    }

    let config = crate::mxr_config::load_config().unwrap_or_default();
    if let Some(default_key) = config.general.default_account.as_deref() {
        if let Some(account) = config.accounts.get(default_key) {
            return account.email.clone();
        }
    }

    config.accounts.values().next().map_or_else(
        || "you@example.com".to_string(),
        |account| account.email.clone(),
    )
}

pub async fn compose(options: ComposeOptions) -> anyhow::Result<()> {
    let from_addr = resolve_compose_from_address(options.from);

    if options.dry_run {
        println!("Would open $EDITOR to compose new email from {from_addr}");
        return Ok(());
    }

    let (path, cursor_line) =
        crate::mxr_compose::create_draft_file(crate::mxr_compose::ComposeKind::New, &from_addr)?;

    // If inline body provided, append it to the draft file
    if let Some(b) = &options.body {
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{b}"))?;
    } else if options.body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{stdin_body}"))?;
    }

    // Pre-fill frontmatter fields if provided via CLI args
    if options.to.is_some()
        || options.cc.is_some()
        || options.bcc.is_some()
        || options.subject.is_some()
        || !options.attach.is_empty()
    {
        let content = std::fs::read_to_string(&path)?;
        let mut updated = content;
        if let Some(to_val) = &options.to {
            updated = updated.replacen("to: \"\"", &format!("to: \"{to_val}\""), 1);
        }
        if let Some(cc_val) = &options.cc {
            updated = updated.replacen("cc: \"\"", &format!("cc: \"{cc_val}\""), 1);
        }
        if let Some(bcc_val) = &options.bcc {
            updated = updated.replacen("bcc: \"\"", &format!("bcc: \"{bcc_val}\""), 1);
        }
        if let Some(subj) = &options.subject {
            updated = updated.replacen("subject: \"\"", &format!("subject: \"{subj}\""), 1);
        }
        std::fs::write(&path, updated)?;
    }

    let editor = crate::mxr_compose::editor::resolve_editor(None);
    crate::mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

pub async fn reply(
    message_id: String,
    body: Option<String>,
    body_stdin: bool,
    _yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;

    let resp = client
        .request(Request::PrepareReply {
            message_id: id,
            reply_all: false,
        })
        .await?;

    let ctx = match resp {
        Response::Ok {
            data: ResponseData::ReplyContext { context },
        } => context,
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };

    if dry_run {
        println!("Would open $EDITOR to reply to {message_id}");
        return Ok(());
    }

    let (path, cursor_line) = crate::mxr_compose::create_draft_file(
        crate::mxr_compose::ComposeKind::Reply {
            in_reply_to: ctx.in_reply_to,
            references: ctx.references,
            to: ctx.reply_to,
            cc: String::new(),
            subject: ctx.subject,
            thread_context: ctx.thread_context,
        },
        &ctx.from,
    )?;

    if let Some(b) = &body {
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{b}"))?;
    } else if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{stdin_body}"))?;
    }

    let editor = crate::mxr_compose::editor::resolve_editor(None);
    crate::mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

pub async fn reply_all(
    message_id: String,
    body: Option<String>,
    body_stdin: bool,
    _yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;

    let resp = client
        .request(Request::PrepareReply {
            message_id: id,
            reply_all: true,
        })
        .await?;

    let ctx = match resp {
        Response::Ok {
            data: ResponseData::ReplyContext { context },
        } => context,
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };

    if dry_run {
        println!("Would open $EDITOR to reply-all to {message_id}");
        return Ok(());
    }

    let (path, cursor_line) = crate::mxr_compose::create_draft_file(
        crate::mxr_compose::ComposeKind::Reply {
            in_reply_to: ctx.in_reply_to,
            references: ctx.references,
            to: ctx.reply_to,
            cc: ctx.cc,
            subject: ctx.subject,
            thread_context: ctx.thread_context,
        },
        &ctx.from,
    )?;

    if let Some(b) = &body {
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{b}"))?;
    } else if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{stdin_body}"))?;
    }

    let editor = crate::mxr_compose::editor::resolve_editor(None);
    crate::mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

pub async fn forward(
    message_id: String,
    to: Option<String>,
    body: Option<String>,
    body_stdin: bool,
    _yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;

    let resp = client
        .request(Request::PrepareForward { message_id: id })
        .await?;

    let ctx = match resp {
        Response::Ok {
            data: ResponseData::ForwardContext { context },
        } => context,
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };

    if dry_run {
        println!("Would open $EDITOR to forward {message_id}");
        return Ok(());
    }

    let (path, cursor_line) = crate::mxr_compose::create_draft_file(
        crate::mxr_compose::ComposeKind::Forward {
            subject: ctx.subject,
            original_context: ctx.forwarded_content,
        },
        &ctx.from,
    )?;

    // Pre-fill "to" if provided
    if let Some(to_val) = &to {
        let content = std::fs::read_to_string(&path)?;
        let updated = content.replacen("to: \"\"", &format!("to: \"{to_val}\""), 1);
        std::fs::write(&path, updated)?;
    }

    if let Some(b) = &body {
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{b}"))?;
    } else if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{content}{stdin_body}"))?;
    }

    let editor = crate::mxr_compose::editor::resolve_editor(None);
    crate::mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

pub async fn drafts() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListDrafts).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Drafts { drafts },
        } => {
            if drafts.is_empty() {
                println!("No drafts");
            } else {
                for d in &drafts {
                    println!("  {} — {}", d.id, d.subject);
                }
            }
        }
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn send_draft(_draft_id: String) -> anyhow::Result<()> {
    println!("SendDraft via CLI is handled by compose flow (compose -> edit -> auto-send)");
    println!("Use `mxr compose` to create and send an email in one step.");
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "Command tests live near the helper they cover; moving them is out of scope here"
)]
mod tests {
    use super::resolve_compose_from_address;
    use crate::commands::mutations::helpers::{
        render_selection_preview_lines, requires_confirmation, MutationSelection,
    };
    use crate::mxr_core::types::Envelope;

    #[test]
    fn compose_from_prefers_explicit_value() {
        let resolved = resolve_compose_from_address(Some("alice@example.com".into()));
        assert_eq!(resolved, "alice@example.com");
    }

    #[test]
    fn compose_from_falls_back_when_no_config() {
        let temp_home =
            std::env::temp_dir().join(format!("mxr-compose-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_home);
        std::fs::create_dir_all(&temp_home).unwrap();
        let resolved = temp_env::with_var("HOME", Some(temp_home.as_os_str()), || {
            resolve_compose_from_address(None)
        });
        let _ = std::fs::remove_dir_all(&temp_home);

        assert_eq!(resolved, "you@example.com");
    }

    fn test_envelope(subject: &str) -> Envelope {
        crate::test_fixtures::TestEnvelopeBuilder::new()
            .subject(subject)
            .provider_id(format!("provider-{subject}"))
            .from_address("Buildkite", "buildkite@example.com")
            .to(vec![])
            .message_id_header(None)
            .snippet("")
            .size_bytes(0)
            .build()
    }

    #[test]
    fn confirmation_required_for_destructive_or_batch_actions() {
        assert!(requires_confirmation(true, false, 1, false));
        assert!(requires_confirmation(false, true, 1, false));
        assert!(requires_confirmation(false, false, 2, false));
        assert!(!requires_confirmation(false, false, 1, false));
        assert!(!requires_confirmation(true, true, 5, true));
    }

    #[test]
    fn preview_render_caps_output() {
        let envelopes = (0..10)
            .map(|i| test_envelope(&format!("Subject {i}")))
            .collect::<Vec<_>>();
        let selection = MutationSelection {
            ids: envelopes.iter().map(|env| env.id.clone()).collect(),
            envelopes,
            used_search: true,
        };

        let lines = render_selection_preview_lines("archive", &selection);
        assert!(lines[0].contains("Would archive 10 message(s)"));
        assert!(lines.iter().any(|line| line.contains("Subject 0")));
        assert!(lines.iter().any(|line| line.contains("... and 2 more")));
    }
}
