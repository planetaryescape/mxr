use crate::ipc_client::IpcClient;
use chrono::{Datelike, Duration, NaiveTime, TimeZone, Utc, Weekday};
use mxr_core::id::MessageId;
use mxr_core::types::Envelope;
use mxr_protocol::*;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_message_id(id_str: &str) -> anyhow::Result<MessageId> {
    let uuid = uuid::Uuid::parse_str(id_str)
        .map_err(|e| anyhow::anyhow!("Invalid message ID '{}': {}", id_str, e))?;
    Ok(MessageId::from_uuid(uuid))
}

async fn resolve_message_ids(
    client: &mut IpcClient,
    message_id: Option<String>,
    search: Option<String>,
) -> anyhow::Result<Vec<MessageId>> {
    match (message_id, search) {
        (Some(id), _) => Ok(vec![parse_message_id(&id)?]),
        (None, Some(query)) => {
            let resp = client
                .request(Request::Search { query, limit: 1000 })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::SearchResults { results },
                } => Ok(results.into_iter().map(|r| r.message_id).collect()),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response from search"),
            }
        }
        (None, None) => anyhow::bail!("Provide a message ID or --search query"),
    }
}

struct MutationSelection {
    ids: Vec<MessageId>,
    envelopes: Vec<Envelope>,
    used_search: bool,
}

async fn resolve_mutation_selection(
    client: &mut IpcClient,
    message_id: Option<String>,
    search: Option<String>,
) -> anyhow::Result<MutationSelection> {
    let used_search = message_id.is_none() && search.is_some();
    let ids = resolve_message_ids(client, message_id, search).await?;
    let envelopes = if ids.is_empty() {
        Vec::new()
    } else {
        let resp = client
            .request(Request::ListEnvelopesByIds {
                message_ids: ids.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } => envelopes,
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response from envelope lookup"),
        }
    };

    Ok(MutationSelection {
        ids,
        envelopes,
        used_search,
    })
}

fn requires_confirmation(
    destructive: bool,
    used_search: bool,
    matched_count: usize,
    yes: bool,
) -> bool {
    !yes && (destructive || used_search || matched_count > 1)
}

fn render_selection_preview_lines(action: &str, selection: &MutationSelection) -> Vec<String> {
    let preview_limit = 8usize;
    let mut lines = vec![format!("Would {action} {} message(s)", selection.ids.len())];

    if !selection.envelopes.is_empty() {
        lines.push(String::new());
        for envelope in selection.envelopes.iter().take(preview_limit) {
            let from = envelope
                .from
                .name
                .as_deref()
                .unwrap_or(&envelope.from.email);
            let subject = if envelope.subject.is_empty() {
                "(no subject)"
            } else {
                &envelope.subject
            };
            lines.push(format!(
                "- {} | {} | {}",
                envelope.id.as_str(),
                from,
                subject
            ));
        }
        if selection.envelopes.len() > preview_limit {
            lines.push(format!(
                "... and {} more",
                selection.envelopes.len() - preview_limit
            ));
        }
    }

    lines
}

fn print_selection_preview(action: &str, selection: &MutationSelection) {
    for line in render_selection_preview_lines(action, selection) {
        println!("{line}");
    }
}

fn confirm_action(action: &str, selection: &MutationSelection) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "Confirmation required for `{action}`. Re-run with --yes or inspect with --dry-run."
        );
    }

    print_selection_preview(action, selection);
    print!("\nContinue? [y/N] ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();
    if answer == "y" || answer == "yes" {
        return Ok(());
    }

    anyhow::bail!("Aborted")
}

