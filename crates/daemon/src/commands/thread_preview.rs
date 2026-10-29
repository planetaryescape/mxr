//! `mxr thread-preview <id>...`: each thread's headline facts with no
//! bodies, from `GetThreadPreviews`.

use crate::cli::OutputFormat;
use crate::commands::desk::parse_thread_ids;
use crate::commands::owed::csv_escape;
use crate::ipc_client::IpcClient;
use crate::output::{print_json, resolve_format, terminal_block, terminal_text};
use chrono::{TimeZone, Utc};
use mxr_protocol::{Request, Response, ResponseData, ThreadPreviewData};
use std::fmt::Write as _;

pub async fn run(thread_ids: Vec<String>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let thread_ids = parse_thread_ids(&thread_ids)?;
    let mut client = IpcClient::connect().await?;
    let previews = match client
        .request(Request::GetThreadPreviews { thread_ids })
        .await?
    {
        Response::Ok {
            data: ResponseData::ThreadPreviews { previews },
        } => previews,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response to GetThreadPreviews"),
    };
    match resolve_format(format) {
        OutputFormat::Json => print_json(&previews, OutputFormat::Json)?,
        OutputFormat::Jsonl => {
            for preview in &previews {
                println!("{}", serde_json::to_string(preview)?);
            }
        }
        OutputFormat::Ids => {
            for preview in &previews {
                println!("{}", preview.thread_id);
            }
        }
        OutputFormat::Csv => print!("{}", csv(&previews)),
        OutputFormat::Table => print!("{}", render(&previews)),
    }
    Ok(())
}

/// One row per thread. Text goes through `terminal_block` before escaping, so
/// a control character from mail cannot reach a terminal that prints the
/// CSV, while newlines stay in the field, quoted as RFC 4180 requires.
fn csv(previews: &[ThreadPreviewData]) -> String {
    let mut out = String::from(
        "thread_id,subject,sender_name,sender_email,latest_at,message_count,unread_count,has_attachments,snippet\n",
    );
    for preview in previews {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{}",
            preview.thread_id,
            csv_escape(&terminal_block(&preview.subject)),
            csv_escape(&terminal_block(
                preview.sender_name.as_deref().unwrap_or_default()
            )),
            csv_escape(&terminal_block(&preview.sender_email)),
            preview.latest_at,
            preview.message_count,
            preview.unread_count,
            preview.has_attachments,
            csv_escape(&terminal_block(&preview.snippet)),
        );
    }
    out
}

fn render(previews: &[ThreadPreviewData]) -> String {
    let mut out = String::new();
    for preview in previews {
        let sender = match &preview.sender_name {
            Some(name) => format!("{} <{}>", name, preview.sender_email),
            None => preview.sender_email.clone(),
        };
        let when = Utc
            .timestamp_opt(preview.latest_at, 0)
            .single()
            .map(|at| at.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let mut marks = Vec::new();
        if preview.unread_count > 0 {
            marks.push(format!("{} unread", preview.unread_count));
        }
        if preview.has_attachments {
            marks.push("attachment".to_string());
        }
        let marks = if marks.is_empty() {
            String::new()
        } else {
            format!(" ({})", marks.join(", "))
        };
        out.push_str(&format!(
            "{}\n  {}\n  {} · {} messages{marks}\n  {}\n",
            terminal_text(&preview.subject),
            terminal_text(&sender),
            when,
            preview.message_count,
            terminal_text(&preview.snippet),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::ThreadId;

    #[test]
    fn the_table_strips_control_characters_from_untrusted_text() {
        let text = render(&[ThreadPreviewData {
            thread_id: ThreadId::new(),
            subject: "Hello\u{1b}[31m red".into(),
            sender_name: Some("Dana".into()),
            sender_email: "dana@example.com".into(),
            latest_at: 1_760_000_000,
            message_count: 2,
            unread_count: 1,
            snippet: "line".into(),
            has_attachments: true,
            account_id: None,
        }]);
        assert!(!text.contains('\u{1b}'));
        assert!(text.contains("Dana <dana@example.com>"));
        assert!(text.contains("2 messages (1 unread, attachment)"));
    }

    #[test]
    fn csv_quotes_commas_quotes_and_newlines_and_keeps_one_row_per_thread() {
        let thread = ThreadId::new();
        let text = csv(&[ThreadPreviewData {
            thread_id: thread.clone(),
            subject: "Q3, \"final\"".into(),
            sender_name: None,
            sender_email: "dana@example.com".into(),
            latest_at: 1_760_000_000,
            message_count: 3,
            unread_count: 0,
            snippet: "line one\nline \u{1b}[31mtwo".into(),
            has_attachments: false,
            account_id: None,
        }]);
        assert_eq!(
            text,
            format!(
                "thread_id,subject,sender_name,sender_email,latest_at,message_count,unread_count,has_attachments,snippet\n\
                 {thread},\"Q3, \"\"final\"\"\",,dana@example.com,1760000000,3,0,false,\"line one\nline  [31mtwo\"\n"
            )
        );
    }
}
