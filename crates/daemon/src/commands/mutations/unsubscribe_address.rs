//! `mxr unsubscribe <address>`: one request for the sender, from the newest
//! message with a usable List-Unsubscribe method.
//!
//! The daemon plans the request (`Request::PlanUnsubscribe`). The dry run
//! prints that plan and the real run sends it, so they cannot disagree. The
//! planned message then goes through `Request::Unsubscribe`, which writes its
//! receipt and enforces the claim.

use super::helpers::{
    confirm_prompt, requires_confirmation, selection_counts, BatchMutationError, MutationSelection,
    SelectionCounts,
};
use super::{unsubscribe_method_text, IpcClient, OutputFormat};
use crate::output::resolve_format;
use mxr_core::id::MessageId;
use mxr_protocol::{Request, Response, ResponseData, UnsubscribePlanData};
use serde::Serialize;

async fn plan_requests(
    client: &mut IpcClient,
    selection: &MutationSelection,
) -> anyhow::Result<UnsubscribePlanData> {
    let resp = client
        .request(Request::PlanUnsubscribe {
            message_ids: selection.ids.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::UnsubscribePlan { plan },
        } => Ok(plan),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from unsubscribe plan"),
    }
}

pub(super) async fn unsubscribe_by_address(
    client: &mut IpcClient,
    selection: &MutationSelection,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let plan = plan_requests(client, selection).await?;
    let requests = request_records(selection, &plan);
    let counts = selection_counts(selection);

    if dry_run {
        return print_preview(selection, &plan, &requests, counts, format);
    }

    if plan.request.is_none() && !plan.already_unsubscribed {
        anyhow::bail!("No matched message has a usable List-Unsubscribe method");
    }

    if requires_confirmation(true, selection.used_search, requests.len(), yes) {
        confirm_prompt(&summary_lines(selection, &plan, &requests, counts))?;
    }

    let mut succeeded = 0usize;
    let mut errors = Vec::new();
    for request in plan.request.iter() {
        let id = &request.message_id;
        let resp = client
            .request(Request::Unsubscribe {
                message_id: id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => succeeded += 1,
            Response::Error { message, .. } => {
                eprintln!("Skipped {} ({message})", id.as_str());
                errors.push(BatchMutationError {
                    message_id: id.as_str(),
                    error: message,
                });
            }
            _ => {
                eprintln!("Skipped {} (unexpected response)", id.as_str());
                errors.push(BatchMutationError {
                    message_id: id.as_str(),
                    error: "unexpected response".to_owned(),
                });
            }
        }
    }

    print_run(
        selection, &plan, &requests, counts, succeeded, errors, format,
    )?;
    if plan.request.is_some() && succeeded == 0 {
        anyhow::bail!("The unsubscribe request was not sent; see the error above");
    }
    Ok(())
}

/// One request the real run sends, with the message it uses.
#[derive(Clone, Serialize)]
struct RequestRecord {
    message_id: String,
    from: String,
    subject: String,
    method: String,
}

fn request_records(
    selection: &MutationSelection,
    plan: &UnsubscribePlanData,
) -> Vec<RequestRecord> {
    plan.request
        .iter()
        .map(|request| {
            let envelope = selection
                .envelopes
                .iter()
                .find(|envelope| envelope.id == request.message_id);
            RequestRecord {
                message_id: request.message_id.as_str(),
                from: envelope
                    .map(|envelope| {
                        envelope
                            .from
                            .name
                            .as_deref()
                            .unwrap_or(&envelope.from.email)
                            .to_owned()
                    })
                    .unwrap_or_default(),
                subject: envelope
                    .map(|envelope| envelope.subject.clone())
                    .unwrap_or_default(),
                method: unsubscribe_method_text(&request.method),
            }
        })
        .collect()
}

fn selected_ids(selection: &MutationSelection) -> Vec<String> {
    selection.ids.iter().map(MessageId::as_str).collect()
}

fn summary_lines(
    selection: &MutationSelection,
    plan: &UnsubscribePlanData,
    requests: &[RequestRecord],
    counts: SelectionCounts,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Would send {} unsubscribe request(s) for {} message(s)",
        requests.len(),
        selection.ids.len()
    )];
    lines.push(counts.selection_line());
    lines.extend(request_lines(requests));
    if plan.already_unsubscribed {
        lines.push(
            "The newest message from this sender already has a receipt; no request is sent."
                .to_owned(),
        );
    } else if plan.request.is_none() {
        lines.push("No matched message has a usable List-Unsubscribe method.".to_owned());
    }
    lines
}

