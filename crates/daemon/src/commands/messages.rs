//! `mxr messages`: people you talk with, one row each, with your
//! conversations inside as topics. Every view is the daemon's, so the CLI
//! shows the same bands, order and new text as the web app and the TUI.

use crate::cli::{MessagesAction, MessagesTurnArg, OutputFormat};
use crate::commands::{expect_response, resolve_account, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use chrono::{DateTime, Local, Utc};
use mxr_core::id::ThreadId;
use mxr_protocol::{
    AckPlanData, MergeSuggestionData, MessageLayoutData, MessagesData, MessagesRowData,
    MessagesTurnData, PersonMergeData, PersonPageData, Request, Response, ResponseData,
};
use std::fmt::Write as _;

pub async fn run(
    action: Option<MessagesAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let format = resolve_format(format);
    match action.unwrap_or(MessagesAction::List {
        turn: None,
        limit: 50,
    }) {
        MessagesAction::List { turn, limit } => {
            let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
            let messages = expect_response(
                client
                    .request(Request::ListMessages {
                        account_id,
                        turn: turn.map(|turn| match turn {
                            MessagesTurnArg::Mine => MessagesTurnData::Mine,
                            MessagesTurnArg::Theirs => MessagesTurnData::Theirs,
                        }),
                        limit,
                    })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::Messages { messages },
                    } => Some(messages),
                    _ => None,
                },
            )?;
            print!("{}", render_list(&messages, format)?);
        }
        MessagesAction::Person { person, topic } => {
            let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
            let topic = topic
                .map(|topic| topic.parse::<ThreadId>())
                .transpose()
                .map_err(|_| anyhow::anyhow!("--topic takes a thread id"))?;
            let page = expect_response(
                client
                    .request(Request::GetPerson {
                        account_id,
                        person,
                        topic,
                    })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::PersonPage { page },
                    } => Some(page),
                    _ => None,
                },
            )?;
            print!("{}", render_person(&page, format)?);
        }
        MessagesAction::Ack { thread_id, dry_run } => {
            let thread_id = thread_id
                .parse::<ThreadId>()
                .map_err(|_| anyhow::anyhow!("not a thread id: {thread_id}"))?;
            let preview = ack(&mut client, &thread_id, true, None).await?;
            let ack = if dry_run {
                preview
            } else {
                // Send exactly what was previewed: the daemon refuses
                // anything else.
                ack(&mut client, &thread_id, false, Some(preview.text)).await?
            };
            print!("{}", render_ack(&ack, format)?);
        }
        MessagesAction::Merge {
            into,
            addresses,
            suggestions,
            dry_run,
        } => {
            if suggestions {
                let account_id =
                    resolve_optional_account(&mut client, account.as_deref()).await?;
                let suggestions = expect_response(
                    client
                        .request(Request::ListMergeSuggestions { account_id })
                        .await?,
                    |response| match response {
                        Response::Ok {
                            data: ResponseData::MergeSuggestions { suggestions },
                        } => Some(suggestions),
                        _ => None,
                    },
                )?;
                print!("{}", render_suggestions(&suggestions, format)?);
                return Ok(());
            }
            let Some(into) = into else {
                anyhow::bail!("Name the person's main address, then the addresses to merge in.");
            };
            if addresses.is_empty() {
                anyhow::bail!("Name at least one address to merge into {into}.");
            }
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let merge = person_merge(
                &mut client,
                Request::MergePeople {
                    account_id,
                    into,
                    addresses,
                    dry_run,
                },
            )
            .await?;
            print!("{}", render_merge(&merge, format)?);
        }
        MessagesAction::Split { address, dry_run } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let merge = person_merge(
                &mut client,
                Request::SplitPerson {
                    account_id,
                    address,
                    dry_run,
                },
            )
            .await?;
            print!("{}", render_merge(&merge, format)?);
        }
    }
    Ok(())
}

async fn ack(
    client: &mut IpcClient,
    thread_id: &ThreadId,
    dry_run: bool,
    expect_text: Option<String>,
) -> anyhow::Result<AckPlanData> {
    expect_response(
        client
            .request(Request::AckMessage {
                thread_id: thread_id.clone(),
                dry_run,
                expect_text,
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::MessagesAck { ack },
            } => Some(ack),
            _ => None,
        },
    )
}

async fn person_merge(client: &mut IpcClient, request: Request) -> anyhow::Result<PersonMergeData> {
    expect_response(client.request(request).await?, |response| match response {
        Response::Ok {
            data: ResponseData::PersonMerge { merge },
        } => Some(merge),
        _ => None,
    })
}

fn json<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

