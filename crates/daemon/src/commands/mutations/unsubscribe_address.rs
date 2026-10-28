//! `mxr unsubscribe <address>...`: for each address, one request from the
//! sender's newest message with a usable List-Unsubscribe method.
//!
//! Each address is planned on its own, with its own search, its own plan and
//! at most one request. The daemon plans the request (`Request::PlanUnsubscribe`).
//! The dry run prints those plans and the real run sends them, so the two
//! cannot disagree. Each planned message then goes through `Request::Unsubscribe`,
//! which writes its receipt and enforces the claim.

use super::helpers::{
    confirm_prompt, preview_records, requires_confirmation, resolve_mutation_selection,
    selection_counts, BatchMutationError, MutationPreviewRecord, MutationSelection,
    SelectionCounts,
};
use super::{unsubscribe_method_text, IpcClient, OutputFormat};
use crate::output::resolve_format;
use mxr_core::id::MessageId;
use mxr_core::AccountId;
use mxr_protocol::{
    Request, Response, ResponseData, UnsubscribePlanData, UnsubscribePlanRequestData,
};
use serde::Serialize;

/// One address, planned on its own.
struct AddressPlan {
    address: String,
    selection: MutationSelection,
    counts: SelectionCounts,
    plan: UnsubscribePlanData,
    /// The planned request, shaped for output. `None` when nothing is sent.
    planned: Option<RequestRecord>,
    succeeded: usize,
    errors: Vec<BatchMutationError>,
}

/// The request for one address, with the message it uses.
#[derive(Clone, Serialize)]
struct RequestRecord {
    message_id: String,
    from: String,
    subject: String,
    method: String,
}

/// Per-address result. Dry runs leave out the run fields.
#[derive(Serialize)]
struct AddressOutput {
    address: String,
    requested: usize,
    selected_messages: usize,
    selected_threads: usize,
    message_ids: Vec<String>,
    messages: Vec<MutationPreviewRecord>,
    request_count: usize,
    already_unsubscribed: u32,
    requests: Vec<RequestRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    succeeded: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<BatchMutationError>>,
}

/// Top-level totals across addresses. The fields keep the meaning they have
/// without an address: `requested` and `selected_messages` count matched
/// messages, `message_ids` and `messages` list them, and `succeeded` and
/// `failed` count requests on this path. `addresses` holds each address.
#[derive(Serialize)]
struct TotalsOutput {
    action: &'static str,
    dry_run: bool,
    requested: usize,
    selected_messages: usize,
    selected_threads: usize,
    message_ids: Vec<String>,
    messages: Vec<MutationPreviewRecord>,
    request_count: usize,
    already_unsubscribed: u32,
    requests: Vec<RequestRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    succeeded: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<BatchMutationError>>,
    addresses: Vec<AddressOutput>,
}

/// One line of the dry-run JSONL stream, one per matched message.
#[derive(Serialize)]
struct PreviewLine<'a> {
    action: &'static str,
    dry_run: bool,
    #[serde(flatten)]
    record: &'a MutationPreviewRecord,
}

