#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::{AccountId, Address, Draft, DraftId, ReplyHeaders};
use mxr_protocol::*;
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
    pub yes: bool,
    pub dry_run: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn resolve_compose_from_address(explicit_from: Option<String>) -> String {
    if let Some(from) = explicit_from {
        return from;
    }

    let config = mxr_config::load_config().unwrap_or_default();
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
    let mut client = IpcClient::connect().await?;
    let account = resolve_compose_account(&mut client, options.from.as_deref()).await?;
    let stdin_or_body = read_body_input(options.body, options.body_stdin)?;

    let (frontmatter, body, draft_file) = if let Some(body) = stdin_or_body {
        (
            mxr_compose::frontmatter::ComposeFrontmatter {
                to: options.to.unwrap_or_default(),
                cc: options.cc.unwrap_or_default(),
                bcc: options.bcc.unwrap_or_default(),
                subject: options.subject.unwrap_or_default(),
                from: account.email.clone(),
                in_reply_to: None,
                references: Vec::new(),
                attach: attachment_strings(&options.attach),
            },
            body,
            None,
        )
    } else if options.dry_run {
        (
            mxr_compose::frontmatter::ComposeFrontmatter {
                to: options.to.unwrap_or_default(),
                cc: options.cc.unwrap_or_default(),
                bcc: options.bcc.unwrap_or_default(),
                subject: options.subject.unwrap_or_default(),
                from: account.email.clone(),
                in_reply_to: None,
                references: Vec::new(),
                attach: attachment_strings(&options.attach),
            },
            String::new(),
            None,
        )
    } else {
        let (path, cursor_line) = mxr_compose::create_draft_file(
            mxr_compose::ComposeKind::New {
                to: options.to.unwrap_or_default(),
                subject: options.subject.unwrap_or_default(),
            },
            &account.email,
        )?;
        rewrite_compose_frontmatter(
            &path,
            options.cc,
            options.bcc,
            attachment_strings(&options.attach),
        )?;
        let editor = mxr_compose::editor::resolve_editor(None);
        mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;
        let content = std::fs::read_to_string(&path)?;
        let (frontmatter, body) = mxr_compose::frontmatter::parse_compose_file(&content)?;
        (frontmatter, body, Some(path))
    };

    let draft = draft_from_frontmatter(account.account_id, &frontmatter, body)?;
    validate_compose_draft(&frontmatter, &draft.body_markdown, options.yes)?;

    if options.dry_run {
        print_draft_preview(&draft, options.yes);
        return Ok(());
    }

    if options.yes {
        expect_ack(
            client
                .request(Request::SendDraft {
                    draft: draft.clone(),
                })
                .await?,
        )?;
        if let Some(path) = draft_file {
            let _ = mxr_compose::delete_draft_file(&path);
        }
        println!("Sent draft {}", draft.id);
    } else {
        expect_ack(
            client
                .request(Request::SaveDraft {
                    draft: draft.clone(),
                })
                .await?,
        )?;
        if let Some(path) = draft_file {
            let _ = mxr_compose::delete_draft_file(&path);
        }
        println!("Draft saved: {}", draft.id);
        println!("Send with: mxr send {}", draft.id);
    }
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

    let (path, cursor_line) = mxr_compose::create_draft_file(
        mxr_compose::ComposeKind::Reply {
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

    let editor = mxr_compose::editor::resolve_editor(None);
    mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

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

    let (path, cursor_line) = mxr_compose::create_draft_file(
        mxr_compose::ComposeKind::Reply {
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

    let editor = mxr_compose::editor::resolve_editor(None);
    mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

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

    let (path, cursor_line) = mxr_compose::create_draft_file(
        mxr_compose::ComposeKind::Forward {
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

    let editor = mxr_compose::editor::resolve_editor(None);
    mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

pub async fn drafts(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListDrafts).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Drafts { drafts },
        } => match resolve_format(format) {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&drafts)?),
            OutputFormat::Jsonl => println!("{}", jsonl(&drafts)?),
            OutputFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());
                writer.write_record(["draft_id", "account_id", "subject", "updated_at"])?;
                for draft in &drafts {
                    writer.write_record(vec![
                        draft.id.as_str(),
                        draft.account_id.as_str(),
                        draft.subject.clone(),
                        draft.updated_at.to_rfc3339(),
                    ])?;
                }
                println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
            }
            OutputFormat::Ids => {
                for draft in &drafts {
                    println!("{}", draft.id);
                }
            }
            OutputFormat::Table => {
                if drafts.is_empty() {
                    println!("No drafts");
                } else {
                    for d in &drafts {
                        println!("  {} — {}", d.id, d.subject);
                    }
                }
            }
        },
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn send_draft(draft_id: String) -> anyhow::Result<()> {
    let draft_id = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::SendStoredDraft {
            draft_id: draft_id.clone(),
        })
        .await?;
    expect_ack(resp)?;
    println!("Sent draft {}", draft_id);
    Ok(())
}

async fn resolve_compose_account(
    client: &mut IpcClient,
    explicit_from: Option<&str>,
) -> anyhow::Result<AccountSummaryData> {
    let resp = client.request(Request::ListAccounts).await?;
    let accounts = crate::commands::expect_response(resp, |response| match response {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => Some(accounts),
        _ => None,
    })?;
    select_compose_account(&accounts, explicit_from)
}

fn select_compose_account(
    accounts: &[AccountSummaryData],
    explicit_from: Option<&str>,
) -> anyhow::Result<AccountSummaryData> {
    let send_capable = accounts
        .iter()
        .filter(|account| account.enabled && account.send_kind.is_some())
        .collect::<Vec<_>>();

    if let Some(value) = explicit_from
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let value_lower = value.to_ascii_lowercase();
        let matches = send_capable
            .iter()
            .filter(|account| {
                account.email.eq_ignore_ascii_case(value)
                    || account.name.eq_ignore_ascii_case(value)
                    || account
                        .key
                        .as_deref()
                        .is_some_and(|key| key.eq_ignore_ascii_case(value))
                    || account
                        .account_id
                        .to_string()
                        .eq_ignore_ascii_case(&value_lower)
            })
            .copied()
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [account] => Ok((*account).clone()),
            [] => anyhow::bail!("No send-capable account matches `{value}`"),
            _ => anyhow::bail!("Multiple send-capable accounts match `{value}`"),
        };
    }

    send_capable
        .iter()
        .find(|account| account.is_default)
        .copied()
        .or_else(|| send_capable.first().copied())
        .map(|account| (*account).clone())
        .ok_or_else(|| anyhow::anyhow!("No send-capable account configured"))
}

