#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic and unwrap for direct fixture failures"
    )
)]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::{AccountId, Address, Draft, DraftId, ReplyHeaders};
use mxr_protocol::*;
use std::collections::HashMap;
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
    pub signature: Option<String>,
    pub no_signature: bool,
    pub yes: bool,
    pub dry_run: bool,
    pub format: Option<OutputFormat>,
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
    let signature = resolve_compose_signature(
        &mut client,
        &account.account_id,
        &account.email,
        SignatureContextData::New,
        options.signature.as_deref(),
        options.no_signature,
    )
    .await?;

    let make_inline_frontmatter = || mxr_compose::frontmatter::ComposeFrontmatter {
        to: options.to.clone().unwrap_or_default(),
        cc: options.cc.clone().unwrap_or_default(),
        bcc: options.bcc.clone().unwrap_or_default(),
        subject: options.subject.clone().unwrap_or_default(),
        from: account.email.clone(),
        attach: attachment_strings(&options.attach),
        signature: signature.as_ref().map(|signature| signature.name.clone()),
        ..Default::default()
    };

    let (frontmatter, body, draft_file) = if let Some(body) = stdin_or_body {
        (
            make_inline_frontmatter(),
            apply_signature_to_body(body, signature.as_ref()),
            None,
        )
    } else if options.dry_run {
        (
            make_inline_frontmatter(),
            apply_signature_to_body(String::new(), signature.as_ref()),
            None,
        )
    } else {
        let (path, cursor_line) = mxr_compose::create_draft_file_with_signature(
            mxr_compose::ComposeKind::New {
                to: options.to.unwrap_or_default(),
                subject: options.subject.unwrap_or_default(),
            },
            &account.email,
            signature.as_ref(),
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
    let body = expand_compose_snippets(&mut client, body).await?;

    let draft = draft_from_frontmatter(
        account.account_id,
        mxr_core::DraftIntent::New,
        &frontmatter,
        body,
    )?;
    validate_compose_draft(&frontmatter, &draft.body_markdown, options.yes)?;

    if options.dry_run {
        print_draft_preview(&draft, options.yes, options.format)?;
        return Ok(());
    }

    if options.yes {
        let receipt = expect_send_receipt(
            client
                .request(Request::SendDraft {
                    draft: draft.clone(),
                    override_safety_token: None,
                })
                .await?,
        )?;
        if let Some(path) = draft_file {
            let _ = mxr_compose::delete_draft_file(&path);
        }
        println!("Sent draft {}", draft.id);
        if let Some(info) = receipt.as_ref() {
            println!("Local message id: {}", info.local_message_id);
        }
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

#[expect(
    clippy::too_many_arguments,
    reason = "CLI wrapper mirrors clap-dispatched reply flags without changing the command surface"
)]
pub async fn reply(
    message_id: String,
    body: Option<String>,
    body_stdin: bool,
    signature: Option<String>,
    no_signature: bool,
    yes: bool,
    dry_run: bool,
    remind_after: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    reply_inner(
        message_id,
        body,
        body_stdin,
        signature,
        no_signature,
        yes,
        dry_run,
        remind_after,
        false,
        format,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI wrapper mirrors clap-dispatched reply-all flags without changing the command surface"
)]
pub async fn reply_all(
    message_id: String,
    body: Option<String>,
    body_stdin: bool,
    signature: Option<String>,
    no_signature: bool,
    yes: bool,
    dry_run: bool,
    remind_after: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    reply_inner(
        message_id,
        body,
        body_stdin,
        signature,
        no_signature,
        yes,
        dry_run,
        remind_after,
        true,
        format,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "reply_inner is the shared CLI reply/reply-all bridge; flags stay explicit at the boundary"
)]
async fn reply_inner(
    message_id: String,
    body: Option<String>,
    body_stdin: bool,
    signature: Option<String>,
    no_signature: bool,
    yes: bool,
    dry_run: bool,
    remind_after: Option<String>,
    reply_all: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;

    let resp = client
        .request(Request::PrepareReply {
            message_id: id,
            reply_all,
        })
        .await?;
    let ctx = match resp {
        Response::Ok {
            data: ResponseData::ReplyContext { context },
        } => context,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };

    let kind = mxr_compose::ComposeKind::Reply {
        reply_all,
        in_reply_to: ctx.in_reply_to.clone(),
        references: ctx.references.clone(),
        thread_id: ctx.thread_id.clone(),
        to: ctx.reply_to.clone(),
        cc: if reply_all {
            ctx.cc.clone()
        } else {
            String::new()
        },
        subject: ctx.subject.clone(),
        thread_context: ctx.thread_context.clone(),
    };

    let stdin_or_body = read_body_input(body, body_stdin)?;
    let signature = resolve_compose_signature(
        &mut client,
        &ctx.account_id,
        &ctx.from,
        SignatureContextData::Reply,
        signature.as_deref(),
        no_signature,
    )
    .await?;
    let (frontmatter, body_text, draft_file) = build_compose_draft(
        &mut client,
        kind,
        &ctx.from,
        stdin_or_body,
        dry_run,
        signature.as_ref(),
    )
    .await?;

    finalize_compose(
        &mut client,
        ctx.account_id,
        if reply_all {
            mxr_core::DraftIntent::ReplyAll
        } else {
            mxr_core::DraftIntent::Reply
        },
        frontmatter,
        body_text,
        draft_file,
        yes,
        dry_run,
        remind_after,
        format,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI wrapper mirrors clap-dispatched forward flags without changing the command surface"
)]
pub async fn forward(
    message_id: String,
    to: Option<String>,
    body: Option<String>,
    body_stdin: bool,
    signature: Option<String>,
    no_signature: bool,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
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
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };

    let kind = mxr_compose::ComposeKind::Forward {
        subject: ctx.subject.clone(),
        original_context: ctx.forwarded_content.clone(),
    };

    let stdin_or_body = read_body_input(body, body_stdin)?;
    let signature = resolve_compose_signature(
        &mut client,
        &ctx.account_id,
        &ctx.from,
        SignatureContextData::Reply,
        signature.as_deref(),
        no_signature,
    )
    .await?;
    let (mut frontmatter, body_text, draft_file) = build_compose_draft(
        &mut client,
        kind,
        &ctx.from,
        stdin_or_body,
        dry_run,
        signature.as_ref(),
    )
    .await?;

    if let Some(to_val) = to {
        if !to_val.trim().is_empty() {
            frontmatter.to = to_val;
        }
    }

    finalize_compose(
        &mut client,
        ctx.account_id,
        mxr_core::DraftIntent::Forward,
        frontmatter,
        body_text,
        draft_file,
        yes,
        dry_run,
        None,
        format,
    )
    .await
}

