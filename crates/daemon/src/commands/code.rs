//! `mxr code`: the one-time code from the mail you just asked for, ready to
//! paste. The daemon finds codes by rules, the same ones Updates uses.

use crate::cli::OutputFormat;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use chrono::{DateTime, Local, Utc};
use mxr_protocol::{OneTimeCodeData, Request, Response, ResponseData};
use std::fmt::Write as _;

/// Codes `--all` lists.
const LISTED: u32 = 20;

pub async fn run(
    account: Option<String>,
    all: bool,
    copy: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let codes = expect_response(
        client
            .request(Request::ListOneTimeCodes {
                account_id,
                limit: Some(LISTED),
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::OneTimeCodes { codes },
            } => Some(codes),
            _ => None,
        },
    )?;
    let format = resolve_format(format);
    if all {
        print!("{}", render_all(&codes, format)?);
        return Ok(());
    }
    let Some(latest) = codes.iter().find(|code| code.current) else {
        let last = codes
            .first()
            .map(|code| {
                format!(
                    " The last one, from {}, expired {}.",
                    who(code),
                    clock(code.expires_at)
                )
            })
            .unwrap_or_default();
        anyhow::bail!("No current one-time code in your mail.{last}");
    };
    if copy {
        let code = latest.code.clone();
        tokio::task::spawn_blocking(move || mxr_tui::copy_to_clipboard(&code))
            .await?
            .map_err(|error| anyhow::anyhow!("Could not copy the code: {error}"))?;
        // Said on stderr, so stdout stays the code alone for scripts.
        eprintln!(
            "Copied the code from {}, expires {}.",
            who(latest),
            clock(latest.expires_at)
        );
    }
    print!("{}", render_one(latest, format)?);
    Ok(())
}

fn render_one(code: &OneTimeCodeData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(code)?),
        OutputFormat::Jsonl => format!("{}\n", serde_json::to_string(code)?),
        // The code alone: `mxr code --format ids | pbcopy`.
        OutputFormat::Ids => format!("{}\n", terminal_text(&code.code)),
        OutputFormat::Csv | OutputFormat::Table => format!("{}\n", line(code)),
    })
}

fn render_all(codes: &[OneTimeCodeData], format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(codes)?),
        OutputFormat::Jsonl => codes.iter().try_fold(String::new(), |mut out, code| {
            let _ = writeln!(out, "{}", serde_json::to_string(code)?);
            Ok::<_, serde_json::Error>(out)
        })?,
        OutputFormat::Ids => codes.iter().fold(String::new(), |mut out, code| {
            let _ = writeln!(out, "{}", terminal_text(&code.code));
            out
        }),
        OutputFormat::Csv => codes.iter().fold(
            "code,current,sender,host,received_at,expires_at\n".to_string(),
            |mut out, code| {
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{}",
                    csv(&code.code),
                    code.current,
                    csv(&code.sender_name),
                    csv(&code.sender_host),
                    code.received_at.to_rfc3339(),
                    code.expires_at.to_rfc3339()
                );
                out
            },
        ),
        OutputFormat::Table if codes.is_empty() => {
            "No one-time codes in the last day.\n".to_string()
        }
        OutputFormat::Table => codes.iter().fold(String::new(), |mut out, code| {
            let _ = writeln!(out, "{}", line(code));
            out
        }),
    })
}

/// "482913  Acme · arrived 14:32 · expires 14:42".
fn line(code: &OneTimeCodeData) -> String {
    let state = if code.current { "expires" } else { "expired" };
    format!(
        "{}  {} · arrived {} · {state} {}",
        terminal_text(&code.code),
        who(code),
        clock(code.received_at),
        clock(code.expires_at)
    )
}

/// "Google (accounts.google.com)": the name the sender chose beside the host
/// the mail came from, so a borrowed brand shows before a code is pasted.
fn who(code: &OneTimeCodeData) -> String {
    terminal_text(&mxr_updates::fact::branded_name(
        &code.sender_name,
        &code.sender_host,
    ))
    .into_owned()
}

/// "14:42" today, "Mon 14:42" otherwise.
fn clock(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&Local);
    if local.date_naive() == Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%a %H:%M").to_string()
    }
}

fn csv(value: &str) -> String {
    crate::commands::owed::csv_escape(&terminal_text(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};

    fn code(current: bool) -> OneTimeCodeData {
        let at = Utc::now();
        OneTimeCodeData {
            code: "482913".into(),
            current,
            expires_at: at,
            received_at: at,
            sender_name: "Acme\u{1b}[2J".into(),
            sender_host: "attacker.example".into(),
            sender_email: "no-reply@acme.example".into(),
            subject: "Your code".into(),
            account_id: AccountId::new(),
            message_id: MessageId::new(),
            thread_id: ThreadId::new(),
        }
    }

    #[test]
    fn ids_print_the_code_alone_for_pasting() {
        assert_eq!(
            render_one(&code(true), OutputFormat::Ids).unwrap(),
            "482913\n"
        );
    }

    #[test]
    fn the_line_names_the_sender_without_its_control_codes() {
        let line = render_one(&code(true), OutputFormat::Table).unwrap();
        assert!(line.starts_with("482913  Acme"), "{line}");
        assert!(line.contains("attacker.example"), "the host shows: {line}");
        assert!(!line.contains('\u{1b}'), "{line:?}");
        assert!(render_all(&[code(false)], OutputFormat::Table)
            .unwrap()
            .contains("expired"));
    }
}