fn read_body_input(body: Option<String>, body_stdin: bool) -> anyhow::Result<Option<String>> {
    if let Some(body) = body {
        return Ok(Some(body));
    }
    if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        return Ok(Some(stdin_body));
    }
    Ok(None)
}

fn rewrite_compose_frontmatter(
    path: &std::path::Path,
    cc: Option<String>,
    bcc: Option<String>,
    attach: Vec<String>,
) -> anyhow::Result<()> {
    if cc.is_none() && bcc.is_none() && attach.is_empty() {
        return Ok(());
    }
    let content = std::fs::read_to_string(path)?;
    let (mut frontmatter, body) = mxr_compose::frontmatter::parse_compose_file(&content)?;
    if let Some(cc) = cc {
        frontmatter.cc = cc;
    }
    if let Some(bcc) = bcc {
        frontmatter.bcc = bcc;
    }
    if !attach.is_empty() {
        frontmatter.attach = attach;
    }
    let updated = mxr_compose::frontmatter::render_compose_file(&frontmatter, &body, None)?;
    std::fs::write(path, updated)?;
    Ok(())
}

fn attachment_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

fn draft_from_frontmatter(
    account_id: AccountId,
    frontmatter: &mxr_compose::frontmatter::ComposeFrontmatter,
    body: String,
) -> anyhow::Result<Draft> {
    let now = chrono::Utc::now();
    let reply_headers = frontmatter
        .in_reply_to
        .as_ref()
        .map(|in_reply_to| ReplyHeaders {
            in_reply_to: in_reply_to.clone(),
            references: frontmatter.references.clone(),
        });
    Ok(Draft {
        id: DraftId::new(),
        account_id,
        reply_headers,
        to: parse_addresses(&frontmatter.to),
        cc: parse_addresses(&frontmatter.cc),
        bcc: parse_addresses(&frontmatter.bcc),
        subject: frontmatter.subject.clone(),
        body_markdown: body,
        attachments: frontmatter.attach.iter().map(PathBuf::from).collect(),
        created_at: now,
        updated_at: now,
    })
}