/// "16h", "3d", "just now".
fn age(at: DateTime<Utc>) -> String {
    let seconds = (Utc::now() - at).num_seconds().max(0);
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m", seconds / 60),
        3600..=86_399 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

fn bands(messages: &MessagesData) -> [(&'static str, &[MessagesRowData]); 4] {
    [
        ("your_turn", &messages.your_turn),
        ("pinned", &messages.pinned),
        ("recent", &messages.recent),
        ("quiet", &messages.quiet),
    ]
}

fn render_list(messages: &MessagesData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => json(messages)?,
        OutputFormat::Jsonl => {
            let mut out = String::new();
            for (_, rows) in bands(messages) {
                for row in rows {
                    let _ = writeln!(out, "{}", serde_json::to_string(row)?);
                }
            }
            out
        }
        // Row ids, so they feed `mxr messages person`.
        OutputFormat::Ids => {
            let mut out = String::new();
            for (_, rows) in bands(messages) {
                for row in rows {
                    let _ = writeln!(out, "{}", row.id);
                }
            }
            out
        }
        OutputFormat::Csv => {
            use super::owed::csv_escape;
            let mut out = String::from("band,id,title,your_turn,last_at,preview\n");
            for (band, rows) in bands(messages) {
                for row in rows {
                    let preview = row.preview.as_ref().map_or("", |p| p.text.as_str());
                    let _ = writeln!(
                        out,
                        "{band},{},{},{},{},{}",
                        row.id,
                        csv_escape(&row.title),
                        row.your_turn,
                        row.last_at.to_rfc3339(),
                        csv_escape(preview)
                    );
                }
            }
            out
        }
        OutputFormat::Table => list_table(messages),
    })
}

fn list_table(messages: &MessagesData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", terminal_text(&messages.header));
    if messages.your_turn.is_empty() {
        if let Some(empty) = &messages.empty_state {
            let _ = writeln!(out, "\n{}", terminal_text(empty));
        }
        for lapsed in &messages.lapsed {
            let _ = writeln!(out, "  {}", terminal_text(&lapsed.line));
        }
    } else {
        let _ = writeln!(out, "\nYOUR TURN");
        for row in &messages.your_turn {
            row_lines(&mut out, row);
        }
    }
    if !messages.pinned.is_empty() {
        let faces: Vec<String> = messages
            .pinned
            .iter()
            .map(|row| {
                let dot = if row.your_turn { " *" } else { "" };
                format!("{}{dot}", terminal_text(&row.title))
            })
            .collect();
        let _ = writeln!(out, "\nPINNED  {}", faces.join(" · "));
    }
    if !messages.recent.is_empty() {
        let _ = writeln!(out, "\nRECENT");
        for row in &messages.recent {
            row_lines(&mut out, row);
        }
        if messages.recent_total as usize > messages.recent.len() {
            let _ = writeln!(
                out,
                "  and {} more (--limit)",
                messages.recent_total as usize - messages.recent.len()
            );
        }
    }
    if messages.quiet_total > 0 {
        let _ = writeln!(out, "\nQUIET ({})", messages.quiet_total);
        for row in messages.quiet.iter().take(5) {
            let _ = writeln!(
                out,
                "  {:<24}  {}",
                terminal_text(&super::owed::truncate(&row.title, 24)),
                age(row.last_at)
            );
        }
    }
    out
}

fn row_lines(out: &mut String, row: &MessagesRowData) {
    let mut facts = vec![row.closeness.label().to_string()];
    facts.extend(row.pace_label.clone());
    let when = row.turn_since.unwrap_or(row.last_at);
    let _ = writeln!(
        out,
        "  {:<24} {:>6}  {}",
        terminal_text(&super::owed::truncate(&row.title, 24)),
        age(when),
        facts.join(" · ")
    );
    if let Some(preview) = &row.preview {
        let text = match preview.kind {
            mxr_protocol::MessagesPreviewKindData::Ask => format!("\"{}\"", preview.text),
            _ => preview.text.clone(),
        };
        let _ = writeln!(out, "    {}", terminal_text(&text));
    }
    if !row.topics.is_empty() {
        let topics: Vec<String> = row
            .topics
            .iter()
            .take(3)
            .map(|t| format!("{} ({})", t.subject, t.state.label()))
            .collect();
        let _ = writeln!(out, "    {}", terminal_text(&topics.join(" · ")));
    }
    let _ = writeln!(out, "    {}", row.id);
}

