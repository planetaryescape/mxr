use crate::ipc_client::IpcClient;
use chrono::Utc;
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, SortOrder};
use mxr_protocol::*;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

pub(super) fn parse_message_id(id_str: &str) -> anyhow::Result<MessageId> {
    let uuid = uuid::Uuid::parse_str(id_str)
        .map_err(|e| anyhow::anyhow!("Invalid message ID '{id_str}': {e}"))?;
    Ok(MessageId::from_uuid(uuid))
}

pub(super) async fn resolve_message_ids(
    client: &mut IpcClient,
    message_id: Option<String>,
    search: Option<String>,
) -> anyhow::Result<Vec<MessageId>> {
    match (message_id, search) {
        (Some(id), _) => Ok(vec![parse_message_id(&id)?]),
        (None, Some(query)) => {
            let resp = client
                .request(Request::Search {
                    query,
                    limit: 1000,
                    offset: 0,
                    mode: None,
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::SearchResults { results, .. },
                } => Ok(results.into_iter().map(|r| r.message_id).collect()),
                Response::Error { message } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response from search"),
            }
        }
        (None, None) => anyhow::bail!("Provide a message ID or --search query"),
    }
}

pub(super) struct MutationSelection {
    pub(super) ids: Vec<MessageId>,
    pub(super) envelopes: Vec<Envelope>,
    pub(super) used_search: bool,
}

pub(super) async fn resolve_mutation_selection(
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
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response from envelope lookup"),
        }
    };

    Ok(MutationSelection {
        ids,
        envelopes,
        used_search,
    })
}

pub(super) fn requires_confirmation(
    destructive: bool,
    used_search: bool,
    matched_count: usize,
    yes: bool,
) -> bool {
    !yes && (destructive || used_search || matched_count > 1)
}

pub(super) fn render_selection_preview_lines(
    action: &str,
    selection: &MutationSelection,
) -> Vec<String> {
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

pub(super) fn print_selection_preview(action: &str, selection: &MutationSelection) {
    for line in render_selection_preview_lines(action, selection) {
        println!("{line}");
    }
}

pub(super) fn confirm_action(action: &str, selection: &MutationSelection) -> anyhow::Result<()> {
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

pub(super) async fn run_simple_mutation<F>(
    client: &mut IpcClient,
    selection: MutationSelection,
    options: MutationRunOptions<'_>,
    build_request: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Vec<MessageId>) -> Request,
{
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    if options.dry_run {
        print_selection_preview(options.action, &selection);
        return Ok(());
    }

    if requires_confirmation(
        options.destructive,
        selection.used_search,
        selection.ids.len(),
        options.yes,
    ) {
        confirm_action(options.action, &selection)?;
    }

    let resp = client.request(build_request(selection.ids)).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => println!("{}", options.success_message),
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub(super) struct MutationRunOptions<'a> {
    pub(super) action: &'a str,
    pub(super) success_message: &'a str,
    pub(super) yes: bool,
    pub(super) dry_run: bool,
    pub(super) destructive: bool,
}

pub(super) fn parse_snooze_until(until: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    let config = mxr_config::load_config().unwrap_or_default().snooze;
    mxr_config::snooze::parse_snooze_until(until, &config).ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot parse '{until}'. Use: tomorrow, tonight, monday, weekend, or ISO 8601"
        )
    })
}

pub(super) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(super) async fn load_attachments(
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
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

pub(super) fn attachment_by_index(
    attachments: &[mxr_core::AttachmentMeta],
    index: usize,
) -> anyhow::Result<&mxr_core::AttachmentMeta> {
    attachments
        .get(index.saturating_sub(1))
        .ok_or_else(|| anyhow::anyhow!("Attachment index {index} out of range"))
}

pub(super) async fn request_attachment_file(
    client: &mut IpcClient,
    request: Request,
) -> anyhow::Result<PathBuf> {
    let resp = client.request(request).await?;
    match resp {
        Response::Ok {
            data: ResponseData::AttachmentFile { file },
        } => Ok(PathBuf::from(file.path)),
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}
