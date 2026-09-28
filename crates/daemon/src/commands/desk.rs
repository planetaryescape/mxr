use crate::cli::{DeskAction, OutputFormat};
use crate::commands::owed::{csv_escape, truncate};
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use mxr_protocol::*;

pub async fn run(
    account: Option<String>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::GetDesk {
            account_id,
            lane_limit: limit,
        })
        .await?;
    let desk = match resp {
        Response::Ok { data } if matches!(data, ResponseData::Desk { .. }) => data,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };
    print!("{}", render(&desk, resolve_format(format))?);
    Ok(())
}

pub async fn run_action(action: DeskAction) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        DeskAction::Dismiss {
            thread_ids,
            dry_run,
            format,
        } => {
            let resp = client
                .request(Request::DismissDeskThreads {
                    thread_ids: parse_thread_ids(&thread_ids)?,
                    dry_run,
                })
                .await?;
            let (threads, dry_run) = match resp {
                Response::Ok {
                    data: ResponseData::DeskThreadsDismissed { threads, dry_run },
                } => (threads, dry_run),
                Response::Error { message, .. } => anyhow::bail!(message),
                _ => anyhow::bail!("Unexpected response"),
            };
            match resolve_format(format) {
                OutputFormat::Table => {
                    let verb = if dry_run {
                        "Would stop waiting on"
                    } else {
                        "Done waiting on"
                    };
                    println!(
                        "{verb} {}.",
                        plural(threads.len() as u32, "conversation", "conversations")
                    );
                    for thread in &threads {
                        println!("  {}", thread.thread_id);
                    }
                }
                OutputFormat::Ids => {
                    for thread in &threads {
                        println!("{}", thread.thread_id);
                    }
                }
                OutputFormat::Jsonl => {
                    for thread in &threads {
                        println!("{}", serde_json::to_string(thread)?);
                    }
                }
                OutputFormat::Csv => {
                    println!("account_id,thread_id");
                    for thread in &threads {
                        println!("{},{}", thread.account_id, thread.thread_id);
                    }
                }
                OutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "dry_run": dry_run,
                        "threads": threads,
                    }))?
                ),
            }
        }
        DeskAction::Restore { thread_ids } => {
            let resp = client
                .request(Request::RestoreDeskThreads {
                    thread_ids: parse_thread_ids(&thread_ids)?,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::DeskThreadsRestored { restored },
                } => println!(
                    "Waiting again on {}.",
                    plural(restored as u32, "conversation", "conversations")
                ),
                Response::Error { message, .. } => anyhow::bail!(message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        DeskAction::Done {
            thread_ids,
            lane,
            promise,
            dry_run,
            format,
        } => {
            super::desk_done::run(
                &mut client,
                parse_thread_ids(&thread_ids)?,
                lane,
                promise,
                dry_run,
                resolve_format(format),
            )
            .await?;
        }
    }
    Ok(())
}

pub(crate) fn parse_thread_ids(raw: &[String]) -> anyhow::Result<Vec<mxr_core::id::ThreadId>> {
    raw.iter()
        .map(|id| {
            id.parse()
                .map_err(|_| anyhow::anyhow!("not a thread id: {id}"))
        })
        .collect()
}

const LANES: [(DeskLaneKind, &str); 4] = [
    (DeskLaneKind::Owed, "You owe"),
    (DeskLaneKind::Due, "Due"),
    (DeskLaneKind::Waiting, "Waiting on"),
    (DeskLaneKind::PeopleNew, "New from people"),
];

fn lanes(desk: &ResponseData) -> Vec<(DeskLaneKind, &str, &DeskLaneData)> {
    let ResponseData::Desk {
        owed,
        due,
        waiting,
        people_new,
        ..
    } = desk
    else {
        return Vec::new();
    };
    LANES
        .iter()
        .map(|(kind, title)| {
            let lane = match kind {
                DeskLaneKind::Owed => owed,
                DeskLaneKind::Due => due,
                DeskLaneKind::Waiting => waiting,
                DeskLaneKind::PeopleNew => people_new,
            };
            (*kind, *title, lane)
        })
        .collect()
}

fn render(desk: &ResponseData, fmt: OutputFormat) -> anyhow::Result<String> {
    let rows = || {
        lanes(desk)
            .into_iter()
            .flat_map(|(_, _, lane)| lane.rows.iter())
    };
    let mut out = String::new();
    match fmt {
        OutputFormat::Json => {
            out.push_str(&serde_json::to_string_pretty(desk)?);
            out.push('\n');
        }
        OutputFormat::Jsonl => {
            for row in rows() {
                out.push_str(&serde_json::to_string(row)?);
                out.push('\n');
            }
        }
        OutputFormat::Ids => {
            for row in rows() {
                out.push_str(&format!("{}\n", row.thread_id));
            }
        }
        OutputFormat::Csv => {
            out.push_str(
                "lane,thread_id,message_id,counterparty_email,subject,reason,since,age_seconds,usual_seconds,overdue\n",
            );
            for row in rows() {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{}\n",
                    lane_key(row.lane),
                    row.thread_id,
                    row.message_id,
                    csv_escape(&row.counterparty_email),
                    csv_escape(&row.subject),
                    csv_escape(&row.reason),
                    row.since.to_rfc3339(),
                    row.age_seconds,
                    row.usual_seconds.map(|s| s.to_string()).unwrap_or_default(),
                    row.overdue,
                ));
            }
        }
        OutputFormat::Table => out.push_str(&table(desk)),
    }
    Ok(out)
}

