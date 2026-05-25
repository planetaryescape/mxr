use crate::cli::OutputFormat;
use crate::commands::expect_response;
use crate::commands::selection::parse_message_id;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{
    CalendarInviteActionData, CalendarInviteData, CalendarInviteResponsePreview, Request, Response,
    ResponseData,
};

pub async fn show(message_id: String, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let message_id = parse_message_id(&message_id)?;
    let resp = client.request(Request::GetInvite { message_id }).await?;
    let invite = expect_response(resp, |response| match response {
        Response::Ok {
            data: ResponseData::Invite { invite },
        } => Some(invite),
        _ => None,
    })?;

    print_invites(&[invite], resolve_format(format))
}

pub async fn list(limit: u32, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListInvites { limit }).await?;
    let invites = expect_response(resp, |response| match response {
        Response::Ok {
            data: ResponseData::Invites { invites },
        } => Some(invites),
        _ => None,
    })?;

    print_invites(&invites, resolve_format(format))
}

pub async fn backfill(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::BackfillCalendarInvites).await?;
    let (backfilled, rehydrated) = expect_response(resp, |response| match response {
        Response::Ok {
            data:
                ResponseData::CalendarInviteBackfill {
                    backfilled,
                    rehydrated,
                },
        } => Some((backfilled, rehydrated)),
        _ => None,
    })?;

    let payload = serde_json::json!({ "backfilled": backfilled, "rehydrated": rehydrated });
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&payload)?),
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&payload)?);
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["backfilled", "rehydrated"])?;
            writer.write_record([backfilled.to_string(), rehydrated.to_string()])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => println!("{backfilled}"),
        OutputFormat::Table => println!(
            "Re-hydrated {rehydrated} attachment-only invite(s); rebuilt {backfilled} calendar-invite row(s)"
        ),
    }
    Ok(())
}

pub async fn reply(
    message_id: String,
    action: CalendarInviteActionData,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let message_id = parse_message_id(&message_id)?;
    let resp = client
        .request(Request::RespondInvite {
            message_id,
            action,
            dry_run,
        })
        .await?;

    match resp {
        Response::Ok {
            data: ResponseData::InviteResponsePreview { preview },
        } => print_preview(&preview, resolve_format(format)),
        Response::Ok {
            data: ResponseData::InviteResponseSent { result },
        } => match resolve_format(format) {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            OutputFormat::Jsonl => {
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            OutputFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());
                writer.write_record([
                    "message_id",
                    "action",
                    "provider_message_id",
                    "rfc2822_message_id",
                ])?;
                writer.write_record([
                    result.message_id.as_str(),
                    format!("{:?}", result.action),
                    result.provider_message_id.unwrap_or_default(),
                    result.rfc2822_message_id,
                ])?;
                println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
                Ok(())
            }
            OutputFormat::Ids => {
                println!("{}", result.rfc2822_message_id);
                Ok(())
            }
            OutputFormat::Table => {
                println!(
                    "{} calendar reply sent ({})",
                    result.action.label(),
                    result.rfc2822_message_id
                );
                Ok(())
            }
        },
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("unexpected response from daemon"),
    }
}

fn print_preview(
    preview: &CalendarInviteResponsePreview,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(preview)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(preview)?),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "message_id",
                "action",
                "attendee_email",
                "organizer_email",
                "subject",
            ])?;
            writer.write_record([
                preview.message_id.as_str(),
                format!("{:?}", preview.action),
                preview.attendee_email.clone(),
                preview.organizer_email.clone(),
                preview.subject.clone(),
            ])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => println!("{}", preview.message_id),
        OutputFormat::Table => {
            println!("Dry run: {}", preview.action.label());
            println!("Message: {}", preview.message_id);
            println!("Attendee: {}", preview.attendee_email);
            println!("Organizer: {}", preview.organizer_email);
            println!("Subject: {}", preview.subject);
            if !preview.warnings.is_empty() {
                println!("Warnings:");
                for warning in &preview.warnings {
                    println!("  - {warning}");
                }
            }
            println!();
            println!("{}", preview.ics);
        }
    }
    Ok(())
}

fn print_invites(invites: &[CalendarInviteData], format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            if invites.len() == 1 {
                println!("{}", serde_json::to_string_pretty(&invites[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(invites)?);
            }
        }
        OutputFormat::Jsonl => {
            println!("{}", jsonl(invites)?);
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "message_id",
                "method",
                "uid",
                "sequence",
                "summary",
                "starts_at",
                "organizer",
                "rsvp_requested",
            ])?;
            for invite in invites {
                writer.write_record([
                    invite.message_id.as_str(),
                    invite.metadata.method.clone().unwrap_or_default(),
                    invite.metadata.uid.clone().unwrap_or_default(),
                    invite
                        .metadata
                        .sequence
                        .map(|sequence| sequence.to_string())
                        .unwrap_or_default(),
                    invite.metadata.summary.clone().unwrap_or_default(),
                    invite.metadata.starts_at.clone().unwrap_or_default(),
                    invite
                        .metadata
                        .organizer
                        .as_ref()
                        .map(|organizer| organizer.email.clone())
                        .unwrap_or_default(),
                    invite.metadata.rsvp_requested.to_string(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for invite in invites {
                println!("{}", invite.message_id);
            }
        }
        OutputFormat::Table => {
            if invites.is_empty() {
                println!("No calendar invites");
            }
            for invite in invites {
                print_invite_table(invite);
            }
        }
    }
    Ok(())
}

fn print_invite_table(invite: &CalendarInviteData) {
    println!(
        "{}",
        invite
            .metadata
            .summary
            .as_deref()
            .unwrap_or("(untitled calendar invite)")
    );
    println!("Message: {}", invite.message_id);
    if let Some(method) = invite.metadata.method.as_deref() {
        println!("Method: {method}");
    }
    if let Some(uid) = invite.metadata.uid.as_deref() {
        println!("UID: {uid}");
    }
    if let Some(sequence) = invite.metadata.sequence {
        println!("Sequence: {sequence}");
    }
    if let Some(starts_at) = invite.metadata.starts_at.as_deref() {
        println!("Starts: {starts_at}");
    }
    if let Some(ends_at) = invite.metadata.ends_at.as_deref() {
        println!("Ends: {ends_at}");
    }
    if let Some(location) = invite.metadata.location.as_deref() {
        println!("Location: {location}");
    }
    if let Some(rrule) = invite.metadata.rrule.as_deref() {
        println!("Repeats: {rrule}");
    }
    if let Some(organizer) = invite.metadata.organizer.as_ref() {
        if let Some(name) = organizer.name.as_deref() {
            println!("Organizer: {name} <{}>", organizer.email);
        } else {
            println!("Organizer: {}", organizer.email);
        }
    }
    if !invite.metadata.attendees.is_empty() {
        println!("Attendees:");
        for attendee in &invite.metadata.attendees {
            let status = attendee.partstat.as_deref().unwrap_or("UNKNOWN");
            if let Some(name) = attendee.name.as_deref() {
                println!("  - {name} <{}> ({status})", attendee.email);
            } else {
                println!("  - {} ({status})", attendee.email);
            }
        }
    }
    if !invite.metadata.warnings.is_empty() {
        println!("Warnings:");
        for warning in &invite.metadata.warnings {
            println!("  - {warning}");
        }
    }
}
