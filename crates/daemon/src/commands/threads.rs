use crate::cli::{OutputFormat, ThreadsSort};
use crate::commands::account_mark::AccountMarks;
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::{AccountId, LabelId};
use mxr_protocol::*;

pub async fn run(
    account: Option<String>,
    label: Option<String>,
    limit: u32,
    offset: u32,
    sort: Option<ThreadsSort>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let scoped = account.is_some();

    let account_id = match account {
        Some(s) => Some(
            s.parse::<AccountId>()
                .map_err(|e| anyhow::anyhow!("invalid --account: {e}"))?,
        ),
        None => None,
    };

    // Resolve --label name → LabelId via the ListLabels endpoint so the
    // CLI accepts the human-readable label name (matching `mxr labels`).
    let selection = match label {
        Some(name) => {
            let resp = client
                .request(Request::ListLabels {
                    account_id: account_id.clone(),
                    all_accounts: false,
                })
                .await?;
            let labels = expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::Labels { labels },
                } => Some(labels),
                _ => None,
            })?;
            select_label(&name, &labels, account_id.is_some())?
        }
        None => LabelSelection::default(),
    };

    let threads = fetch_threads(
        &mut client,
        account_id,
        selection,
        limit,
        offset,
        sort.map(mxr_core::types::SortOrder::from),
    )
    .await?;

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&threads)?);
        }
        OutputFormat::Jsonl => {
            for thread in &threads {
                println!("{}", serde_json::to_string(thread)?);
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["thread_id", "subject", "message_count", "latest_date"])?;
            for thread in &threads {
                writer.write_record(&[
                    thread.id.as_str().clone(),
                    thread.subject.clone(),
                    thread.message_count.to_string(),
                    thread.latest_date.to_rfc3339(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for thread in &threads {
                println!("{}", thread.id);
            }
        }
        OutputFormat::Table => {
            if threads.is_empty() {
                println!("(no threads)");
            }
            let marks = AccountMarks::load(&mut client, scoped).await?;
            for thread in &threads {
                let acct = if marks.shown() {
                    format!("{:<4} ", marks.cell(&thread.account_id))
                } else {
                    String::new()
                };
                println!(
                    "{acct}{}  {} msgs ({} unread)  {}",
                    thread.latest_date.format("%Y-%m-%d %H:%M"),
                    thread.message_count,
                    thread.unread_count,
                    thread.subject,
                );
            }
        }
    }
    Ok(())
}

/// What `--label` resolved to. Without `--account`, a system mailbox name
/// (INBOX, SENT, ...) also sets `label_role`, so a daemon that knows it lists
/// that mailbox across every account; `label_id` stays the default account's
/// label for a daemon that does not.
#[derive(Debug, Default, PartialEq, Eq)]
struct LabelSelection {
    label_id: Option<LabelId>,
    label_role: Option<String>,
}

fn select_label(
    name: &str,
    labels: &[mxr_core::Label],
    account_scoped: bool,
) -> anyhow::Result<LabelSelection> {
    let label_id = labels.iter().find(|l| l.name == name).map(|l| l.id.clone());
    let label_role = if account_scoped {
        None
    } else {
        mxr_core::Role::from_system_label_name(name)
            .and_then(mxr_core::Role::system_mailbox_wire_name)
            .map(str::to_string)
    };
    if label_id.is_none() && label_role.is_none() {
        anyhow::bail!("no label named '{name}'");
    }
    Ok(LabelSelection {
        label_id,
        label_role,
    })
}

async fn fetch_threads(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    selection: LabelSelection,
    limit: u32,
    offset: u32,
    sort: Option<mxr_core::types::SortOrder>,
) -> anyhow::Result<Vec<mxr_core::Thread>> {
    let resp = client
        .request(Request::ListThreads {
            account_id,
            label_id: selection.label_id,
            limit,
            offset,
            sort,
            label_role: selection.label_role,
        })
        .await?;
    expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Threads { threads },
        } => Some(threads),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests unwrap fixture values")]
    use super::*;
    use mxr_core::{AccountId, Label, LabelKind};

    fn label(name: &str, kind: LabelKind) -> Label {
        Label {
            id: LabelId::new(),
            account_id: AccountId::new(),
            name: name.to_string(),
            kind,
            color: None,
            provider_id: name.to_string(),
            unread_count: 0,
            total_count: 0,
            role: None,
        }
    }

    #[test]
    fn system_name_without_account_selects_role_and_keeps_default_label_id() {
        let inbox = label("INBOX", LabelKind::System);
        let selection = select_label("INBOX", std::slice::from_ref(&inbox), false).unwrap();
        assert_eq!(selection.label_role.as_deref(), Some("inbox"));
        assert_eq!(selection.label_id, Some(inbox.id));
    }

    #[test]
    fn system_name_is_a_role_even_when_the_default_account_lacks_it() {
        let selection = select_label("SENT", &[], false).unwrap();
        assert_eq!(selection.label_role.as_deref(), Some("sent"));
        assert_eq!(selection.label_id, None);
    }

    #[test]
    fn account_scoped_label_stays_a_plain_label_id() {
        let inbox = label("INBOX", LabelKind::System);
        let selection = select_label("INBOX", std::slice::from_ref(&inbox), true).unwrap();
        assert_eq!(selection.label_role, None);
        assert_eq!(selection.label_id, Some(inbox.id));
    }

    #[test]
    fn user_label_never_becomes_a_role() {
        let work = label("Work", LabelKind::User);
        let selection = select_label("Work", std::slice::from_ref(&work), false).unwrap();
        assert_eq!(selection.label_role, None);
        assert!(select_label("Missing", &[work], false).is_err());
    }
}
