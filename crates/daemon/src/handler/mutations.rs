use super::{
    apply_snooze, build_reply_references, persist_local_label_changes, render_message_context,
    restore_snoozed_message, HandlerResult,
};
use crate::mxr_core::types::{Address, Draft, Envelope, UnsubscribeMethod};
use crate::mxr_protocol::{ForwardContext, MutationCommand, ReplyContext, ResponseData};
use crate::mxr_store::EventLogRefs;
use crate::state::AppState;
use std::sync::Arc;

async fn log_mutation(
    state: &Arc<AppState>,
    envelope: &Envelope,
    summary: String,
    details: Option<String>,
) -> Result<(), String> {
    let message_id = envelope.id.as_str();
    state
        .store
        .insert_event_refs(
            "info",
            "mutation",
            &summary,
            EventLogRefs {
                account_id: Some(&envelope.account_id),
                message_id: Some(message_id.as_str()),
                rule_id: None,
            },
            details.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}

fn quoted_subject(subject: &str) -> String {
    if subject.is_empty() {
        "message".to_string()
    } else {
        format!("\"{subject}\"")
    }
}

pub(super) async fn mutation(state: &Arc<AppState>, cmd: &MutationCommand) -> HandlerResult {
    let message_ids = match cmd {
        MutationCommand::Archive { message_ids }
        | MutationCommand::ReadAndArchive { message_ids }
        | MutationCommand::Trash { message_ids }
        | MutationCommand::Spam { message_ids }
        | MutationCommand::Star { message_ids, .. }
        | MutationCommand::SetRead { message_ids, .. }
        | MutationCommand::ModifyLabels { message_ids, .. }
        | MutationCommand::Move { message_ids, .. } => message_ids,
    };

    for message_id in message_ids {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Message not found: {message_id}"))?;
        let provider_id = &envelope.provider_id;
        let provider = state.get_provider(Some(&envelope.account_id)).clone();

        let result = match cmd {
            MutationCommand::Archive { .. } => {
                provider
                    .modify_labels(provider_id, &[], &["INBOX".to_string()])
                    .await
                    .map_err(|e| e.to_string())?;
                let mut label_ids = state
                    .store
                    .get_message_label_ids(message_id)
                    .await
                    .unwrap_or_default();
                label_ids.retain(|label_id| label_id.as_str() != "INBOX");
                state
                    .store
                    .set_message_labels(message_id, &label_ids)
                    .await
                    .map_err(|e| e.to_string())
            }
            MutationCommand::ReadAndArchive { .. } => {
                provider
                    .set_read(provider_id, true)
                    .await
                    .map_err(|e| e.to_string())?;
                state
                    .store
                    .set_read(message_id, true)
                    .await
                    .map_err(|e| e.to_string())?;
                provider
                    .modify_labels(provider_id, &[], &["INBOX".to_string()])
                    .await
                    .map_err(|e| e.to_string())?;
                let mut label_ids = state
                    .store
                    .get_message_label_ids(message_id)
                    .await
                    .unwrap_or_default();
                label_ids.retain(|label_id| label_id.as_str() != "INBOX");
                state
                    .store
                    .set_message_labels(message_id, &label_ids)
                    .await
                    .map_err(|e| e.to_string())
            }
            MutationCommand::Trash { .. } => {
                provider.trash(provider_id).await.map_err(|e| e.to_string())
            }
            MutationCommand::Spam { .. } => provider
                .modify_labels(provider_id, &["SPAM".to_string()], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string()),
            MutationCommand::Star { starred, .. } => {
                provider
                    .set_starred(provider_id, *starred)
                    .await
                    .map_err(|e| e.to_string())?;
                state
                    .store
                    .set_starred(message_id, *starred)
                    .await
                    .map_err(|e| e.to_string())
            }
            MutationCommand::SetRead { read, .. } => {
                provider
                    .set_read(provider_id, *read)
                    .await
                    .map_err(|e| e.to_string())?;
                state
                    .store
                    .set_read(message_id, *read)
                    .await
                    .map_err(|e| e.to_string())
            }
            MutationCommand::ModifyLabels { add, remove, .. } => {
                provider
                    .modify_labels(provider_id, add, remove)
                    .await
                    .map_err(|e| e.to_string())?;
                persist_local_label_changes(state, message_id, add, remove)
                    .await
                    .map_err(|e| e.to_string())
            }
            MutationCommand::Move { target_label, .. } => {
                provider
                    .modify_labels(
                        provider_id,
                        std::slice::from_ref(target_label),
                        &["INBOX".to_string()],
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                persist_local_label_changes(
                    state,
                    message_id,
                    std::slice::from_ref(target_label),
                    &["INBOX".to_string()],
                )
                .await
                .map_err(|e| e.to_string())
            }
        };

        result?;
        let (summary, details) = match cmd {
            MutationCommand::Archive { .. } => (
                format!("Archived {}", quoted_subject(&envelope.subject)),
                Some(format!("from={}", envelope.from.email)),
            ),
            MutationCommand::ReadAndArchive { .. } => (
                format!(
                    "Marked {} as read and archived",
                    quoted_subject(&envelope.subject)
                ),
                Some(format!("from={}", envelope.from.email)),
            ),
            MutationCommand::Trash { .. } => (
                format!("Moved {} to trash", quoted_subject(&envelope.subject)),
                Some(format!("from={}", envelope.from.email)),
            ),
            MutationCommand::Spam { .. } => (
                format!("Marked {} as spam", quoted_subject(&envelope.subject)),
                Some(format!("from={}", envelope.from.email)),
            ),
            MutationCommand::Star { starred, .. } => (
                if *starred {
                    format!("Starred {}", quoted_subject(&envelope.subject))
                } else {
                    format!("Unstarred {}", quoted_subject(&envelope.subject))
                },
                Some(format!("from={}", envelope.from.email)),
            ),
            MutationCommand::SetRead { read, .. } => (
                if *read {
                    format!("Marked {} as read", quoted_subject(&envelope.subject))
                } else {
                    format!("Marked {} as unread", quoted_subject(&envelope.subject))
                },
                Some(format!("from={}", envelope.from.email)),
            ),
            MutationCommand::ModifyLabels { add, remove, .. } => (
                format!("Updated labels on {}", quoted_subject(&envelope.subject)),
                Some(format!(
                    "from={} add={} remove={}",
                    envelope.from.email,
                    add.join(","),
                    remove.join(",")
                )),
            ),
            MutationCommand::Move { target_label, .. } => (
                format!(
                    "Moved {} to {}",
                    quoted_subject(&envelope.subject),
                    target_label
                ),
                Some(format!("from={}", envelope.from.email)),
            ),
        };
        if let Err(error) = log_mutation(state, &envelope, summary, details).await {
            tracing::warn!(%error, "failed to record mutation event");
        }
    }

    Ok(ResponseData::Ack)
}

pub(super) async fn snooze(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
    wake_at: &chrono::DateTime<chrono::Utc>,
) -> HandlerResult {
    apply_snooze(state, message_id, wake_at).await?;
    if let Some(envelope) = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    {
        if let Err(error) = log_mutation(
            state,
            &envelope,
            format!(
                "Snoozed {} until {}",
                quoted_subject(&envelope.subject),
                wake_at
            ),
            Some(format!("from={}", envelope.from.email)),
        )
        .await
        {
            tracing::warn!(%error, "failed to record snooze event");
        }
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn unsnooze(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
) -> HandlerResult {
    let snoozed = state
        .store
        .get_snooze(message_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(snoozed) = snoozed {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?;
        restore_snoozed_message(state, &snoozed).await?;
        if let Some(envelope) = envelope {
            if let Err(error) = log_mutation(
                state,
                &envelope,
                format!("Unsnoozed {}", quoted_subject(&envelope.subject)),
                Some(format!("from={}", envelope.from.email)),
            )
            .await
            {
                tracing::warn!(%error, "failed to record unsnooze event");
            }
        }
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn list_snoozed(state: &Arc<AppState>) -> HandlerResult {
    let snoozed = state
        .store
        .list_snoozed()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SnoozedMessages { snoozed })
}

pub(super) async fn list_drafts(state: &Arc<AppState>) -> HandlerResult {
    let Some(default_account_id) = state.default_account_id_opt() else {
        return Ok(ResponseData::Drafts { drafts: Vec::new() });
    };
    let drafts = state
        .store
        .list_drafts(&default_account_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Drafts { drafts })
}

pub(super) async fn prepare_reply(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
    reply_all: bool,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;

    let from = state
        .store
        .get_account(&envelope.account_id)
        .await
        .ok()
        .flatten()
        .map(|account| account.email)
        .unwrap_or_default();

    let thread_context = match state.sync_engine.get_body(message_id).await {
        Ok(body) => render_message_context(&body),
        Err(_) => String::new(),
    };

    let cc = if reply_all {
        envelope
            .cc
            .iter()
            .map(|address| address.email.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        String::new()
    };

    Ok(ResponseData::ReplyContext {
        context: ReplyContext {
            in_reply_to: envelope.message_id_header.clone().unwrap_or_default(),
            references: build_reply_references(&envelope),
            reply_to: envelope.from.email.clone(),
            cc,
            subject: envelope.subject.clone(),
            from,
            thread_context,
        },
    })
}

pub(super) async fn prepare_forward(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;

    let from = state
        .store
        .get_account(&envelope.account_id)
        .await
        .ok()
        .flatten()
        .map(|account| account.email)
        .unwrap_or_default();

    let forwarded_content = match state.sync_engine.get_body(message_id).await {
        Ok(body) => render_message_context(&body),
        Err(_) => String::new(),
    };

    Ok(ResponseData::ForwardContext {
        context: ForwardContext {
            subject: envelope.subject.clone(),
            from,
            forwarded_content,
        },
    })
}

pub(super) async fn send_draft(state: &Arc<AppState>, draft: &Draft) -> HandlerResult {
    let sender = state
        .get_send_provider(Some(&draft.account_id))
        .ok_or_else(|| "No send provider configured".to_string())?;
    let account = state
        .store
        .get_account(&draft.account_id)
        .await
        .ok()
        .flatten();
    let from = Address {
        name: account.as_ref().map(|account| account.name.clone()),
        email: account
            .as_ref()
            .map(|account| account.email.clone())
            .unwrap_or_else(|| "user@example.com".to_string()),
    };
    sender.send(draft, &from).await.map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn save_draft_to_server(state: &Arc<AppState>, draft: &Draft) -> HandlerResult {
    let sender = state
        .get_send_provider(Some(&draft.account_id))
        .ok_or_else(|| "No send provider configured".to_string())?;
    let account = state
        .store
        .get_account(&draft.account_id)
        .await
        .ok()
        .flatten();
    let from = Address {
        name: account.as_ref().map(|account| account.name.clone()),
        email: account
            .as_ref()
            .map(|account| account.email.clone())
            .unwrap_or_else(|| "user@example.com".to_string()),
    };
    match sender.save_draft(draft, &from).await {
        Ok(Some(draft_id)) => {
            tracing::info!(draft_id, "Draft saved to server");
            Ok(ResponseData::Ack)
        }
        Ok(None) => Err("Provider does not support server-side drafts".to_string()),
        Err(error) => Err(format!("Failed to save draft: {error}")),
    }
}

pub(super) async fn unsubscribe(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;
    match &envelope.unsubscribe {
        UnsubscribeMethod::Mailto { address, subject } => {
            let sender = state
                .get_send_provider(Some(&envelope.account_id))
                .ok_or_else(|| "No send provider configured".to_string())?;
            let account = state
                .store
                .get_account(&envelope.account_id)
                .await
                .ok()
                .flatten();
            let from = Address {
                name: account.as_ref().map(|account| account.name.clone()),
                email: account
                    .as_ref()
                    .map(|account| account.email.clone())
                    .unwrap_or_else(|| "user@example.com".to_string()),
            };
            let now = chrono::Utc::now();
            let draft = Draft {
                id: crate::mxr_core::DraftId::new(),
                account_id: envelope.account_id.clone(),
                reply_headers: None,
                to: vec![Address {
                    name: None,
                    email: address.clone(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: subject.clone().unwrap_or_else(|| "unsubscribe".to_string()),
                body_markdown: "unsubscribe".to_string(),
                attachments: vec![],
                created_at: now,
                updated_at: now,
            };
            sender
                .send(&draft, &from)
                .await
                .map_err(|e| e.to_string())?;
            if let Err(error) = log_mutation(
                state,
                &envelope,
                format!(
                    "Sent unsubscribe request for {}",
                    quoted_subject(&envelope.subject)
                ),
                Some(format!("mailto={address} from={}", envelope.from.email)),
            )
            .await
            {
                tracing::warn!(%error, "failed to record unsubscribe event");
            }
            Ok(ResponseData::Ack)
        }
        _ => {
            let client = reqwest::Client::new();
            match crate::unsubscribe::execute_unsubscribe(&envelope.unsubscribe, &client).await {
                crate::unsubscribe::UnsubscribeResult::Success(result) => {
                    if let Err(error) = log_mutation(
                        state,
                        &envelope,
                        format!("Unsubscribed from {}", quoted_subject(&envelope.subject)),
                        Some(format!("result={result} from={}", envelope.from.email)),
                    )
                    .await
                    {
                        tracing::warn!(%error, "failed to record unsubscribe event");
                    }
                    Ok(ResponseData::Ack)
                }
                crate::unsubscribe::UnsubscribeResult::Failed(message) => Err(message),
                crate::unsubscribe::UnsubscribeResult::NoMethod => {
                    Err("No unsubscribe method available for this message".to_string())
                }
            }
        }
    }
}

pub(super) async fn set_flags(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
    flags: crate::mxr_core::MessageFlags,
) -> HandlerResult {
    state
        .store
        .update_flags(message_id, flags)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}
