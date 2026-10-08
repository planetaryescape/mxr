//! `mxr todo`: things email asked you to do, ordered by when to act.
//!
//! The daemon builds the runway, the labels and the why lines; this prints
//! them as a table, or passes the JSON through for scripts and agents.

use crate::cli::{OutputFormat, TodoAction, TodoStateArg};
use crate::commands::selection::parse_message_id;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, print_json, resolve_format, terminal_block};
use chrono::Local;
use mxr_core::id::AccountId;
use mxr_protocol::{
    todo_copy, Request, Response, ResponseData, TodoCatchupData, TodoCatchupDecisionData,
    TodoChangeData, TodoData, TodoEditData, TodoRunwayData, TodoStateActionData, TodoStateData,
};
use std::fmt::Write as _;

/// The payload of one `ResponseData` variant, or the daemon's error.
macro_rules! expect_data {
    ($response:expr, $variant:ident, $field:ident) => {
        expect_response($response, |response| match response {
            Response::Ok {
                data: ResponseData::$variant { $field },
            } => Some($field),
            _ => None,
        })?
    };
}

pub async fn run(
    action: Option<TodoAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let format = resolve_format(format);
    let action = action.unwrap_or(TodoAction::List {
        expired: false,
        state: None,
        limit: 200,
    });
    match action {
        TodoAction::List {
            expired,
            state,
            limit,
        } => {
            let state = match (expired, state) {
                (true, _) => Some(TodoStateData::Expired),
                (false, state) => state.map(state_data),
            };
            match state {
                Some(state) => list_state(&mut client, account_id, state, limit, format).await,
                None => runway(&mut client, account_id, format).await,
            }
        }
        TodoAction::Why { todo_id } => {
            let todo = expect_data!(
                client.request(Request::GetTodo { todo_id }).await?,
                Todo,
                todo
            );
            match format {
                OutputFormat::Json | OutputFormat::Jsonl => print_json(&todo, format)?,
                _ => print!("{}", terminal_block(&why_text(&todo))),
            }
            Ok(())
        }
        TodoAction::Done { todo_ids, dry_run } => {
            set_state(
                &mut client,
                todo_ids,
                TodoStateActionData::Done,
                dry_run,
                format,
            )
            .await
        }
        TodoAction::Undo { todo_ids, dry_run } => {
            set_state(
                &mut client,
                todo_ids,
                TodoStateActionData::Undo,
                dry_run,
                format,
            )
            .await
        }
        TodoAction::Dismiss { todo_ids, dry_run } => {
            set_state(
                &mut client,
                todo_ids,
                TodoStateActionData::Dismiss,
                dry_run,
                format,
            )
            .await
        }
        TodoAction::Snooze {
            todo_id,
            when,
            clear,
            dry_run,
        } => {
            let when = (!clear).then(|| when.join(" "));
            let request = Request::ScheduleTodo {
                todo_id,
                when,
                time_zone: None,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        TodoAction::Edit {
            todo_id,
            edits,
            dry_run,
        } => {
            let edits = edits
                .iter()
                .map(|edit| parse_edit(edit))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let request = Request::UpdateTodo {
                todo_id,
                edits,
                time_zone: None,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        TodoAction::Add {
            message_id,
            title,
            due,
            kind,
            dry_run,
        } => {
            let request = Request::CreateTodo {
                message_id: parse_message_id(&message_id)?,
                title,
                kind,
                due,
                time_zone: None,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        TodoAction::Catchup {
            keep,
            let_go,
            let_go_all,
            undecide,
            dry_run,
        } => {
            let decision = if !undecide.is_empty() {
                Some(TodoCatchupDecisionData::Undecide { todo_ids: undecide })
            } else if !keep.is_empty() {
                Some(TodoCatchupDecisionData::Keep { todo_ids: keep })
            } else if !let_go.is_empty() {
                Some(TodoCatchupDecisionData::LetGo { todo_ids: let_go })
            } else if let_go_all {
                Some(TodoCatchupDecisionData::LetGoAll)
            } else {
                None
            };
            match decision {
                Some(decision) => {
                    let request = Request::SetTodoCatchup {
                        account_id,
                        decision,
                        dry_run,
                    };
                    print_change(change(&mut client, request).await?, format)
                }
                None => catchup(&mut client, account_id, format).await,
            }
        }
    }
}

fn state_data(state: TodoStateArg) -> TodoStateData {
    match state {
        TodoStateArg::Open => TodoStateData::Open,
        TodoStateArg::Done => TodoStateData::Done,
        TodoStateArg::Dismissed => TodoStateData::Dismissed,
        TodoStateArg::Expired => TodoStateData::Expired,
    }
}

fn parse_edit(edit: &str) -> anyhow::Result<TodoEditData> {
    let (field, value) = edit.split_once('=').ok_or_else(|| {
        anyhow::anyhow!("Write edits as field=value, like due=\"fri 9 oct\" or amount=£142.00.")
    })?;
    Ok(TodoEditData {
        field: field.trim().to_string(),
        value: value.trim().to_string(),
    })
}

async fn runway(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let runway = expect_data!(
        client
            .request(Request::GetTodoRunway {
                account_id,
                mark_seen: true,
            })
            .await?,
        TodoRunway,
        runway
    );
    match format {
        OutputFormat::Json | OutputFormat::Jsonl => print_json(&runway, format),
        OutputFormat::Ids => {
            for todo in runway
                .now
                .iter()
                .chain(runway.coming_up.iter().flat_map(|week| &week.todos))
            {
                println!("{}", todo.id);
            }
            Ok(())
        }
        _ => {
            print!("{}", terminal_block(&runway_text(&runway)));
            Ok(())
        }
    }
}

async fn list_state(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    state: TodoStateData,
    limit: u32,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let todos = expect_data!(
        client
            .request(Request::ListTodos {
                account_id,
                state,
                limit,
            })
            .await?,
        Todos,
        todos
    );
    match format {
        OutputFormat::Json => print_json(&todos, format),
        OutputFormat::Jsonl => {
            println!("{}", jsonl(&todos)?);
            Ok(())
        }
        OutputFormat::Ids => {
            todos.iter().for_each(|todo| println!("{}", todo.id));
            Ok(())
        }
        _ => {
            if todos.is_empty() {
                println!("No to-dos in that state.");
            }
            let mut out = String::new();
            for todo in &todos {
                row(&mut out, todo, "");
            }
            if state == TodoStateData::Expired && !todos.is_empty() {
                out.push_str("\nRestore one with `mxr todo undo ID`.\n");
            }
            print!("{}", terminal_block(&out));
            Ok(())
        }
    }
}

async fn catchup(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let catchup = expect_data!(
        client
            .request(Request::GetTodoCatchup {
                account_id: account_id.clone(),
            })
            .await?,
        TodoCatchup,
        catchup
    );
    if matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        return print_json(&catchup, format);
    }
    let runway = expect_data!(
        client
            .request(Request::GetTodoRunway {
                account_id,
                mark_seen: false,
            })
            .await?,
        TodoRunway,
        runway
    );
    print!("{}", terminal_block(&catchup_text(&catchup, &runway)));
    Ok(())
}

async fn set_state(
    client: &mut IpcClient,
    todo_ids: Vec<String>,
    action: TodoStateActionData,
    dry_run: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let request = Request::SetTodoState {
        todo_ids,
        action,
        dry_run,
    };
    print_change(change(client, request).await?, format)
}

async fn change(client: &mut IpcClient, request: Request) -> anyhow::Result<TodoChangeData> {
    Ok(expect_data!(
        client.request(request).await?,
        TodoChange,
        change
    ))
}

fn print_change(change: TodoChangeData, format: OutputFormat) -> anyhow::Result<()> {
    if matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        return print_json(&change, format);
    }
    print!("{}", terminal_block(&change_text(&change)));
    Ok(())
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// The short id printed with each row; any unique prefix works.
fn short_id(id: &str) -> &str {
    let bare = id.strip_prefix("todo_").unwrap_or(id);
    bare.get(..8).unwrap_or(bare)
}

fn row(out: &mut String, todo: &TodoData, indent: &str) {
    let who = todo
        .person_label
        .clone()
        .or_else(|| todo.counterparty.clone())
        .unwrap_or_default();
    let amount = todo
        .amount
        .as_ref()
        .map(|amount| amount.display.clone())
        .unwrap_or_default();
    let unchecked = todo.fields.iter().any(|field| !field.checked);
    let _ = writeln!(
        out,
        "{indent}  {}  {}{}{}  {}{}",
        short_id(&todo.id),
        todo.title,
        if who.is_empty() {
            String::new()
        } else {
            format!("  {who}")
        },
        if amount.is_empty() {
            String::new()
        } else {
            format!("  {amount}")
        },
        todo.when_label,
        if unchecked { " (date unchecked)" } else { "" },
    );
    if let Some(action) = &todo.action {
        let domain = action
            .domain
            .as_deref()
            .map(|domain| format!(" (link goes to {domain})"))
            .unwrap_or_default();
        let _ = writeln!(out, "{indent}            [{}]{domain}", action.label);
    }
    if let Some(done) = &todo.looks_done {
        let _ = writeln!(out, "{indent}            Looks done: {}.", done.reason);
    }
    if let Some(arrived) = todo.source_date {
        let _ = writeln!(
            out,
            "{indent}            Arrived {}",
            arrived.with_timezone(&Local).format("%a %-d %b")
        );
    }
    let _ = writeln!(out, "{indent}            {}", todo.why);
}

fn runway_text(runway: &TodoRunwayData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "To do: {}", runway.header);
    if !runway.first_run.complete {
        let _ = writeln!(
            out,
            "Sorting your mail, newest first: {} messages so far.",
            runway.first_run.scanned
        );
    }
    out.push('\n');
    if let Some(empty) = &runway.empty_state {
        let _ = writeln!(out, "{empty}");
        if runway.done_this_week.is_empty()
            && runway.whenever.is_empty()
            && runway.coming_up.is_empty()
        {
            let _ = writeln!(out, "{}", todo_copy::ADD_ONE_CLI);
        }
    } else {
        let _ = writeln!(out, "{}", runway.headline);
        out.push_str("\nNOW\n");
        for todo in &runway.now {
            row(&mut out, todo, "");
        }
    }
    if !runway.coming_up.is_empty() {
        out.push_str("\nCOMING UP\n");
        for week in &runway.coming_up {
            let _ = writeln!(out, "  {}", week.label);
            for todo in &week.todos {
                row(&mut out, todo, "  ");
            }
        }
    }
    if !runway.later.is_empty() {
        let _ = writeln!(out, "\nLATER ({})", runway.later.len());
        for todo in &runway.later {
            row(&mut out, todo, "");
        }
    }
    if !runway.whenever.is_empty() {
        let _ = writeln!(out, "\nWHENEVER ({})", runway.whenever.len());
        for todo in &runway.whenever {
            row(&mut out, todo, "");
        }
    }
    if !runway.done_this_week.is_empty() {
        let titles: Vec<&str> = runway
            .done_this_week
            .iter()
            .map(|todo| todo.title.as_str())
            .collect();
        let _ = writeln!(
            out,
            "\nDONE THIS WEEK ({}): {}",
            titles.len(),
            titles.join(", ")
        );
    }
    if runway.catchup_count > 0 {
        let _ = writeln!(
            out,
            "\nCatch up: {} {} from before mxr sorted your mail might still need you. `mxr todo catchup` shows them once.",
            runway.catchup_count,
            if runway.catchup_count == 1 { "thing" } else { "things" }
        );
    }
    if runway.expired_since_last_looked > 0 {
        let _ = writeln!(
            out,
            "\n{} expired since you last looked: `mxr todo list --expired`.",
            runway.expired_since_last_looked
        );
    }
    if runway.empty_state.is_none() {
        out.push_str(
            "\nmxr todo done ID · why ID · snooze ID WHEN · edit ID field=value · dismiss ID\n",
        );
    }
    out
}

fn catchup_text(catchup: &TodoCatchupData, runway: &TodoRunwayData) -> String {
    let mut out = String::new();
    let open = runway.now.len()
        + runway
            .coming_up
            .iter()
            .map(|week| week.todos.len())
            .sum::<usize>()
        + runway.later.len()
        + runway.whenever.len();
    let _ = writeln!(out, "{}", todo_copy::FIRST_RUN_TITLE);
    let _ = writeln!(
        out,
        "To do   {open} {}, {} to act on now   {}",
        if open == 1 { "thing" } else { "things" },
        runway.now.len(),
        todo_copy::FIRST_RUN_LINE
    );
    if let Some(line) = &catchup.already_over_line {
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(out, "\n{}", catchup.title);
    if catchup.todos.is_empty() {
        return out;
    }
    let _ = writeln!(out, "{}\n", catchup.why);
    for todo in &catchup.todos {
        row(&mut out, todo, "");
    }
    if catchup.overflow_count > 0 {
        let _ = writeln!(
            out,
            "\n{} more didn't fit and are in the Expired list: `mxr todo list --expired`.",
            catchup.overflow_count
        );
    }
    out.push_str(
        "\nKeep: mxr todo catchup --keep ID...   Let go: mxr todo catchup --let-go ID...   All: mxr todo catchup --let-go-all --dry-run\n",
    );
    out
}

fn change_text(change: &TodoChangeData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", change.summary);
    for todo in &change.changed {
        row(&mut out, todo, "");
    }
    if !change.unchanged.is_empty() {
        let ids: Vec<&str> = change.unchanged.iter().map(|id| short_id(id)).collect();
        let _ = writeln!(
            out,
            "Left as they were (not in a state this applies to): {}",
            ids.join(", ")
        );
    }
    if change.dry_run && !change.changed.is_empty() {
        out.push_str("Run it again without --dry-run to do it.\n");
    }
    out
}

fn why_text(todo: &TodoData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}  (todo {})", todo.title, short_id(&todo.id));
    let _ = writeln!(out, "{}", todo.why);
    if let Some(next) = &todo.next {
        let _ = writeln!(out, "{next}");
    }
    let _ = writeln!(out, "{}\n", todo.when_label);
    for field in &todo.fields {
        let evidence = field
            .evidence
            .as_deref()
            .map(|evidence| format!(": {evidence}"))
            .unwrap_or_default();
        let unchecked = if field.checked {
            ""
        } else {
            "\n                  unchecked: confirm it with `mxr todo edit`"
        };
        let _ = writeln!(
            out,
            "  {:<14} {:<24} from {}{evidence}{unchecked}",
            field_label(&field.field),
            field_value(todo, &field.field),
            field.source_label
        );
    }
    if let Some(action) = &todo.action {
        let _ = writeln!(out, "\nAction: {}", action.label);
        if let Some(domain) = &action.domain {
            let _ = writeln!(
                out,
                "  The email's link goes to {domain}. mxr opens the email, never the link."
            );
        }
    }
    if todo.user_touched {
        out.push_str("\nYou made or changed this to-do, so it never expires.\n");
    }
    out
}

/// The field's value as the row shows it.
fn field_value(todo: &TodoData, field: &str) -> String {
    let day = |at: Option<chrono::DateTime<chrono::Utc>>| {
        at.map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%a %-d %b")
                .to_string()
        })
        .unwrap_or_default()
    };
    match field {
        "title" => todo.title.clone(),
        "kind" => todo.kind.clone(),
        "counterparty" => todo.counterparty.clone().unwrap_or_default(),
        "amount" => todo
            .amount
            .as_ref()
            .map(|amount| amount.display.clone())
            .unwrap_or_default(),
        "due_at" => day(todo.due_at),
        "act_by_at" => day(todo.act_by_at),
        "surface_at" => day(todo.surface_at),
        "relevant_until" => day(todo.relevant_until),
        "action_url" => todo
            .action
            .as_ref()
            .and_then(|action| action.domain.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn field_label(field: &str) -> &str {
    match field {
        "due_at" => "due",
        "act_by_at" => "act by",
        "surface_at" => "shows up",
        "relevant_until" => "matters until",
        "action_url" => "button",
        "event_start" => "event",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_split_on_the_first_equals() {
        let edit = parse_edit("title=Pay = council tax").expect("edit");
        assert_eq!(edit.field, "title");
        assert_eq!(edit.value, "Pay = council tax");
        assert!(parse_edit("due fri").is_err());
    }

    fn hostile_row() -> TodoData {
        serde_json::from_value(serde_json::json!({
            "id": "todo_0123456789abcdef",
            "account_id": mxr_core::id::AccountId::new(),
            "kind": "bill",
            "verb": "pay",
            "title": "Pay\u{1b}]0;pwned\u{7} council\u{202e}xat\u{1b}[2J",
            "counterparty": "Eve\u{9b}31m",
            "state": "open",
            "origin": "rule",
            "why": "Here because: \"due\u{2066} 9 October\" (rule).",
            "when_label": "act by Fri 9 Oct",
            "overdue": false,
            "fields": [],
            "user_touched": false,
            "created_at": "2026-10-01T09:00:00Z",
            "updated_at": "2026-10-01T09:00:00Z"
        }))
        .expect("a row")
    }

    #[test]
    fn mail_text_cannot_send_escape_sequences_or_reorder_the_terminal() {
        let mut out = String::new();
        row(&mut out, &hostile_row(), "");
        let printed = terminal_block(&out);
        assert!(
            !printed
                .chars()
                .any(|c| matches!(c as u32, 0x00..=0x09 | 0x0B..=0x1F | 0x7F..=0x9F)),
            "{printed:?}"
        );
        assert!(
            !printed
                .chars()
                .any(|c| matches!(c as u32, 0x202A..=0x202E | 0x2066..=0x2069)),
            "{printed:?}"
        );
        assert!(
            printed.contains("Pay ]0;pwned  councilxat [2J"),
            "{printed:?}"
        );
        assert_eq!(
            printed.lines().count(),
            out.lines().count(),
            "the block keeps its own lines"
        );
    }

    #[test]
    fn short_ids_drop_the_prefix() {
        assert_eq!(short_id("todo_0123456789abcdef"), "01234567");
        assert_eq!(short_id("abc"), "abc");
    }
}
