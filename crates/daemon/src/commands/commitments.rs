use crate::cli::{CommitmentStatusArg, CommitmentsAction, OutputFormat};
use crate::commands::daemon_response_error;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use chrono::{Local, TimeZone};
use mxr_protocol::*;

pub async fn run(
    action: Option<CommitmentsAction>,
    contact: Option<String>,
    status: Option<CommitmentStatusArg>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        Some(CommitmentsAction::Resolve { id }) => {
            let resp = client
                .request(Request::ResolveCommitment { commitment_id: id })
                .await?;
            match resp {
                Response::Ok { .. } => println!("resolved"),
                Response::Error { message, kind, .. } => {
                    return Err(daemon_response_error(message, kind))
                }
            }
        }
        Some(CommitmentsAction::Add {
            message_id,
            what,
            due,
            dry_run,
            format,
        }) => {
            super::promises::add(&mut client, &message_id, what, &due, dry_run, format).await?;
        }
        Some(CommitmentsAction::Detect { message_id, format }) => {
            super::promises::detect(&mut client, &message_id, format).await?;
        }
        None => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::ListCommitments {
                    account_id,
                    email: contact,
                    status: status.map(status_data),
                })
                .await?;
            print_list(resp, resolve_format(format))?;
        }
    }
    Ok(())
}

fn print_list(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::CommitmentList { commitments },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&commitments)?),
            OutputFormat::Jsonl => {
                for commitment in commitments {
                    println!("{}", serde_json::to_string(&commitment)?);
                }
            }
            OutputFormat::Ids => {
                for commitment in commitments {
                    println!("{}", commitment.id);
                }
            }
            _ => {
                for commitment in commitments {
                    println!(
                        "{}\t{}\t{:?}\t{}{}",
                        commitment.id,
                        commitment.email,
                        commitment.direction,
                        commitment.what,
                        due_column(&commitment, &Local)
                    );
                }
            }
        },
        Response::Error { message, kind, .. } => return Err(daemon_response_error(message, kind)),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

/// A fifth tab-separated column, only on dated rows, so the four existing
/// columns keep their places for scripts. The day is the deadline's day in
/// the reader's zone, not UTC's.
fn due_column<Tz: TimeZone>(commitment: &CommitmentData, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let Some(when) = commitment.by_when else {
        return String::new();
    };
    let day = when.with_timezone(tz).format("%Y-%m-%d");
    if commitment.date_unverified {
        format!("\tdue {day} (date unverified)")
    } else {
        format!("\tdue {day}")
    }
}

fn status_data(value: CommitmentStatusArg) -> CommitmentStatusData {
    match value {
        CommitmentStatusArg::Open => CommitmentStatusData::Open,
        CommitmentStatusArg::Resolved => CommitmentStatusData::Resolved,
        CommitmentStatusArg::Expired => CommitmentStatusData::Expired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone, Utc};
    use mxr_core::id::{AccountId, MessageId, ThreadId};

    fn commitment(by_when: bool, date_unverified: bool) -> CommitmentData {
        CommitmentData {
            id: "c-1".into(),
            account_id: AccountId::new(),
            email: "nora@example.com".into(),
            thread_id: ThreadId::new(),
            direction: CommitmentDirectionData::Yours,
            status: CommitmentStatusData::Open,
            who_owes: "you".into(),
            what: "send the deck".into(),
            by_when: by_when.then(|| Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap()),
            date_unverified,
            evidence_msg_id: MessageId::new(),
            extracted_at: Utc.with_ymd_and_hms(2026, 9, 20, 8, 0, 0).unwrap(),
        }
    }

    #[test]
    fn json_output_says_whether_the_date_is_unverified() {
        let unverified = serde_json::to_value(commitment(true, true)).unwrap();
        assert_eq!(unverified["date_unverified"], true);
        assert_eq!(unverified["by_when"], "2026-10-02T08:00:00Z");
        let verified = serde_json::to_value(commitment(true, false)).unwrap();
        assert_eq!(verified["date_unverified"], false);
    }

    #[test]
    fn an_older_daemons_commitment_without_the_field_reads_as_verified() {
        let mut value = serde_json::to_value(commitment(true, true)).unwrap();
        value.as_object_mut().unwrap().remove("date_unverified");
        let parsed: CommitmentData = serde_json::from_value(value).unwrap();
        assert!(!parsed.date_unverified);
    }

    #[test]
    fn text_output_appends_a_due_column_only_on_dated_rows() {
        let utc = FixedOffset::east_opt(0).unwrap();
        assert_eq!(due_column(&commitment(false, false), &utc), "");
        assert_eq!(
            due_column(&commitment(true, false), &utc),
            "\tdue 2026-10-02"
        );
        assert_eq!(
            due_column(&commitment(true, true), &utc),
            "\tdue 2026-10-02 (date unverified)"
        );
    }

    #[test]
    fn the_due_day_is_the_day_in_the_readers_zone() {
        // 00:30 on 23 October in London (BST), which is still the 22nd in UTC.
        let london = FixedOffset::east_opt(3600).unwrap();
        let mut late = commitment(true, false);
        late.by_when = Some(Utc.with_ymd_and_hms(2026, 10, 22, 23, 30, 0).unwrap());
        assert_eq!(due_column(&late, &london), "\tdue 2026-10-23");
        late.date_unverified = true;
        assert_eq!(
            due_column(&late, &london),
            "\tdue 2026-10-23 (date unverified)"
        );
    }
}