pub(super) async fn unsubscribe_by_address(
    client: &mut IpcClient,
    addresses: &[String],
    search: Option<&str>,
    account_id: Option<&AccountId>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut plans = plan_addresses(client, addresses, search, account_id).await?;
    if plans.iter().all(|plan| plan.selection.ids.is_empty()) {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        return print_preview(&plans, format);
    }

    let planned = plans.iter().filter(|plan| plan.planned.is_some()).count();
    if planned == 0 && !plans.iter().any(|plan| plan.plan.already_unsubscribed) {
        anyhow::bail!("No matched message has a usable List-Unsubscribe method");
    }
    if planned > 0 && requires_confirmation(true, true, planned, yes) {
        confirm_prompt(&summary_lines(&plans))?;
    }

    for plan in &mut plans {
        let Some(message_id) = plan.plan.request.as_ref().map(|r| r.message_id.clone()) else {
            continue;
        };
        let resp = client
            .request(Request::Unsubscribe {
                message_id: message_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => plan.succeeded += 1,
            Response::Error { message, .. } => {
                eprintln!("Skipped {} ({message})", message_id.as_str());
                plan.errors.push(BatchMutationError {
                    message_id: message_id.as_str(),
                    error: message,
                });
            }
            _ => {
                eprintln!("Skipped {} (unexpected response)", message_id.as_str());
                plan.errors.push(BatchMutationError {
                    message_id: message_id.as_str(),
                    error: "unexpected response".to_owned(),
                });
            }
        }
    }

    print_run(&plans, format)?;
    let succeeded: usize = plans.iter().map(|plan| plan.succeeded).sum();
    if planned > 0 && succeeded == 0 {
        anyhow::bail!("The unsubscribe request was not sent; see the error above");
    }
    Ok(())
}

/// Plans each address with its own search, so each gets its own request.
async fn plan_addresses(
    client: &mut IpcClient,
    addresses: &[String],
    search: Option<&str>,
    account_id: Option<&AccountId>,
) -> anyhow::Result<Vec<AddressPlan>> {
    let mut plans = Vec::with_capacity(addresses.len());
    for address in addresses {
        let selection = resolve_mutation_selection(
            client,
            Vec::new(),
            Some(address_query(address, search)),
            account_id,
        )
        .await?;
        let plan = if selection.ids.is_empty() {
            UnsubscribePlanData {
                matched_messages: 0,
                request: None,
                already_unsubscribed: false,
            }
        } else {
            request_plan(client, &selection).await?
        };
        let planned = plan
            .request
            .as_ref()
            .map(|request| request_record(&selection, request));
        let counts = selection_counts(&selection);
        plans.push(AddressPlan {
            address: address.clone(),
            selection,
            counts,
            plan,
            planned,
            succeeded: 0,
            errors: Vec::new(),
        });
    }
    Ok(plans)
}

/// The search for one address. A `--search` scope narrows it with AND.
fn address_query(address: &str, search: Option<&str>) -> String {
    let from = format!("from:{address}");
    match search.map(str::trim).filter(|scope| !scope.is_empty()) {
        Some(scope) => format!("({scope}) AND ({from})"),
        None => from,
    }
}

async fn request_plan(
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

fn request_record(
    selection: &MutationSelection,
    request: &UnsubscribePlanRequestData,
) -> RequestRecord {
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
}

fn selected_ids(selection: &MutationSelection) -> Vec<String> {
    selection.ids.iter().map(MessageId::as_str).collect()
}

fn summary_lines(plans: &[AddressPlan]) -> Vec<String> {
    let planned = plans.iter().filter(|plan| plan.planned.is_some()).count();
    let matched: usize = plans.iter().map(|plan| plan.selection.ids.len()).sum();
    let mut lines = vec![format!(
        "Would send {planned} unsubscribe request(s) for {matched} matched message(s) across {} address(es)",
        plans.len()
    )];
    for plan in plans {
        lines.extend(address_lines(plan, false));
    }
    lines
}

fn address_lines(plan: &AddressPlan, ran: bool) -> Vec<String> {
    let mut lines = vec![format!(
        "{}: {} matched message(s)",
        plan.address,
        plan.selection.ids.len()
    )];
    match (&plan.planned, plan.plan.already_unsubscribed) {
        _ if plan.selection.ids.is_empty() => lines.push("  no messages matched".to_owned()),
        (Some(request), _) => {
            lines.push(format!(
                "  - {} via {} | {} | {}",
                request.method, request.message_id, request.from, request.subject
            ));
            if ran {
                lines.push(if plan.errors.is_empty() {
                    "  sent".to_owned()
                } else {
                    "  not sent; see the error below".to_owned()
                });
            }
        }
        (None, true) => lines.push(
            "  already unsubscribed: the newest message has a receipt; no request is sent"
                .to_owned(),
        ),
        (None, false) => {
            lines.push("  no usable List-Unsubscribe method; no request is sent".to_owned());
        }
    }
    lines
}

fn address_output(plan: &AddressPlan, run: bool) -> AddressOutput {
    AddressOutput {
        address: plan.address.clone(),
        requested: plan.selection.ids.len(),
        selected_messages: plan.counts.selected_messages,
        selected_threads: plan.counts.selected_threads,
        message_ids: selected_ids(&plan.selection),
        messages: preview_records(&plan.selection, true),
        request_count: usize::from(plan.planned.is_some()),
        already_unsubscribed: u32::from(plan.plan.already_unsubscribed),
        requests: plan.planned.iter().cloned().collect(),
        succeeded: run.then_some(plan.succeeded),
        failed: run.then_some(plan.errors.len()),
        errors: run.then(|| plan.errors.clone()),
    }
}

fn totals(plans: &[AddressPlan], run: bool) -> TotalsOutput {
    let addresses: Vec<AddressOutput> =
        plans.iter().map(|plan| address_output(plan, run)).collect();
    TotalsOutput {
        action: "unsubscribe",
        dry_run: !run,
        requested: addresses.iter().map(|a| a.requested).sum(),
        selected_messages: addresses.iter().map(|a| a.selected_messages).sum(),
        selected_threads: addresses.iter().map(|a| a.selected_threads).sum(),
        message_ids: addresses
            .iter()
            .flat_map(|a| a.message_ids.clone())
            .collect(),
        messages: addresses.iter().flat_map(|a| a.messages.clone()).collect(),
        request_count: addresses.iter().map(|a| a.request_count).sum(),
        already_unsubscribed: addresses.iter().map(|a| a.already_unsubscribed).sum(),
        requests: addresses.iter().flat_map(|a| a.requests.clone()).collect(),
        succeeded: run.then(|| addresses.iter().filter_map(|a| a.succeeded).sum()),
        failed: run.then(|| addresses.iter().filter_map(|a| a.failed).sum()),
        errors: run.then(|| {
            addresses
                .iter()
                .flat_map(|a| a.errors.clone().unwrap_or_default())
                .collect()
        }),
        addresses,
    }
}

fn print_preview(plans: &[AddressPlan], format: Option<OutputFormat>) -> anyhow::Result<()> {
    let output = totals(plans, false);
    match resolve_format(format) {
        OutputFormat::Table => {
            for line in summary_lines(plans) {
                println!("{line}");
            }
        }
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        OutputFormat::Jsonl => {
            for record in &output.messages {
                println!(
                    "{}",
                    serde_json::to_string(&PreviewLine {
                        action: "unsubscribe",
                        dry_run: true,
                        record,
                    })?
                );
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["action", "dry_run", "message_id", "from", "subject"])?;
            for record in &output.messages {
                writer.write_record([
                    "unsubscribe",
                    "true",
                    &record.message_id,
                    &record.from,
                    &record.subject,
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in &output.message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

fn print_run(plans: &[AddressPlan], format: Option<OutputFormat>) -> anyhow::Result<()> {
    let output = totals(plans, true);
    let succeeded = output.succeeded.unwrap_or_default();
    let failed = output.failed.unwrap_or_default();
    match resolve_format(format) {
        OutputFormat::Table => {
            println!(
                "Sent {} unsubscribe request(s) for {} matched message(s): {succeeded} succeeded, {failed} failed",
                output.request_count, output.requested
            );
            for plan in plans {
                for line in address_lines(plan, true) {
                    println!("{line}");
                }
            }
            for error in output.errors.iter().flatten() {
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
                "requested",
                "succeeded",
                "failed",
                "message_id",
                "error",
            ])?;
            let errors = output.errors.clone().unwrap_or_default();
            if errors.is_empty() {
                for id in &output.message_ids {
                    writer.write_record([
                        "unsubscribe",
                        "false",
                        &output.requested.to_string(),
                        &succeeded.to_string(),
                        &failed.to_string(),
                        id,
                        "",
                    ])?;
                }
            } else {
                for error in &errors {
                    writer.write_record([
                        "unsubscribe",
                        "false",
                        &output.requested.to_string(),
                        &succeeded.to_string(),
                        &failed.to_string(),
                        &error.message_id,
                        &error.error,
                    ])?;
                }
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in &output.message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::mutations::helpers::MutationSelection;
    use mxr_core::types::UnsubscribeMethod;

    #[test]
    fn a_dry_run_keeps_the_matched_messages_next_to_the_requests() {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .unsubscribe(UnsubscribeMethod::OneClick {
                url: "https://promo.example/unsubscribe/7".into(),
            })
            .build();
        let id = envelope.id.clone();
        let selection = MutationSelection {
            ids: vec![id.clone()],
            envelopes: vec![envelope],
            used_search: true,
        };
        let request = UnsubscribePlanRequestData {
            message_id: id,
            method: UnsubscribeMethod::OneClick {
                url: "https://promo.example/unsubscribe/7".into(),
            },
        };
        let planned = request_record(&selection, &request);
        let plan = AddressPlan {
            address: "alice@example.com".into(),
            counts: selection_counts(&selection),
            selection,
            plan: UnsubscribePlanData {
                matched_messages: 1,
                request: Some(request),
                already_unsubscribed: false,
            },
            planned: Some(planned),
            succeeded: 0,
            errors: Vec::new(),
        };

        let json = serde_json::to_value(totals(&[plan], false)).unwrap();
        assert_eq!(json["dry_run"], true);
        assert_eq!(json["requested"], 1);
        assert_eq!(json["request_count"], 1);
        assert_eq!(json["messages"].as_array().unwrap().len(), 1);
        assert_eq!(json["messages"][0]["message_id"], json["message_ids"][0]);
        assert_eq!(json["requests"][0]["method"], "one-click");
        assert_eq!(json["addresses"][0]["address"], "alice@example.com");
        assert_eq!(
            json["addresses"][0]["messages"].as_array().unwrap().len(),
            1
        );
        assert!(json.get("succeeded").is_none(), "a dry run sends nothing");
    }

    #[test]
    fn an_address_query_is_scoped_by_search_with_and() {
        assert_eq!(
            address_query("alice@example.com", None),
            "from:alice@example.com"
        );
        assert_eq!(
            address_query("alice@example.com", Some("label:news")),
            "(label:news) AND (from:alice@example.com)"
        );
        assert_eq!(
            address_query("alice@example.com", Some("  ")),
            "from:alice@example.com"
        );
    }
}