async fn run_simple_mutation<F>(
    client: &mut IpcClient,
    selection: MutationSelection,
    action: &str,
    success_message: &str,
    yes: bool,
    dry_run: bool,
    destructive: bool,
    build_request: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Vec<MessageId>) -> Request,
{
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    if dry_run {
        print_selection_preview(action, &selection);
        return Ok(());
    }

    if requires_confirmation(destructive, selection.used_search, selection.ids.len(), yes) {
        confirm_action(action, &selection)?;
    }

    let resp = client.request(build_request(selection.ids)).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => println!("{success_message}"),
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn parse_snooze_until(until: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    let now = Utc::now();
    let lower = until.trim().to_lowercase();

    let dt = match lower.as_str() {
        "tomorrow" => {
            let date = (now + Duration::days(1)).date_naive();
            let time = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
            Utc.from_utc_datetime(&date.and_time(time))
        }
        "tonight" => {
            let date = now.date_naive();
            let time = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
            let candidate = Utc.from_utc_datetime(&date.and_time(time));
            if candidate <= now {
                candidate + Duration::days(1)
            } else {
                candidate
            }
        }
        "monday" => next_weekday(now, Weekday::Mon, 9),
        "tuesday" => next_weekday(now, Weekday::Tue, 9),
        "wednesday" => next_weekday(now, Weekday::Wed, 9),
        "thursday" => next_weekday(now, Weekday::Thu, 9),
        "friday" => next_weekday(now, Weekday::Fri, 9),
        "weekend" | "saturday" => next_weekday(now, Weekday::Sat, 9),
        "sunday" => next_weekday(now, Weekday::Sun, 9),
        _ => {
            // Try ISO 8601
            chrono::DateTime::parse_from_rfc3339(until)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(until, "%Y-%m-%dT%H:%M:%S")
                        .map(|ndt| Utc.from_utc_datetime(&ndt))
                })
                .map_err(|_| {
                    anyhow::anyhow!(
                        "Cannot parse '{}'. Use: tomorrow, tonight, monday, weekend, or ISO 8601",
                        until
                    )
                })?
        }
    };
    Ok(dt)
}

fn next_weekday(now: chrono::DateTime<Utc>, target: Weekday, hour: u32) -> chrono::DateTime<Utc> {
    let current = now.weekday().num_days_from_monday();
    let target_day = target.num_days_from_monday();
    let days_ahead = if target_day <= current {
        7 - (current - target_day)
    } else {
        target_day - current
    };
    let date = (now + Duration::days(days_ahead as i64)).date_naive();
    let time = NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    Utc.from_utc_datetime(&date.and_time(time))
}

// ---------------------------------------------------------------------------
// Simple mutations
// ---------------------------------------------------------------------------

pub async fn archive(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "archive",
        "Archived",
        yes,
        dry_run,
        false,
        |ids| Request::Mutation(MutationCommand::Archive { message_ids: ids }),
    )
    .await
}

pub async fn trash(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "trash",
        "Trashed",
        yes,
        dry_run,
        true,
        |ids| Request::Mutation(MutationCommand::Trash { message_ids: ids }),
    )
    .await
}

pub async fn spam(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "mark as spam",
        "Marked as spam",
        yes,
        dry_run,
        true,
        |ids| Request::Mutation(MutationCommand::Spam { message_ids: ids }),
    )
    .await
}

pub async fn star(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "star",
        "Starred",
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::Star {
                message_ids: ids,
                starred: true,
            })
        },
    )
    .await
}

pub async fn unstar(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "unstar",
        "Unstarred",
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::Star {
                message_ids: ids,
                starred: false,
            })
        },
    )
    .await
}

pub async fn mark_read(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "mark as read",
        "Marked as read",
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: true,
            })
        },
    )
    .await
}

pub async fn unread(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        "mark as unread",
        "Marked as unread",
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: false,
            })
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Label mutations
// ---------------------------------------------------------------------------

pub async fn label(
    name: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        &format!("add label '{name}'"),
        &format!("Added label '{name}'"),
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: ids,
                add: vec![name.clone()],
                remove: vec![],
            })
        },
    )
    .await
}

pub async fn unlabel(
    name: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        &format!("remove label '{name}'"),
        &format!("Removed label '{name}'"),
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: ids,
                add: vec![],
                remove: vec![name.clone()],
            })
        },
    )
    .await
}