fn render_person(page: &PersonPageData, format: OutputFormat) -> anyhow::Result<String> {
    if !matches!(format, OutputFormat::Table) {
        return json(page);
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} · {}",
        terminal_text(&page.row.title),
        terminal_text(&page.header_line)
    );
    let _ = writeln!(out, "{}", terminal_text(&page.relationship_line));
    for suggestion in &page.merge_suggestions {
        let _ = writeln!(
            out,
            "Same person? {} ({}). `mxr messages merge {} --dry-run`",
            terminal_text(&suggestion.name),
            suggestion.addresses.join(", "),
            suggestion.addresses.join(" ")
        );
    }
    let selected = page.conversation.as_ref().map(|c| &c.thread_id);
    let _ = writeln!(out, "\nTOPICS");
    for topic in &page.topics {
        let mark = if Some(&topic.thread_id) == selected {
            "*"
        } else {
            " "
        };
        let label = if topic.with.is_empty() {
            topic.subject.clone()
        } else {
            format!("with {}: {}", topic.with.join(", "), topic.subject)
        };
        let _ = writeln!(
            out,
            "  {mark} {:<36} {} · {}  {}",
            terminal_text(&super::owed::truncate(&label, 36)),
            topic.state.label(),
            age(topic.last_at),
            topic.thread_id
        );
    }
    if let Some(conversation) = &page.conversation {
        let _ = writeln!(out, "\n{}", terminal_text(&conversation.subject));
        if conversation.earlier_count > 0 {
            let _ = writeln!(out, "  ({} earlier messages)", conversation.earlier_count);
        }
        for message in &conversation.messages {
            let who = if message.from_me {
                "You".to_string()
            } else {
                message
                    .from
                    .name
                    .clone()
                    .unwrap_or_else(|| message.from.email.clone())
            };
            let when = message.date.with_timezone(&Local).format("%a %-d %b %H:%M");
            let indent = if message.from_me && message.layout == MessageLayoutData::Compact {
                "                "
            } else {
                "  "
            };
            let _ = writeln!(out, "\n{indent}{} · {when}", terminal_text(&who));
            for line in message.text.lines() {
                let _ = writeln!(out, "{indent}  {}", terminal_text(line));
            }
            for attachment in &message.attachments {
                let _ = writeln!(
                    out,
                    "{indent}  [{} · {} KB]",
                    terminal_text(&attachment.filename),
                    attachment.size_bytes.div_ceil(1024)
                );
            }
            if let Some(label) = &message.trimmed_label {
                let _ = writeln!(
                    out,
                    "{indent}  ({label}; `mxr cat {}` shows it as sent)",
                    message.message_id
                );
            }
        }
        let _ = writeln!(
            out,
            "\n{}   `mxr messages ack {}` to say got it",
            terminal_text(&conversation.composer.label),
            conversation.thread_id
        );
    }
    Ok(out)
}

fn render_ack(ack: &AckPlanData, format: OutputFormat) -> anyhow::Result<String> {
    if !matches!(format, OutputFormat::Table) {
        return json(ack);
    }
    let mut out = String::new();
    let to: Vec<String> = ack
        .to
        .iter()
        .map(|a| match &a.name {
            Some(name) => format!("{name} <{}>", a.email),
            None => a.email.clone(),
        })
        .collect();
    let _ = writeln!(
        out,
        "Got it to {} · {}",
        terminal_text(&to.join(", ")),
        terminal_text(&ack.subject)
    );
    let _ = writeln!(out, "---\n{}\n---", terminal_text_block(&ack.text));
    let _ = writeln!(out, "{}", terminal_text(&ack.built_from));
    if ack.dry_run {
        let _ = writeln!(out, "Dry run: nothing sent. Run without --dry-run to send it.");
    } else if let Some(id) = &ack.sent_message_id {
        let _ = writeln!(out, "Sent ({id}).");
    }
    Ok(out)
}

/// Keep the text's own line breaks, cleaning each line.
fn terminal_text_block(text: &str) -> String {
    text.lines()
        .map(|line| terminal_text(line).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_merge(merge: &PersonMergeData, format: OutputFormat) -> anyhow::Result<String> {
    if !matches!(format, OutputFormat::Table) {
        return json(merge);
    }
    let mut out = String::new();
    let _ = writeln!(out, "{}", terminal_text(&merge.summary));
    let _ = writeln!(
        out,
        "{} conversations show under this person.",
        merge.thread_count
    );
    if merge.dry_run {
        let _ = writeln!(out, "Dry run: nothing changed.");
    }
    Ok(out)
}

fn render_suggestions(
    suggestions: &[MergeSuggestionData],
    format: OutputFormat,
) -> anyhow::Result<String> {
    if !matches!(format, OutputFormat::Table) {
        return json(&suggestions);
    }
    if suggestions.is_empty() {
        return Ok("No merges to suggest. mxr suggests one when two addresses you've written to share a name.\n".to_string());
    }
    let mut out = String::new();
    for suggestion in suggestions {
        let _ = writeln!(
            out,
            "{}: {}\n  {}\n  mxr messages merge {} --dry-run",
            terminal_text(&suggestion.name),
            suggestion.addresses.join(", "),
            suggestion.reason,
            suggestion.addresses.join(" ")
        );
    }
    Ok(out)
}