async fn build_compose_draft(
    _client: &mut IpcClient,
    kind: mxr_compose::ComposeKind,
    from_email: &str,
    stdin_or_body: Option<String>,
    dry_run: bool,
    signature: Option<&mxr_compose::ComposeSignature>,
) -> anyhow::Result<(
    mxr_compose::frontmatter::ComposeFrontmatter,
    String,
    Option<PathBuf>,
)> {
    if let Some(body) = stdin_or_body {
        let frontmatter =
            mxr_compose::seed_frontmatter_with_signature(kind, from_email, signature)?;
        return Ok((frontmatter, apply_signature_to_body(body, signature), None));
    }

    if dry_run {
        let frontmatter =
            mxr_compose::seed_frontmatter_with_signature(kind, from_email, signature)?;
        return Ok((
            frontmatter,
            apply_signature_to_body(String::new(), signature),
            None,
        ));
    }

    let (path, cursor_line) =
        mxr_compose::create_draft_file_with_signature(kind, from_email, signature)?;
    let editor = mxr_compose::editor::resolve_editor(None);
    mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;
    let content = std::fs::read_to_string(&path)?;
    let (frontmatter, body) = mxr_compose::frontmatter::parse_compose_file(&content)?;
    Ok((frontmatter, body, Some(path)))
}

#[expect(
    clippy::too_many_arguments,
    reason = "compose finalization keeps draft state and CLI mode flags explicit at the send/save boundary"
)]
async fn finalize_compose(
    client: &mut IpcClient,
    account_id: AccountId,
    intent: mxr_core::DraftIntent,
    frontmatter: mxr_compose::frontmatter::ComposeFrontmatter,
    body: String,
    draft_file: Option<PathBuf>,
    yes: bool,
    dry_run: bool,
    remind_after: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    // Recipient + thread context is only meaningful at the
    // post-$EDITOR stage where the user has filled in `to:` / `subject:`.
    let snippet_ctx = snippet_context_from_frontmatter(&frontmatter);
    let body = expand_compose_snippets_with_context(client, body, Some(&snippet_ctx)).await?;
    let draft = draft_from_frontmatter(account_id, intent, &frontmatter, body)?;
    validate_compose_draft(&frontmatter, &draft.body_markdown, yes)?;

    if dry_run {
        print_draft_preview(&draft, yes, format)?;
        return Ok(());
    }

    if !yes && remind_after.is_some() {
        anyhow::bail!("--remind-after requires --yes; saved drafts can be sent later with `mxr send <draft-id> --remind-after <time>`");
    }

    if yes {
        let receipt = expect_send_receipt(
            client
                .request(Request::SendDraft {
                    draft: draft.clone(),
                    override_safety_token: None,
                })
                .await?,
        )?;
        set_auto_reminder_after_send(client, receipt.as_ref(), remind_after.as_deref()).await?;
        if let Some(path) = draft_file {
            let _ = mxr_compose::delete_draft_file(&path);
        }
        println!("Sent draft {}", draft.id);
        if let Some(info) = receipt.as_ref() {
            println!("Local message id: {}", info.local_message_id);
        }
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
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

/// CLI surface for `mxr drafts recover`. Surfaces drafts the daemon
/// believes are orphaned mid-send (status `'sending'` with stale
/// activity) so the user can decide between resume and discard.
pub async fn drafts_recover(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListOrphanedDrafts).await?;
    let drafts = match resp {
        Response::Ok {
            data: ResponseData::Drafts { drafts },
        } => drafts,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };
    match resolve_format(format) {
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
                println!("No orphaned drafts");
            } else {
                println!("{} orphaned draft(s):", drafts.len());
                for d in &drafts {
                    println!("  {} — {}", d.id, d.subject);
                }
                println!();
                println!("Resume any with: mxr drafts resume <draft-id>");
                println!("Discard any with: mxr drafts discard <draft-id>");
            }
        }
    }
    Ok(())
}