fn table(desk: &ResponseData) -> String {
    let mut out = String::new();
    out.push_str(&summary_line(desk));
    out.push('\n');
    for (kind, title, lane) in lanes(desk) {
        if lane.total == 0 {
            continue;
        }
        out.push('\n');
        out.push_str(&format!("{title} {}\n", lane.total));
        for row in &lane.rows {
            let who = row
                .counterparty_name
                .as_deref()
                .unwrap_or(&row.counterparty_email);
            let who = if kind == DeskLaneKind::Due {
                format!("to {who}")
            } else {
                who.to_string()
            };
            let what = if row.subject.is_empty() {
                row.reason.clone()
            } else {
                format!("{} \u{b7} {}", row.subject, row.reason)
            };
            out.push_str(&format!(
                "  {:<22}  {:<58}  {}\n",
                truncate(&terminal_text(&who), 22),
                truncate(&terminal_text(&what), 58),
                age_cell(row)
            ));
        }
        let hidden = lane.total as usize - lane.rows.len();
        if hidden > 0 {
            out.push_str(&format!("  and {hidden} more (--limit to see them)\n"));
        }
    }
    if let ResponseData::Desk { elsewhere, .. } = desk {
        let parts: Vec<String> = [
            ("Reading", elsewhere.reading),
            ("Paper trail", elsewhere.paper_trail),
            ("Deliveries", elsewhere.deliveries),
            ("Invites", elsewhere.invites),
            ("Screener", elsewhere.screener),
        ]
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(label, count)| format!("{label} {count}"))
        .collect();
        if !parts.is_empty() {
            out.push_str(&format!("\nEverything else: {}\n", parts.join(" \u{b7} ")));
        }
    }
    out
}

/// "3 replies owed, 1 promise due." or a calm line when the desk is clear.
fn summary_line(desk: &ResponseData) -> String {
    let counts: Vec<String> = lanes(desk)
        .into_iter()
        .filter(|(_, _, lane)| lane.total > 0)
        .map(|(kind, _, lane)| {
            let n = lane.total;
            match kind {
                DeskLaneKind::Owed => format!("{} owed", plural(n, "reply", "replies")),
                DeskLaneKind::Due => format!("{} due", plural(n, "promise", "promises")),
                DeskLaneKind::Waiting => format!("waiting on {}", plural(n, "thread", "threads")),
                DeskLaneKind::PeopleNew => {
                    format!("{} from people", plural(n, "new message", "new messages"))
                }
            }
        })
        .collect();
    if counts.is_empty() {
        "Nothing needs you right now.".to_string()
    } else {
        let mut line = counts.join(", ");
        if let Some(first) = line.get(0..1) {
            line = first.to_uppercase() + &line[1..];
        }
        line + "."
    }
}

pub(crate) fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub(crate) fn lane_key(lane: DeskLaneKind) -> &'static str {
    match lane {
        DeskLaneKind::Owed => "owed",
        DeskLaneKind::Due => "due",
        DeskLaneKind::Waiting => "waiting",
        DeskLaneKind::PeopleNew => "people_new",
    }
}

