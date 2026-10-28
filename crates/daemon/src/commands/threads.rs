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

/// What `--label` resolved to. Without `--account`, a label that is a System
/// label with a mailbox role (Inbox, Sent, ...) also sets `label_role`, so a
/// daemon that knows it lists that mailbox across every account. `label_id`
/// is always the default account's label for that role, which is what a
/// daemon without `label_role` filters by. A user label or a plain folder
/// that happens to be called "Sent" has no role and stays a label.
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
    let named = labels
        .iter()
        .find(|l| l.name == name)
        .or_else(|| labels.iter().find(|l| l.name.eq_ignore_ascii_case(name)));
    if let Some(label) = named {
        let label_role = if account_scoped {
            None
        } else {
            label
                .system_mailbox_role()
                .and_then(mxr_core::Role::system_mailbox_wire_name)
                .map(str::to_string)
        };
        return Ok(LabelSelection {
            label_id: Some(label.id.clone()),
            label_role,
        });
    }
    // Not a label of the default account by name: a canonical mailbox name
    // still means that mailbox, through the default account's label for it.
    if !account_scoped {
        if let Some(role) = mxr_core::Role::from_system_label_name(name) {
            let label = labels
                .iter()
                .find(|l| l.system_mailbox_role() == Some(role))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "the default account has no {} mailbox, so '{name}' cannot be listed \
                         across accounts; name an account with --account",
                        role.system_mailbox_title().unwrap_or(name)
                    )
                })?;
            return Ok(LabelSelection {
                label_id: Some(label.id.clone()),
                label_role: role.system_mailbox_wire_name().map(str::to_string),
            });
        }
    }
    anyhow::bail!("no label named '{name}'")
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
    fn system_label_without_account_selects_role_and_keeps_default_label_id() {
        let inbox = label("INBOX", LabelKind::System);
        let selection = select_label("INBOX", std::slice::from_ref(&inbox), false).unwrap();
        assert_eq!(selection.label_role.as_deref(), Some("inbox"));
        assert_eq!(selection.label_id, Some(inbox.id));
    }

    #[test]
    fn a_lowercase_mailbox_name_still_sends_the_default_accounts_label_id() {
        // `--label inbox` against a Gmail-style `INBOX`: the older-daemon
        // fallback must not be an unfiltered request.
        let inbox = label("INBOX", LabelKind::System);
        let selection = select_label("inbox", std::slice::from_ref(&inbox), false).unwrap();
        assert_eq!(selection.label_role.as_deref(), Some("inbox"));
        assert_eq!(selection.label_id, Some(inbox.id));
        // The default account calls it something else but has the role.
        let mut drafts = label("DRAFT", LabelKind::System);
        drafts.role = Some(mxr_core::Role::Drafts);
        let selection = select_label("drafts", std::slice::from_ref(&drafts), false).unwrap();
        assert_eq!(selection.label_role.as_deref(), Some("drafts"));
        assert_eq!(selection.label_id, Some(drafts.id));
    }

    #[test]
    fn a_mailbox_the_default_account_lacks_is_refused_not_sent_unfiltered() {
        let work = label("Work", LabelKind::User);
        let error = select_label("sent", &[work], false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("no Sent mailbox"), "{error}");
        assert!(error.contains("--account"), "{error}");
    }

    #[test]
    fn a_plain_folder_called_sent_stays_a_folder() {
        // IMAP folder with no special-use flag: kind Folder, no role.
        let folder = label("Sent", LabelKind::Folder);
        let selection = select_label("Sent", std::slice::from_ref(&folder), false).unwrap();
        assert_eq!(selection.label_role, None);
        assert_eq!(selection.label_id, Some(folder.id));
        // A user label named "inbox" is not the Inbox either.
        let user = label("inbox", LabelKind::User);
        let selection = select_label("inbox", std::slice::from_ref(&user), false).unwrap();
        assert_eq!(selection.label_role, None);
    }

    #[test]
    fn account_scoped_label_stays_a_plain_label_id() {
        let inbox = label("INBOX", LabelKind::System);
        let selection = select_label("INBOX", std::slice::from_ref(&inbox), true).unwrap();
        assert_eq!(selection.label_role, None);
        assert_eq!(selection.label_id, Some(inbox.id));
        assert!(select_label("sent", &[], true).is_err());
    }

    #[test]
    fn unknown_label_is_an_error() {
        let work = label("Work", LabelKind::User);
        assert!(select_label("Missing", &[work], false).is_err());
    }
}