fn request_lines(requests: &[RequestRecord]) -> Vec<String> {
    requests
        .iter()
        .map(|request| {
            format!(
                "- {} via {} | {} | {}",
                request.method, request.message_id, request.from, request.subject
            )
        })
        .collect()
}

#[derive(Serialize)]
struct PreviewOutput {
    action: &'static str,
    dry_run: bool,
    /// Matched messages. Same meaning as the address-less dry run.
    requested: usize,
    selected_messages: usize,
    selected_threads: usize,
    request_count: usize,
    already_unsubscribed: u32,
    message_ids: Vec<String>,
    requests: Vec<RequestRecord>,
}

#[derive(Serialize)]
struct RunOutput {
    action: &'static str,
    dry_run: bool,
    /// Matched messages. Same meaning as the address-less run.
    requested: usize,
    selected_messages: usize,
    selected_threads: usize,
    /// Requests sent (0 or 1). `succeeded` and `failed` count these, not messages.
    request_count: usize,
    already_unsubscribed: u32,
    succeeded: usize,
    failed: usize,
    message_ids: Vec<String>,
    requests: Vec<RequestRecord>,
    errors: Vec<BatchMutationError>,
}

fn print_preview(
    selection: &MutationSelection,
    plan: &UnsubscribePlanData,
    requests: &[RequestRecord],
    counts: SelectionCounts,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => {
            for line in summary_lines(selection, plan, requests, counts) {
                println!("{line}");
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&preview_output(selection, plan, requests, counts))?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&preview_output(selection, plan, requests, counts))?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "action",
                "dry_run",
                "message_id",
                "from",
                "subject",
                "method",
            ])?;
            for request in requests {
                writer.write_record([
                    "unsubscribe",
                    "true",
                    &request.message_id,
                    &request.from,
                    &request.subject,
                    &request.method,
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for request in requests {
                println!("{}", request.message_id);
            }
        }
    }
    Ok(())
}

fn preview_output(
    selection: &MutationSelection,
    plan: &UnsubscribePlanData,
    requests: &[RequestRecord],
    counts: SelectionCounts,
) -> PreviewOutput {
    PreviewOutput {
        action: "unsubscribe",
        dry_run: true,
        requested: selection.ids.len(),
        selected_messages: counts.selected_messages,
        selected_threads: counts.selected_threads,
        request_count: requests.len(),
        already_unsubscribed: u32::from(plan.already_unsubscribed),
        message_ids: selected_ids(selection),
        requests: requests.to_vec(),
    }
}

fn print_run(
    selection: &MutationSelection,
    plan: &UnsubscribePlanData,
    requests: &[RequestRecord],
    counts: SelectionCounts,
    succeeded: usize,
    errors: Vec<BatchMutationError>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let output = RunOutput {
        action: "unsubscribe",
        dry_run: false,
        requested: selection.ids.len(),
        selected_messages: counts.selected_messages,
        selected_threads: counts.selected_threads,
        request_count: requests.len(),
        already_unsubscribed: u32::from(plan.already_unsubscribed),
        succeeded,
        failed: errors.len(),
        message_ids: selected_ids(selection),
        requests: requests.to_vec(),
        errors,
    };
    match resolve_format(format) {
        OutputFormat::Table => {
            println!(
                "Sent {} unsubscribe request(s) for {} message(s); {} succeeded, {} failed",
                output.request_count, output.requested, output.succeeded, output.failed
            );
            println!("{}", counts.selection_line());
            for error in &output.errors {
                println!("Failed {} ({})", error.message_id, error.error);
            }
        }
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&output)?),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "action",
                "dry_run",
                "request_count",
                "succeeded",
                "failed",
                "message_id",
                "error",
            ])?;
            if output.errors.is_empty() {
                for request in &output.requests {
                    writer.write_record([
                        "unsubscribe",
                        "false",
                        &output.request_count.to_string(),
                        &output.succeeded.to_string(),
                        "0",
                        &request.message_id,
                        "",
                    ])?;
                }
            } else {
                for error in &output.errors {
                    writer.write_record([
                        "unsubscribe",
                        "false",
                        &output.request_count.to_string(),
                        &output.succeeded.to_string(),
                        &output.failed.to_string(),
                        &error.message_id,
                        &error.error,
                    ])?;
                }
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for request in &output.requests {
                println!("{}", request.message_id);
            }
        }
    }
    Ok(())
}