/// "2d", "5h", "in 3d" for a promise not yet due, plus the usual pace.
fn age_cell(row: &DeskRowData) -> String {
    let age = if row.age_seconds < 0 {
        format!("in {}", short_duration(-row.age_seconds))
    } else {
        short_duration(row.age_seconds)
    };
    match row.usual_seconds {
        Some(usual) if row.lane != DeskLaneKind::Due => {
            format!("{age} \u{b7} usually {}", short_duration(usual))
        }
        _ => age,
    }
}

fn short_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        s if s < 3_600 => format!("{}m", (s / 60).max(1)),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s if s < 14 * 86_400 => format!("{}d", s / 86_400),
        s => format!("{}w", s / (7 * 86_400)),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests use unwrap for direct fixture failures"
    )]

    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};

    fn row(lane: DeskLaneKind, age: i64, usual: Option<i64>) -> DeskRowData {
        DeskRowData {
            lane,
            account_id: AccountId::new(),
            thread_id: ThreadId::new(),
            message_id: MessageId::new(),
            message_ids: vec![],
            counterparty_email: "maya@example.com".into(),
            counterparty_name: Some("Maya Ortiz".into()),
            subject: "Launch checklist".into(),
            reason: "replied to your message".into(),
            since: chrono::Utc::now(),
            age_seconds: age,
            usual_seconds: usual,
            usual_samples: 3,
            overdue: usual.is_some_and(|u| age > u),
            unread: true,
            starred: false,
            commitment_id: None,
        }
    }

    fn desk(owed: Vec<DeskRowData>, due: Vec<DeskRowData>) -> ResponseData {
        let lane = |rows: Vec<DeskRowData>| DeskLaneData {
            total: rows.len() as u32,
            rows,
        };
        ResponseData::Desk {
            account_id: None,
            owed: lane(owed),
            due: lane(due),
            waiting: DeskLaneData::default(),
            people_new: DeskLaneData::default(),
            elsewhere: DeskElsewhereData {
                reading: 7,
                ..Default::default()
            },
            last_from_people_at: None,
            generated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn table_groups_rows_by_lane_with_pace() {
        let text = render(
            &desk(
                vec![row(DeskLaneKind::Owed, 2 * 86_400, Some(4 * 3_600))],
                vec![row(DeskLaneKind::Due, -2 * 86_400, None)],
            ),
            OutputFormat::Table,
        )
        .unwrap();
        assert!(text.starts_with("1 reply owed, 1 promise due.\n"), "{text}");
        assert!(text.contains("You owe 1\n"));
        assert!(text.contains("2d \u{b7} usually 4h"));
        assert!(text.contains("to Maya Ortiz"));
        assert!(text.contains("in 2d"));
        assert!(text.contains("Everything else: Reading 7"));
        assert!(!text.contains('\u{2014}'), "no em dashes: {text}");
    }

    #[test]
    fn table_strips_terminal_control_sequences_from_mail_text() {
        let mut hostile = row(DeskLaneKind::Owed, 60, None);
        hostile.subject = "Invoice\u{1b}]0;pwned\u{7}\u{1b}[2J\r\nnow".into();
        hostile.counterparty_name = Some("Eve\u{9b}31m".into());
        let text = render(&desk(vec![hostile], vec![]), OutputFormat::Table).unwrap();
        assert!(
            !text
                .chars()
                .any(|c| matches!(c as u32, 0x00..=0x09 | 0x0B..=0x1F | 0x7F..=0x9F)),
            "{text:?}"
        );
        assert!(text.contains("Invoice ]0;pwned  [2J  now"), "{text:?}");
    }

    #[test]
    fn empty_desk_is_calm() {
        let text = render(&desk(vec![], vec![]), OutputFormat::Table).unwrap();
        assert!(text.starts_with("Nothing needs you right now."));
    }

    #[test]
    fn jsonl_emits_one_row_per_line_with_lane() {
        let text = render(
            &desk(vec![row(DeskLaneKind::Owed, 60, None)], vec![]),
            OutputFormat::Jsonl,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(value["lane"], "owed");
    }
}