pub async fn move_msg(
    target_label: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        &format!("move to '{target_label}'"),
        &format!("Moved to '{target_label}'"),
        yes,
        dry_run,
        false,
        |ids| {
            Request::Mutation(MutationCommand::Move {
                message_ids: ids,
                target_label: target_label.clone(),
            })
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Snooze
// ---------------------------------------------------------------------------

pub async fn snooze(
    message_id: Option<String>,
    until: String,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let wake_at = parse_snooze_until(&until)?;
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_selection_preview(
            &format!("snooze until {}", wake_at.to_rfc3339()),
            &selection,
        );
        return Ok(());
    }
    if requires_confirmation(false, selection.used_search, selection.ids.len(), yes) {
        confirm_action(
            &format!("snooze until {}", wake_at.to_rfc3339()),
            &selection,
        )?;
    }
    for id in &selection.ids {
        let resp = client
            .request(Request::Snooze {
                message_id: id.clone(),
                wake_at,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {}
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    println!(
        "Snoozed {} message(s) until {}",
        selection.ids.len(),
        wake_at.to_rfc3339()
    );
    Ok(())
}

pub async fn unsnooze(message_id: Option<String>, all: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    if all {
        let resp = client.request(Request::ListSnoozed).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            } => {
                if snoozed.is_empty() {
                    println!("No snoozed messages");
                    return Ok(());
                }
                for s in &snoozed {
                    let resp = client
                        .request(Request::Unsnooze {
                            message_id: s.message_id.clone(),
                        })
                        .await?;
                    match resp {
                        Response::Ok {
                            data: ResponseData::Ack,
                        } => {}
                        Response::Error { message } => {
                            eprintln!("Failed to unsnooze {}: {}", s.message_id, message);
                        }
                        _ => {}
                    }
                }
                println!("Unsnoozed {} message(s)", snoozed.len());
            }
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
    } else {
        let id_str = message_id.ok_or_else(|| anyhow::anyhow!("Provide a message ID or --all"))?;
        let id = parse_message_id(&id_str)?;
        let resp = client.request(Request::Unsnooze { message_id: id }).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => println!("Unsnoozed"),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    Ok(())
}

pub async fn snoozed() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListSnoozed).await?;
    match resp {
        Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        } => {
            if snoozed.is_empty() {
                println!("No snoozed messages");
            } else {
                println!(
                    "{:<38} {:<25} {:<25}",
                    "MESSAGE ID", "SNOOZED AT", "WAKE AT"
                );
                println!("{}", "-".repeat(88));
                for s in &snoozed {
                    println!(
                        "{:<38} {:<25} {:<25}",
                        s.message_id.as_str(),
                        s.snoozed_at.to_rfc3339(),
                        s.wake_at.to_rfc3339(),
                    );
                }
                println!("\n{} snoozed message(s)", snoozed.len());
            }
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Compose
// ---------------------------------------------------------------------------

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

fn resolve_compose_from_address(explicit_from: Option<String>) -> String {
    if let Some(from) = explicit_from {
        return from;
    }

    let config = mxr_config::load_config().unwrap_or_default();
    if let Some(default_key) = config.general.default_account.as_deref() {
        if let Some(account) = config.accounts.get(default_key) {
            return account.email.clone();
        }
    }

    config
        .accounts
        .values()
        .next()
        .map(|account| account.email.clone())
        .unwrap_or_else(|| "you@example.com".to_string())
}

pub async fn compose(options: ComposeOptions) -> anyhow::Result<()> {
    let from_addr = resolve_compose_from_address(options.from);

    if options.dry_run {
        println!("Would open $EDITOR to compose new email from {}", from_addr);
        return Ok(());
    }

    let (path, cursor_line) =
        mxr_compose::create_draft_file(mxr_compose::ComposeKind::New, &from_addr)?;

    // If inline body provided, append it to the draft file
    if let Some(b) = &options.body {
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{}{}", content, b))?;
    } else if options.body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{}{}", content, stdin_body))?;
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
            updated = updated.replacen("to: \"\"", &format!("to: \"{}\"", to_val), 1);
        }
        if let Some(cc_val) = &options.cc {
            updated = updated.replacen("cc: \"\"", &format!("cc: \"{}\"", cc_val), 1);
        }
        if let Some(bcc_val) = &options.bcc {
            updated = updated.replacen("bcc: \"\"", &format!("bcc: \"{}\"", bcc_val), 1);
        }
        if let Some(subj) = &options.subject {
            updated = updated.replacen("subject: \"\"", &format!("subject: \"{}\"", subj), 1);
        }
        std::fs::write(&path, updated)?;
    }

    let editor = mxr_compose::editor::resolve_editor(None);
    mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "Command tests live near the helper they cover; moving them is out of scope here"
)]
mod tests {
    use super::{
        render_selection_preview_lines, requires_confirmation, resolve_compose_from_address,
        MutationSelection,
    };
    use chrono::Utc;
    use mxr_core::id::{AccountId, ThreadId};
    use mxr_core::types::{Address, Envelope, MessageFlags, UnsubscribeMethod};
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn compose_from_prefers_explicit_value() {
        let resolved = resolve_compose_from_address(Some("alice@example.com".into()));
        assert_eq!(resolved, "alice@example.com");
    }

    #[test]
    fn compose_from_falls_back_when_no_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev_home = std::env::var("HOME").ok();
        let temp_home =
            std::env::temp_dir().join(format!("mxr-compose-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_home);
        std::fs::create_dir_all(&temp_home).unwrap();
        unsafe { std::env::set_var("HOME", &temp_home) };

        let resolved = resolve_compose_from_address(None);

        match prev_home {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        let _ = std::fs::remove_dir_all(&temp_home);

        assert_eq!(resolved, "you@example.com");
    }

    fn test_envelope(subject: &str) -> Envelope {
        Envelope {
            id: mxr_core::MessageId::new(),
            account_id: AccountId::new(),
            provider_id: format!("provider-{subject}"),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Buildkite".into()),
                email: "buildkite@example.com".into(),
            },
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: String::new(),
            has_attachments: false,
            size_bytes: 0,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: Vec::new(),
        }
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
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    };

    if dry_run {
        println!("Would open $EDITOR to reply to {}", message_id);
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
        std::fs::write(&path, format!("{}{}", content, b))?;
    } else if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{}{}", content, stdin_body))?;
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
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    };

    if dry_run {
        println!("Would open $EDITOR to reply-all to {}", message_id);
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
        std::fs::write(&path, format!("{}{}", content, b))?;
    } else if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{}{}", content, stdin_body))?;
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
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    };

    if dry_run {
        println!("Would open $EDITOR to forward {}", message_id);
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
        let updated = content.replacen("to: \"\"", &format!("to: \"{}\"", to_val), 1);
        std::fs::write(&path, updated)?;
    }

    if let Some(b) = &body {
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{}{}", content, b))?;
    } else if body_stdin {
        use std::io::Read;
        let mut stdin_body = String::new();
        std::io::stdin().read_to_string(&mut stdin_body)?;
        let content = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{}{}", content, stdin_body))?;
    }

    let editor = mxr_compose::editor::resolve_editor(None);
    mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor_line)).await?;

    println!("Draft saved to {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Drafts / Send
// ---------------------------------------------------------------------------

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
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn send_draft(_draft_id: String) -> anyhow::Result<()> {
    println!("SendDraft via CLI is handled by compose flow (compose -> edit -> auto-send)");
    println!("Use `mxr compose` to create and send an email in one step.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Unsubscribe / Open / Attachments
// ---------------------------------------------------------------------------

pub async fn unsubscribe(
    message_id: Option<String>,
    yes: bool,
    search: Option<String>,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_selection_preview("unsubscribe", &selection);
        return Ok(());
    }
    if requires_confirmation(true, selection.used_search, selection.ids.len(), yes) {
        confirm_action("unsubscribe", &selection)?;
    }
    for id in &selection.ids {
        let resp = client
            .request(Request::Unsubscribe {
                message_id: id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {}
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    println!("Unsubscribed from {} message(s)", selection.ids.len());
    Ok(())
}

pub async fn open_in_browser(message_id: String) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetEnvelope { message_id: id })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Envelope { envelope },
        } => {
            let url = format!(
                "https://mail.google.com/mail/u/0/#inbox/{}",
                envelope.provider_id
            );
            #[cfg(target_os = "macos")]
            std::process::Command::new("open").arg(&url).spawn()?;
            #[cfg(target_os = "linux")]
            std::process::Command::new("xdg-open").arg(&url).spawn()?;
            println!("Opened in browser: {}", url);
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn attachments_list(message_id: String) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::GetBody { message_id: id }).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => {
            if body.attachments.is_empty() {
                println!("No attachments");
            } else {
                println!(
                    "{:<4} {:<40} {:<25} {:>10}",
                    "#", "FILENAME", "TYPE", "SIZE"
                );
                println!("{}", "-".repeat(82));
                for (i, att) in body.attachments.iter().enumerate() {
                    println!(
                        "{:<4} {:<40} {:<25} {:>10}",
                        i + 1,
                        att.filename,
                        att.mime_type,
                        format_bytes(att.size_bytes),
                    );
                }
                println!("\n{} attachment(s)", body.attachments.len());
            }
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn attachments_download(
    message_id: String,
    index: Option<usize>,
    dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let attachments = load_attachments(&mut client, &id).await?;

    let selected: Vec<(usize, &mxr_core::AttachmentMeta)> = match index {
        Some(index) => vec![(index, attachment_by_index(&attachments, index)?)],
        None => attachments
            .iter()
            .enumerate()
            .map(|(idx, attachment)| (idx + 1, attachment))
            .collect(),
    };

    for (display_index, attachment) in selected {
        let path = request_attachment_file(
            &mut client,
            Request::DownloadAttachment {
                message_id: id.clone(),
                attachment_id: attachment.id.clone(),
            },
        )
        .await?;
        let final_path = if let Some(target_dir) = dir.as_ref() {
            std::fs::create_dir_all(target_dir)?;
            let target = target_dir.join(&attachment.filename);
            std::fs::copy(&path, &target)?;
            target
        } else {
            path
        };
        println!(
            "#{} {} -> {}",
            display_index,
            attachment.filename,
            final_path.display()
        );
    }

    Ok(())
}

pub async fn attachments_open(message_id: String, index: usize) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let attachments = load_attachments(&mut client, &id).await?;
    let attachment = attachment_by_index(&attachments, index)?;

    let path = request_attachment_file(
        &mut client,
        Request::OpenAttachment {
            message_id: id,
            attachment_id: attachment.id.clone(),
        },
    )
    .await?;
    println!("Opened {} ({})", attachment.filename, path.display());
    Ok(())
}

async fn load_attachments(
    client: &mut IpcClient,
    message_id: &MessageId,
) -> anyhow::Result<Vec<mxr_core::AttachmentMeta>> {
    let resp = client
        .request(Request::GetBody {
            message_id: message_id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => {
            if body.attachments.is_empty() {
                anyhow::bail!("No attachments");
            }
            Ok(body.attachments)
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

fn attachment_by_index(
    attachments: &[mxr_core::AttachmentMeta],
    index: usize,
) -> anyhow::Result<&mxr_core::AttachmentMeta> {
    attachments
        .get(index.saturating_sub(1))
        .ok_or_else(|| anyhow::anyhow!("Attachment index {} out of range", index))
}

async fn request_attachment_file(
    client: &mut IpcClient,
    request: Request,
) -> anyhow::Result<PathBuf> {
    let resp = client.request(request).await?;
    match resp {
        Response::Ok {
            data: ResponseData::AttachmentFile { file },
        } => Ok(PathBuf::from(file.path)),
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