/// CLI surface for `mxr drafts resume <id>`. Force-resets the draft to
/// `'draft'` status so the user can retry the send via the normal
/// pipeline. Idempotent — already-`'draft'` drafts are a no-op.
pub async fn drafts_resume(draft_id: String) -> anyhow::Result<()> {
    let parsed = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ResetOrphanedDraft {
            draft_id: parsed.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            println!(
                "Draft {} reset to 'draft' — retry with `mxr send {}`",
                parsed, parsed
            );
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

/// CLI surface for `mxr drafts discard <id>`. Permanently deletes the
/// draft. Use after `mxr drafts recover` when you don't want to retry.
pub async fn drafts_discard(draft_id: String) -> anyhow::Result<()> {
    let parsed = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::DeleteDraft {
            draft_id: parsed.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            println!("Discarded draft {parsed}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

pub async fn send_draft(
    draft_id: String,
    dry_run: bool,
    format: Option<OutputFormat>,
    override_safety_token: Option<String>,
    remind_after: Option<String>,
) -> anyhow::Result<()> {
    let draft_id = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let mut client = IpcClient::connect().await?;

    if dry_run {
        let resp = client.request(Request::ListDrafts).await?;
        let drafts = match resp {
            Response::Ok {
                data: ResponseData::Drafts { drafts },
            } => drafts,
            Response::Error { message, .. } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        };
        let draft = drafts
            .into_iter()
            .find(|d| d.id == draft_id)
            .ok_or_else(|| anyhow::anyhow!("Draft not found: {draft_id}"))?;

        let recipients = draft
            .to
            .iter()
            .chain(draft.cc.iter())
            .chain(draft.bcc.iter())
            .map(|addr| addr.email.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if recipients.is_empty() {
            anyhow::bail!("Draft has no recipients; aborting before send");
        }
        print_draft_preview(&draft, true, format)?;
        return Ok(());
    }

    let resp = client
        .request(Request::SendStoredDraft {
            draft_id: draft_id.clone(),
            override_safety_token,
        })
        .await?;
    let receipt = expect_send_receipt(resp)?;
    set_auto_reminder_after_send(&mut client, receipt.as_ref(), remind_after.as_deref()).await?;
    println!("Sent draft {}", draft_id);
    if let Some(info) = receipt.as_ref() {
        println!("Local message id: {}", info.local_message_id);
    }
    Ok(())
}

/// Build the safety-check context for a CLI `--check` invocation. Pulls
/// `thread_id` from the draft's reply headers so the answer-coverage
/// check (and any future thread-scoped check) has the parent thread to
/// load. `no_llm` short-circuits LLM-backed checks.
pub(crate) fn build_check_context(
    draft: &Draft,
    no_llm: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> DraftSafetyContextData {
    let thread_id = draft
        .reply_headers
        .as_ref()
        .and_then(|h| h.thread_id.as_deref())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .map(mxr_core::ThreadId::from_uuid);
    DraftSafetyContextData {
        mode: DraftSafetyModeData::Check,
        reply_all: matches!(draft.intent, mxr_core::DraftIntent::ReplyAll),
        original_message_id: None,
        thread_id,
        allow_llm: !no_llm,
        // CLI `--check` defaults to "send now" timing.
        proposed_send_at: Some(now),
    }
}

/// Run `mxr send <draft-id> --check`. Loads the draft from the daemon
/// and submits it through the safety pipeline without sending. Exits
/// non-zero when at least one Blocker issue is present.
pub async fn check_send(
    draft_id: String,
    format: Option<OutputFormat>,
    no_llm: bool,
) -> anyhow::Result<()> {
    let draft_id = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListDrafts).await?;
    let drafts = match resp {
        Response::Ok {
            data: ResponseData::Drafts { drafts },
        } => drafts,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response listing drafts"),
    };
    let draft = drafts
        .into_iter()
        .find(|d| d.id == draft_id)
        .ok_or_else(|| anyhow::anyhow!("Draft not found: {draft_id}"))?;

    let context = build_check_context(&draft, no_llm, chrono::Utc::now());
    let resp = client
        .request(Request::CheckDraftSafety {
            draft: draft.clone(),
            context,
        })
        .await?;
    let report = match resp {
        Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        } => report,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from CheckDraftSafety"),
    };

    let resolved = resolve_format(format);
    if matches!(resolved, OutputFormat::Json) {
        let json = serde_json::to_string_pretty(&report)?;
        println!("{json}");
    } else if matches!(resolved, OutputFormat::Jsonl) {
        let lines = jsonl(std::slice::from_ref(&report))?;
        print!("{lines}");
    } else {
        print_safety_report_table(&draft, &report);
    }

    if !report.allowed {
        std::process::exit(2);
    }
    Ok(())
}

/// Run `mxr compose ... --check`. Builds a transient draft from CLI
/// args (NOT persisted; no daemon-side draft row created) and runs the
/// safety pipeline against it. Exits non-zero when at least one
/// Blocker issue is present. Mirrors `check_send` but for the inline
/// compose path used by ad-hoc scripts and pipelines.
pub async fn compose_check(options: ComposeOptions, no_llm: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account = resolve_compose_account(&mut client, options.from.as_deref()).await?;
    let body_text = read_body_input(options.body, options.body_stdin)?.unwrap_or_default();

    let frontmatter = mxr_compose::frontmatter::ComposeFrontmatter {
        to: options.to.clone().unwrap_or_default(),
        cc: options.cc.clone().unwrap_or_default(),
        bcc: options.bcc.clone().unwrap_or_default(),
        subject: options.subject.clone().unwrap_or_default(),
        from: account.email.clone(),
        attach: attachment_strings(&options.attach),
        signature: None,
        ..Default::default()
    };
    let snippet_ctx = snippet_context_from_frontmatter(&frontmatter);
    let body =
        expand_compose_snippets_with_context(&mut client, body_text, Some(&snippet_ctx)).await?;
    let draft = draft_from_frontmatter(
        account.account_id,
        mxr_core::DraftIntent::New,
        &frontmatter,
        body,
    )?;

    let context = build_check_context(&draft, no_llm, chrono::Utc::now());
    let resp = client
        .request(Request::CheckDraftSafety {
            draft: draft.clone(),
            context,
        })
        .await?;
    let report = match resp {
        Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        } => report,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from CheckDraftSafety"),
    };

    let resolved = resolve_format(options.format);
    if matches!(resolved, OutputFormat::Json) {
        let json = serde_json::to_string_pretty(&report)?;
        println!("{json}");
    } else if matches!(resolved, OutputFormat::Jsonl) {
        let lines = jsonl(std::slice::from_ref(&report))?;
        print!("{lines}");
    } else {
        print_safety_report_table(&draft, &report);
    }

    if !report.allowed {
        std::process::exit(2);
    }
    Ok(())
}

fn print_safety_report_table(draft: &Draft, report: &mxr_core::DraftSafetyReport) {
    let verdict = match report.verdict {
        mxr_core::DraftSafetyVerdict::Safe => "SAFE",
        mxr_core::DraftSafetyVerdict::Warn => "WARN",
        mxr_core::DraftSafetyVerdict::Blocked => "BLOCKED",
    };
    println!("Draft {} → {}", draft.id, verdict);
    if report.issues.is_empty() {
        println!("  no issues");
        return;
    }
    for issue in &report.issues {
        let sev = match issue.severity {
            mxr_core::DraftSafetySeverity::Info => "info",
            mxr_core::DraftSafetySeverity::Warning => "warn",
            mxr_core::DraftSafetySeverity::Blocker => "BLOCK",
        };
        println!("  [{sev}] {:?}: {}", issue.code, issue.message);
        if let Some(detail) = &issue.detail {
            println!("        {}", detail);
        }
    }
}

pub async fn schedule_send(draft_id: String, when: String) -> anyhow::Result<()> {
    let draft_id = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let send_at = mxr_core::parse_relative_time(&when, chrono::Utc::now()).map_err(|e| {
        anyhow::anyhow!(
            "Cannot parse '{when}': {e}. Try: `in 2h`, `tomorrow 9am`, `monday 17:00`, or ISO 8601."
        )
    })?;
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ScheduleSend {
            draft_id: draft_id.clone(),
            send_at,
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            let pretty = send_at
                .with_timezone(&chrono::Local)
                .format("%a %b %e %H:%M");
            println!("Scheduled draft {draft_id} for {pretty}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

pub async fn cancel_scheduled_send(draft_id: String) -> anyhow::Result<()> {
    let draft_id = DraftId::from_uuid(uuid::Uuid::parse_str(&draft_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::CancelScheduledSend {
            draft_id: draft_id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            println!("Cancelled scheduled send for draft {draft_id}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
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

async fn resolve_compose_signature(
    client: &mut IpcClient,
    account_id: &AccountId,
    from_email: &str,
    kind: SignatureContextData,
    name: Option<&str>,
    disabled: bool,
) -> anyhow::Result<Option<mxr_compose::ComposeSignature>> {
    if disabled {
        return Ok(None);
    }
    let name = name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let response = client
        .request(Request::ResolveSignature {
            name,
            kind,
            account_id: Some(account_id.clone()),
            from_email: Some(from_email.to_string()),
        })
        .await?;
    match response {
        Response::Ok {
            data: ResponseData::ResolvedSignature { signature },
        } => Ok(signature.map(|signature| mxr_compose::ComposeSignature {
            name: signature.name,
            body: signature.body,
        })),
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

fn apply_signature_to_body(
    body: String,
    signature: Option<&mxr_compose::ComposeSignature>,
) -> String {
    match signature {
        Some(signature) => mxr_compose::append_signature_to_body(&body, &signature.body),
        None => body,
    }
}

async fn expand_compose_snippets(client: &mut IpcClient, body: String) -> anyhow::Result<String> {
    expand_compose_snippets_with_context(client, body, None).await
}

async fn expand_compose_snippets_with_context(
    client: &mut IpcClient,
    body: String,
    context: Option<&SnippetContext>,
) -> anyhow::Result<String> {
    if !body.contains(';') && !body.contains('{') {
        return Ok(body);
    }

    let response = client.request(Request::ListSnippets).await?;
    let snippets = crate::commands::expect_response(response, |response| match response {
        Response::Ok {
            data: ResponseData::Snippets { snippets },
        } => Some(snippets),
        _ => None,
    })?;
    Ok(expand_snippet_keywords_with_context(
        &body, &snippets, context,
    ))
}

/// Contextual values surfaced to snippets at expansion time.
///
/// Built-in tokens like `{first_name}` and `{thread_subject}` resolve
/// against this context. Tokens with no value (or with `context = None`)
/// are left as-is so the unresolved-token warning at send-time can still
/// fire if the user forgot to fill them in manually.
#[derive(Debug, Default, Clone)]
pub(crate) struct SnippetContext {
    pub first_name: Option<String>,
    pub full_name: Option<String>,
    pub thread_subject: Option<String>,
}

impl SnippetContext {
    fn is_empty(&self) -> bool {
        self.first_name.is_none() && self.full_name.is_none() && self.thread_subject.is_none()
    }
}

/// Build a snippet expansion context from a compose frontmatter.
///
/// We use the FIRST address in `to:` for `{first_name}` / `{full_name}`
/// — the user's "primary" recipient. A reply with the original sender
/// in `to:` is the common shape, so this captures the intent without
/// guessing. CC/BCC are deliberately ignored: those are observers, not
/// the person you're addressing.
pub(crate) fn snippet_context_from_frontmatter(
    frontmatter: &mxr_compose::frontmatter::ComposeFrontmatter,
) -> SnippetContext {
    let recipients = parse_addresses(&frontmatter.to);
    let primary = recipients.first();
    let full_name = primary.and_then(|addr| addr.name.clone()).map(|name| {
        // Strip surrounding quotes ("Last, First" style) and trim.
        name.trim().trim_matches('"').to_string()
    });
    let first_name = full_name
        .as_ref()
        .and_then(|name| first_name_from_full(name))
        .or_else(|| {
            // Fall back to the email local-part if there's no display
            // name at all. Better to fill in `alice` than to leave a
            // raw `{first_name}` token in someone's draft.
            primary.and_then(|addr| {
                addr.email
                    .split('@')
                    .next()
                    .filter(|local| !local.is_empty())
                    .map(|local| local.to_string())
            })
        });
    let thread_subject = strip_reply_forward_prefix(&frontmatter.subject);
    SnippetContext {
        first_name,
        full_name,
        thread_subject,
    }
}

fn first_name_from_full(name: &str) -> Option<String> {
    // Handle "Last, First" (with optional trailing extras) — the comma
    // form is common in directory exports. Take the first whitespace-
    // delimited token AFTER the comma as the first name.
    if let Some((_last, after_comma)) = name.split_once(',') {
        let candidate = after_comma
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('"');
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    name.split_whitespace()
        .next()
        .map(|s| s.trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

fn strip_reply_forward_prefix(subject: &str) -> Option<String> {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut current = trimmed;
    loop {
        let lower = current.to_ascii_lowercase();
        if let Some(rest) = lower
            .strip_prefix("re:")
            .or_else(|| lower.strip_prefix("fwd:"))
            .or_else(|| lower.strip_prefix("fw:"))
        {
            current = current[current.len() - rest.len()..].trim_start();
            continue;
        }
        break;
    }
    if current.is_empty() {
        None
    } else {
        Some(current.to_string())
    }
}

#[cfg(test)]
fn expand_snippet_keywords(body: &str, snippets: &[SnippetData]) -> String {
    expand_snippet_keywords_with_context(body, snippets, None)
}

fn expand_snippet_keywords_with_context(
    body: &str,
    snippets: &[SnippetData],
    context: Option<&SnippetContext>,
) -> String {
    // Fast path: no `;name` keywords to process. Still run the
    // builtin-var pass so bare `{first_name}` / `{today}` tokens the
    // user typed directly get resolved.
    if snippets.is_empty() {
        return expand_builtin_snippet_vars(body, context);
    }
    let by_name = snippets
        .iter()
        .map(|snippet| (snippet.name.as_str(), snippet.body.as_str()))
        .collect::<HashMap<_, _>>();

    let mut output = String::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        let rest = &body[index..];
        if !rest.starts_with(';') || !is_snippet_boundary(body, index) {
            let ch = rest.chars().next().expect("non-empty rest");
            output.push(ch);
            index += ch.len_utf8();
            continue;
        }

        let name_start = index + 1;
        let mut name_end = name_start;
        for (offset, ch) in body[name_start..].char_indices() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                name_end = name_start + offset + ch.len_utf8();
            } else {
                break;
            }
        }

        if name_end == name_start {
            output.push(';');
            index += 1;
            continue;
        }

        let name = &body[name_start..name_end];
        if let Some(replacement) = by_name.get(name) {
            output.push_str(&expand_builtin_snippet_vars(replacement, context));
            index = name_end;
        } else {
            output.push_str(&body[index..name_end]);
            index = name_end;
        }
    }

    // Second pass: also resolve bare `{first_name}` / `{thread_subject}`
    // / `{today}` tokens the user typed directly (i.e. not introduced by
    // a snippet keyword). Unresolved tokens are left intact so the
    // send-time validator can warn about them.
    expand_builtin_snippet_vars(&output, context)
}

fn expand_builtin_snippet_vars(template: &str, context: Option<&SnippetContext>) -> String {
    let now = chrono::Local::now();
    let mut out = template
        .replace("{today}", &now.format("%Y-%m-%d").to_string())
        .replace("{date}", &now.format("%Y-%m-%d").to_string())
        .replace("{year}", &now.format("%Y").to_string());
    if let Some(ctx) = context.filter(|ctx| !ctx.is_empty()) {
        if let Some(name) = ctx.first_name.as_deref() {
            out = out.replace("{first_name}", name);
        }
        if let Some(name) = ctx.full_name.as_deref() {
            out = out.replace("{full_name}", name);
        }
        if let Some(subject) = ctx.thread_subject.as_deref() {
            out = out.replace("{thread_subject}", subject);
        }
    }
    out
}

fn is_snippet_boundary(body: &str, semicolon_index: usize) -> bool {
    if semicolon_index == 0 {
        return true;
    }
    body[..semicolon_index]
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '[' | '{' | '"' | '\''))
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
    fallback_intent: mxr_core::DraftIntent,
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
            thread_id: frontmatter.thread_id.clone(),
        });
    Ok(Draft {
        id: DraftId::new(),
        account_id,
        reply_headers,
        intent: if frontmatter.intent == mxr_core::DraftIntent::New {
            fallback_intent
        } else {
            frontmatter.intent
        },
        to: parse_addresses(&frontmatter.to),
        cc: parse_addresses(&frontmatter.cc),
        bcc: parse_addresses(&frontmatter.bcc),
        subject: frontmatter.subject.clone(),
        body_markdown: body,
        attachments: frontmatter.attach.iter().map(PathBuf::from).collect(),
        inline_calendar_reply: None,
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
        // SendReceipt is also an "ack-shaped" success for callers that don't
        // need the message id (e.g. SaveDraft, where receipt is None anyway).
        Response::Ok {
            data: ResponseData::SendReceipt { .. },
        } => Some(()),
        _ => None,
    })
}

/// Decode a daemon response from `Request::SendDraft` / `SendStoredDraft`.
/// Returns the message ids minted during synthetic Sent ingestion.
/// Falls back to `None` for older daemons that still return `Ack`.
fn expect_send_receipt(resp: Response) -> anyhow::Result<Option<SendReceiptInfo>> {
    crate::commands::expect_response(resp, |response| match response {
        Response::Ok {
            data:
                ResponseData::SendReceipt {
                    local_message_id,
                    provider_message_id,
                    rfc2822_message_id,
                },
        } => Some(Some(SendReceiptInfo {
            local_message_id,
            provider_message_id,
            rfc2822_message_id,
        })),
        Response::Ok {
            data: ResponseData::Ack,
        } => Some(None),
        _ => None,
    })
}

struct SendReceiptInfo {
    local_message_id: mxr_core::MessageId,
    #[allow(dead_code)]
    provider_message_id: Option<String>,
    #[allow(dead_code)]
    rfc2822_message_id: String,
}

fn auto_reminder_request_after_send(
    receipt: Option<&SendReceiptInfo>,
    remind_after: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Option<Request>> {
    let Some(remind_after) = remind_after else {
        return Ok(None);
    };
    let Some(receipt) = receipt else {
        anyhow::bail!("daemon did not return a sent message id; cannot set reminder");
    };
    let remind_at = mxr_core::parse_relative_time(remind_after, now).map_err(|e| {
        anyhow::anyhow!(
            "Cannot parse --remind-after value '{remind_after}': {e}. Try: `in 2h`, `tomorrow 9am`, `monday 17:00`, or ISO 8601."
        )
    })?;
    Ok(Some(Request::SetAutoReminder {
        sent_message_id: receipt.local_message_id.clone(),
        remind_at,
    }))
}

async fn set_auto_reminder_after_send(
    client: &mut IpcClient,
    receipt: Option<&SendReceiptInfo>,
    remind_after: Option<&str>,
) -> anyhow::Result<()> {
    let Some(request) =
        auto_reminder_request_after_send(receipt, remind_after, chrono::Utc::now())?
    else {
        return Ok(());
    };
    match client.request(request).await? {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            if let Some(when) = remind_after {
                println!("Reminder set for {when}");
            }
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response setting reminder"),
    }
}

#[derive(serde::Serialize)]
struct DraftPreviewOutput<'a> {
    action: &'static str,
    dry_run: bool,
    draft: &'a Draft,
}

fn print_draft_preview(
    draft: &Draft,
    sending: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = if sending { "send" } else { "save draft" };
    match resolve_format(format) {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&DraftPreviewOutput {
                action,
                dry_run: true,
                draft,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&DraftPreviewOutput {
                action,
                dry_run: true,
                draft,
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "action",
                "dry_run",
                "draft_id",
                "account_id",
                "to",
                "cc",
                "bcc",
                "subject",
                "body_bytes",
                "attachments",
            ])?;
            writer.write_record(vec![
                action.to_string(),
                "true".to_string(),
                draft.id.as_str(),
                draft.account_id.as_str(),
                format_addresses(&draft.to),
                format_addresses(&draft.cc),
                format_addresses(&draft.bcc),
                draft.subject.clone(),
                draft.body_markdown.len().to_string(),
                draft.attachments.len().to_string(),
            ])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => println!("{}", draft.id),
        OutputFormat::Table => {
            println!("Would {action}:");
            println!("  id: {}", draft.id);
            println!("  to: {}", format_addresses(&draft.to));
            println!("  cc: {}", format_addresses(&draft.cc));
            println!("  bcc: {}", format_addresses(&draft.bcc));
            println!("  subject: {}", draft.subject);
            println!("  attachments: {}", draft.attachments.len());
        }
    }
    Ok(())
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
            capabilities: Default::default(),
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
            intent: mxr_core::DraftIntent::ReplyAll,
            references: vec!["<root@example.com>".into()],
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        };

        let draft = draft_from_frontmatter(
            mxr_core::AccountId::new(),
            mxr_core::DraftIntent::Reply,
            &frontmatter,
            "body".into(),
        )
        .unwrap();

        assert_eq!(draft.to.len(), 2);
        assert_eq!(draft.to[0].name.as_deref(), Some("Last, First"));
        assert_eq!(draft.bcc[0].email, "hidden@example.com");
        assert_eq!(draft.intent, mxr_core::DraftIntent::ReplyAll);
        assert_eq!(
            draft.reply_headers.unwrap().in_reply_to,
            "<reply@example.com>"
        );
    }

    fn snippet(name: &str, body: &str) -> SnippetData {
        let now = chrono::Utc::now();
        SnippetData {
            name: name.into(),
            body: body.into(),
            vars: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn snippet_keywords_expand_at_word_boundaries() {
        let body = "Hi ;thanks\n\n;sig".to_string();
        let snippets = vec![
            snippet("thanks", "thanks for reaching out"),
            snippet("sig", "Best,\nmxr"),
        ];

        let expanded = expand_snippet_keywords(&body, &snippets);

        assert_eq!(expanded, "Hi thanks for reaching out\n\nBest,\nmxr");
    }

    #[test]
    fn snippet_keywords_expand_builtin_vars() {
        let body = ";today".to_string();
        let snippets = vec![snippet("today", "Today is {today}")];

        let expanded = expand_snippet_keywords(&body, &snippets);

        assert!(expanded.starts_with("Today is 20"));
        assert!(!expanded.contains("{today}"));
    }

    #[test]
    fn unknown_snippet_keywords_remain_literal() {
        let body = "Keep ;missing and expand ;ok.".to_string();
        let snippets = vec![snippet("ok", "done")];

        let expanded = expand_snippet_keywords(&body, &snippets);

        assert_eq!(expanded, "Keep ;missing and expand done.");
    }

    #[test]
    fn snippet_keywords_do_not_expand_mid_word() {
        let body = "value;ok but (;ok)".to_string();
        let snippets = vec![snippet("ok", "done")];

        let expanded = expand_snippet_keywords(&body, &snippets);

        assert_eq!(expanded, "value;ok but (done)");
    }

    fn frontmatter_with(to: &str, subject: &str) -> mxr_compose::frontmatter::ComposeFrontmatter {
        mxr_compose::frontmatter::ComposeFrontmatter {
            to: to.into(),
            cc: String::new(),
            bcc: String::new(),
            subject: subject.into(),
            from: "me@example.com".into(),
            attach: Vec::new(),
            signature: None,
            ..Default::default()
        }
    }

    /// Phase 3.1: `{first_name}` resolves to the recipient's display
    /// name's first token. The user is replying to "Alice Smith" — the
    /// salutation should read "Hi Alice", not "Hi Alice Smith" or worse,
    /// the literal token.
    #[test]
    fn first_name_resolves_from_recipient_display_name() {
        let fm = frontmatter_with("Alice Smith <alice@example.com>", "Q3 plans");
        let ctx = snippet_context_from_frontmatter(&fm);

        let body = "Hi {first_name},\n\nLet's chat.";
        let expanded = expand_snippet_keywords_with_context(body, &[], Some(&ctx));

        assert_eq!(expanded, "Hi Alice,\n\nLet's chat.");
    }

    /// Phase 3.1: when the recipient has no display name (just an
    /// email), fall back to the email's local part so `{first_name}`
    /// still produces something useful instead of a literal token.
    /// Filling in "alice" is better than shipping "{first_name}".
    #[test]
    fn first_name_falls_back_to_email_local_part_when_no_display_name() {
        let fm = frontmatter_with("alice@example.com", "kickoff");
        let ctx = snippet_context_from_frontmatter(&fm);

        let body = "Hi {first_name}!".to_string();
        let expanded = expand_snippet_keywords_with_context(&body, &[], Some(&ctx));

        assert_eq!(expanded, "Hi alice!");
    }

    /// "Last, First" is a common directory-export display format. We
    /// must pick `First`, not `Last`, so contacts imported from CSVs
    /// don't end up addressed by their last name.
    #[test]
    fn first_name_handles_last_comma_first_display_order() {
        let fm = frontmatter_with(r#""Smith, Alice" <alice@example.com>"#, "Q3 plans");
        let ctx = snippet_context_from_frontmatter(&fm);

        let body = "Hi {first_name}!";
        let expanded = expand_snippet_keywords_with_context(body, &[], Some(&ctx));

        assert_eq!(expanded, "Hi Alice!");
    }

    /// Phase 3.1: `{thread_subject}` resolves to the subject the user
    /// is replying about, with `Re:` / `Fwd:` chains stripped so a
    /// snippet like "About: {thread_subject}" reads "About: Q3 plans"
    /// even when the frontmatter subject is "Re: Re: Q3 plans".
    #[test]
    fn thread_subject_strips_reply_and_forward_prefixes() {
        let fm = frontmatter_with("alice@example.com", "Re: Re: Fwd: Q3 plans");
        let ctx = snippet_context_from_frontmatter(&fm);

        let body = "About: {thread_subject}";
        let expanded = expand_snippet_keywords_with_context(body, &[], Some(&ctx));

        assert_eq!(expanded, "About: Q3 plans");
    }

    /// When context is `None` (no recipient yet, e.g. the initial pre-
    /// editor pass), built-in tokens must remain literal. The send-time
    /// validator still warns about unresolved `{var}` tokens; we must
    /// not silently strip them or swap them for blanks.
    #[test]
    fn unresolved_context_tokens_remain_literal() {
        let body = "Hi {first_name}, thanks for {thread_subject}.";
        let expanded = expand_snippet_keywords_with_context(body, &[], None);

        assert_eq!(expanded, body);
    }

    /// The recipient's email-local-part fallback shouldn't fire when
    /// the recipient address itself is missing. Otherwise `{first_name}`
    /// would silently turn into an empty string, which is worse than
    /// leaving the token visible for the user to fill in.
    #[test]
    fn missing_recipient_leaves_first_name_token_literal() {
        let fm = frontmatter_with("", "no recipient yet");
        let ctx = snippet_context_from_frontmatter(&fm);

        let body = "Hi {first_name}!";
        let expanded = expand_snippet_keywords_with_context(body, &[], Some(&ctx));

        assert_eq!(expanded, "Hi {first_name}!");
    }

    /// Smart vars also fire when reached via a snippet body (not just
    /// when typed directly in the message). Snippet `;greeting` whose
    /// body contains `{first_name}` should land as the resolved name.
    #[test]
    fn snippet_body_can_reference_smart_vars() {
        let fm = frontmatter_with("Alice Smith <alice@example.com>", "");
        let ctx = snippet_context_from_frontmatter(&fm);
        let snippets = vec![snippet("greeting", "Hi {first_name},")];

        let body = ";greeting let me know.".to_string();
        let expanded = expand_snippet_keywords_with_context(&body, &snippets, Some(&ctx));

        assert_eq!(expanded, "Hi Alice, let me know.");
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

    mod check_context_tests {
        use super::super::build_check_context;
        use chrono::Utc;
        use mxr_core::types::Draft;
        use mxr_core::{AccountId, DraftId, DraftIntent, ReplyHeaders, ThreadId};

        fn draft_with(reply_headers: Option<ReplyHeaders>, intent: DraftIntent) -> Draft {
            Draft {
                id: DraftId::new(),
                account_id: AccountId::new(),
                reply_headers,
                intent,
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: "subj".into(),
                body_markdown: "body".into(),
                attachments: vec![],
                inline_calendar_reply: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }
        }

        /// CLI `--check` must forward the draft's thread_id so the
        /// answer-coverage LLM check (and any thread-scoped check) can
        /// load the parent thread.
        #[test]
        fn build_check_context_plumbs_thread_id_from_reply_headers() {
            let uuid = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap();
            let draft = draft_with(
                Some(ReplyHeaders {
                    in_reply_to: "<abc@example.com>".into(),
                    references: vec![],
                    thread_id: Some(uuid.to_string()),
                }),
                DraftIntent::Reply,
            );
            let ctx = build_check_context(&draft, false, Utc::now());
            assert_eq!(
                ctx.thread_id,
                Some(ThreadId::from_uuid(uuid)),
                "thread_id should be parsed from reply_headers"
            );
        }

        /// Default behavior: LLM-backed checks are enabled. Users opt
        /// out with --no-llm (e.g., to skip rate-limited cloud models
        /// or save tokens).
        #[test]
        fn build_check_context_no_llm_flag_disables_llm() {
            let draft = draft_with(None, DraftIntent::New);
            let with_llm = build_check_context(&draft, false, Utc::now());
            let without_llm = build_check_context(&draft, true, Utc::now());
            assert!(with_llm.allow_llm, "default should allow LLM");
            assert!(!without_llm.allow_llm, "--no-llm must disable LLM");
        }

        /// Drafts without reply context (fresh compose) should still
        /// produce a valid context; just no thread.
        #[test]
        fn build_check_context_handles_draft_without_reply_headers() {
            let draft = draft_with(None, DraftIntent::New);
            let ctx = build_check_context(&draft, false, Utc::now());
            assert_eq!(ctx.thread_id, None);
            assert_eq!(ctx.original_message_id, None);
        }

        /// Reply-all intent must propagate so the reply-all sanity
        /// check actually runs (vs. a normal reply that should skip it).
        #[test]
        fn build_check_context_reply_all_intent_propagates() {
            let draft = draft_with(None, DraftIntent::ReplyAll);
            let ctx = build_check_context(&draft, false, Utc::now());
            assert!(ctx.reply_all);

            let draft_reply = draft_with(None, DraftIntent::Reply);
            let ctx_reply = build_check_context(&draft_reply, false, Utc::now());
            assert!(!ctx_reply.reply_all);
        }
    }

    mod remind_after_tests {
        use super::super::{auto_reminder_request_after_send, SendReceiptInfo};
        use chrono::{TimeZone, Utc};
        use mxr_core::MessageId;
        use mxr_protocol::Request;

        #[test]
        fn remind_after_builds_set_auto_reminder_request_from_send_receipt() {
            let message_id = MessageId::new();
            let receipt = SendReceiptInfo {
                local_message_id: message_id.clone(),
                provider_message_id: Some("provider-1".into()),
                rfc2822_message_id: "<sent@example.com>".into(),
            };
            let now = Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap();

            let request =
                auto_reminder_request_after_send(Some(&receipt), Some("in 1h"), now).unwrap();

            match request {
                Some(Request::SetAutoReminder {
                    sent_message_id,
                    remind_at,
                }) => {
                    assert_eq!(sent_message_id, message_id);
                    assert_eq!(remind_at, now + chrono::Duration::hours(1));
                }
                other => panic!("Expected SetAutoReminder request, got {other:?}"),
            }
        }

        #[test]
        fn remind_after_requires_send_receipt_message_id() {
            let now = Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap();

            let error = auto_reminder_request_after_send(None, Some("in 1h"), now)
                .expect_err("missing receipt must be rejected");

            assert!(
                error.to_string().contains("sent message id"),
                "error should explain missing sent id: {error}"
            );
        }

        #[test]
        fn remind_after_absent_does_not_build_request() {
            let now = Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap();

            let request = auto_reminder_request_after_send(None, None, now).unwrap();

            assert!(request.is_none());
        }
    }
}
