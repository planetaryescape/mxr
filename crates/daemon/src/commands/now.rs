//! `mxr now`: the front page. People, Due soon, the Updates card and the
//! evening Reading pick, as the daemon capped them, so the CLI shows the
//! same Now as the web app and the TUI.

use crate::cli::OutputFormat;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use mxr_protocol::{NowData, Request, Response, ResponseData};
use std::fmt::Write as _;

pub async fn run(account: Option<String>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let now = expect_response(
        client.request(Request::GetNow { account_id }).await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::Now { now },
            } => Some(now),
            _ => None,
        },
    )?;
    print!("{}", render(&now, resolve_format(format))?);
    Ok(())
}

fn render(now: &NowData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(now)?),
        OutputFormat::Jsonl => items_jsonl(now)?,
        OutputFormat::Ids => {
            let mut out = String::new();
            for row in &now.people.rows {
                let _ = writeln!(out, "{}", row.row.thread_id);
            }
            for todo in &now.due_soon.todos {
                let _ = writeln!(out, "{}", todo.todo.id);
            }
            if let Some(pick) = &now.reading {
                let _ = writeln!(out, "{}", pick.thread_id);
            }
            out
        }
        OutputFormat::Csv => csv(now),
        OutputFormat::Table => table(now),
    })
}

/// One line per item on Now, each tagged with its section, in order.
fn items_jsonl(now: &NowData) -> anyhow::Result<String> {
    let mut out = String::new();
    for row in &now.people.rows {
        let mut value = serde_json::to_value(row)?;
        tag(&mut value, "people");
        let _ = writeln!(out, "{}", serde_json::to_string(&value)?);
    }
    for todo in &now.due_soon.todos {
        let mut value = serde_json::to_value(todo)?;
        tag(&mut value, "due_soon");
        let _ = writeln!(out, "{}", serde_json::to_string(&value)?);
    }
    if let Some(card) = &now.updates {
        let mut value = serde_json::to_value(card)?;
        tag(&mut value, "updates");
        let _ = writeln!(out, "{}", serde_json::to_string(&value)?);
    }
    if let Some(pick) = &now.reading {
        let mut value = serde_json::to_value(pick)?;
        tag(&mut value, "reading");
        let _ = writeln!(out, "{}", serde_json::to_string(&value)?);
    }
    Ok(out)
}

fn tag(value: &mut serde_json::Value, section: &str) {
    if let Some(object) = value.as_object_mut() {
        object.insert("section".into(), section.into());
    }
}

fn csv(now: &NowData) -> String {
    use super::owed::csv_escape;
    let mut out = String::from("section,id,title,why\n");
    for row in &now.people.rows {
        let who = row
            .row
            .counterparty_name
            .as_deref()
            .unwrap_or(&row.row.counterparty_email);
        let _ = writeln!(
            out,
            "people,{},{},{}",
            row.row.thread_id,
            csv_escape(who),
            csv_escape(&row.why)
        );
    }
    for todo in &now.due_soon.todos {
        let _ = writeln!(
            out,
            "due_soon,{},{},{}",
            todo.todo.id,
            csv_escape(&todo.todo.title),
            csv_escape(&todo.why)
        );
    }
    if let Some(card) = &now.updates {
        let _ = writeln!(out, "updates,,{},", csv_escape(&card.line));
    }
    if let Some(pick) = &now.reading {
        let _ = writeln!(
            out,
            "reading,{},{},{}",
            pick.thread_id,
            csv_escape(&pick.subject),
            csv_escape(&pick.why)
        );
    }
    out
}

fn section_title(out: &mut String, title: &str, more: Option<&str>) {
    let _ = match more {
        Some(more) => writeln!(out, "\n{title}  ({more})"),
        None => writeln!(out, "\n{title}"),
    };
}

fn table(now: &NowData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", terminal_text(&now.headline));
    if let Some(empty) = &now.empty_state {
        let _ = writeln!(out, "\n{}", terminal_text(empty));
        return out;
    }
    if !now.people.rows.is_empty() {
        section_title(&mut out, "PEOPLE", now.people.more_line.as_deref());
        if let Some(overload) = &now.people.overload_line {
            let _ = writeln!(out, "  {overload}");
        }
        for person in &now.people.rows {
            let row = &person.row;
            let who = row
                .counterparty_name
                .as_deref()
                .unwrap_or(&row.counterparty_email);
            let _ = writeln!(
                out,
                "  {:<22}  {}",
                terminal_text(&super::owed::truncate(who, 22)),
                terminal_text(&row.subject)
            );
            let _ = writeln!(out, "    {}", terminal_text(&person.why));
            if let Some(question) = &person.new_sender {
                let _ = writeln!(
                    out,
                    "    {} Answer with `mxr sender kind {} KIND`.",
                    question.question,
                    terminal_text(&question.sender_email)
                );
            }
        }
    }
    if !now.due_soon.todos.is_empty() {
        section_title(&mut out, "DUE SOON", now.due_soon.more_line.as_deref());
        for item in &now.due_soon.todos {
            let todo = &item.todo;
            let amount = todo
                .amount
                .as_ref()
                .map(|amount| format!("  {}", amount.display))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {}{}  {}",
                terminal_text(&todo.title),
                amount,
                terminal_text(&todo.when_label)
            );
        }
    }
    if let Some(card) = &now.updates {
        section_title(&mut out, "UPDATES", Some("early version"));
        let _ = writeln!(out, "  {}", terminal_text(&card.line));
    }
    if let Some(pick) = &now.reading {
        section_title(&mut out, "READING", Some("for tonight"));
        let from = pick.sender_name.as_deref().unwrap_or(&pick.sender_email);
        let _ = writeln!(
            out,
            "  {}  {}",
            terminal_text(from),
            terminal_text(&pick.subject)
        );
        let _ = writeln!(out, "    {}", terminal_text(&pick.why));
    }
    if let Some(not_now) = &now.not_now {
        let _ = writeln!(out, "\n{not_now}");
    }
    out
}