fn parse_addresses(raw: &str) -> Vec<Address> {
    mxr_mail_parse::parse_address_list(raw)
}

fn validate_compose_draft(
    frontmatter: &mxr_compose::frontmatter::ComposeFrontmatter,
    body: &str,
    sending: bool,
) -> anyhow::Result<()> {
    let issues = if sending {
        mxr_compose::validate_draft(frontmatter, body)
    } else {
        mxr_compose::validate_draft_for_save(frontmatter, body)
    };
    for issue in &issues {
        eprintln!("{issue}");
    }
    if issues.iter().any(mxr_compose::ComposeValidation::is_error) {
        anyhow::bail!("Draft validation failed");
    }
    mxr_compose::attachments::resolve_attachment_paths(
        &frontmatter
            .attach
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

fn expect_ack(resp: Response) -> anyhow::Result<()> {
    crate::commands::expect_response(resp, |response| match response {
        Response::Ok {
            data: ResponseData::Ack,
        } => Some(()),
        _ => None,
    })
}

fn print_draft_preview(draft: &Draft, sending: bool) {
    let action = if sending { "send" } else { "save draft" };
    println!("Would {action}:");
    println!("  id: {}", draft.id);
    println!("  to: {}", format_addresses(&draft.to));
    println!("  cc: {}", format_addresses(&draft.cc));
    println!("  bcc: {}", format_addresses(&draft.bcc));
    println!("  subject: {}", draft.subject);
    println!("  attachments: {}", draft.attachments.len());
}

fn format_addresses(addresses: &[Address]) -> String {
    addresses
        .iter()
        .map(|address| match address.name.as_deref() {
            Some(name) if !name.is_empty() => format!("{name} <{}>", address.email),
            _ => address.email.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::mutations::helpers::{
        render_selection_preview_lines, requires_confirmation, MutationSelection,
    };
    use mxr_core::types::Envelope;
    use mxr_protocol::{AccountEditModeData, AccountSourceData, AccountSummaryData};

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

    fn account(
        key: &str,
        email: &str,
        is_default: bool,
        send_kind: Option<&str>,
    ) -> AccountSummaryData {
        AccountSummaryData {
            account_id: mxr_core::AccountId::from_provider_id("test", email),
            key: Some(key.into()),
            name: key.into(),
            email: email.into(),
            provider_kind: send_kind.unwrap_or("imap").into(),
            sync_kind: Some("imap".into()),
            send_kind: send_kind.map(str::to_string),
            enabled: true,
            is_default,
            source: AccountSourceData::Config,
            editable: AccountEditModeData::Full,
            sync: None,
            send: None,
        }
    }

    #[test]
    fn compose_account_selection_uses_default_send_account() {
        let accounts = vec![
            account("personal", "me@example.com", false, Some("smtp")),
            account("work", "work@example.com", true, Some("gmail")),
        ];

        let selected = select_compose_account(&accounts, None).unwrap();

        assert_eq!(selected.email, "work@example.com");
    }

    #[test]
    fn compose_account_selection_rejects_sync_only_accounts() {
        let accounts = vec![account("work", "work@example.com", true, None)];

        let error = select_compose_account(&accounts, None).unwrap_err();

        assert!(error.to_string().contains("No send-capable account"));
    }

    #[test]
    fn draft_from_frontmatter_parses_recipients_and_reply_headers() {
        let frontmatter = mxr_compose::frontmatter::ComposeFrontmatter {
            to: "\"Last, First\" <first@example.com>, second@example.com".into(),
            cc: String::new(),
            bcc: "hidden@example.com".into(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: Some("<reply@example.com>".into()),
            references: vec!["<root@example.com>".into()],
            attach: Vec::new(),
        };

        let draft = draft_from_frontmatter(mxr_core::AccountId::new(), &frontmatter, "body".into())
            .unwrap();

        assert_eq!(draft.to.len(), 2);
        assert_eq!(draft.to[0].name.as_deref(), Some("Last, First"));
        assert_eq!(draft.bcc[0].email, "hidden@example.com");
        assert_eq!(
            draft.reply_headers.unwrap().in_reply_to,
            "<reply@example.com>"
        );
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
