use super::{
    apply_snooze, build_reply_references, reconcile_label_mutation, render_message_context,
    restore_snoozed_message, HandlerResult,
};
use crate::state::AppState;
use mxr_core::types::{
    Address, Draft, DraftStatus, Envelope, MessageBody, MessageFlags, MessageMetadata, SendReceipt,
    UnsubscribeMethod,
};
use mxr_protocol::{
    AccountMutationResultData, ForwardContext, MutationCommand, MutationResultData, ReplyContext,
    ResponseData,
};
use mxr_store::EventLogRefs;
use std::collections::HashMap;

async fn log_mutation(
    state: &AppState,
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

pub(super) async fn mutation(state: &AppState, cmd: &MutationCommand) -> HandlerResult {
    let message_ids = mutation_message_ids(cmd);
    let mut grouped: HashMap<mxr_core::AccountId, Vec<Envelope>> = HashMap::new();
    for message_id in message_ids {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Message not found: {message_id}"))?;
        grouped
            .entry(envelope.account_id.clone())
            .or_default()
            .push(envelope);
    }

    let mut accounts = Vec::new();
    for (account_id, envelopes) in grouped {
        let account_name = state
            .store
            .get_account(&account_id)
            .await
            .ok()
            .flatten()
            .map(|account| account.name)
            .unwrap_or_else(|| account_id.to_string());

        let mut account_result = AccountMutationResultData {
            account_id: account_id.clone(),
            account_name,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            error: None,
        };

        let provider = match state.get_provider(Some(&account_id)) {
            Ok(provider) => provider,
            Err(error) => {
                account_result.skipped = envelopes.len() as u32;
                account_result.error = Some(format!("account unavailable: {error}"));
                accounts.push(account_result);
                continue;
            }
        };

        for (index, envelope) in envelopes.iter().enumerate() {
            match apply_mutation_to_envelope(state, provider.as_ref(), cmd, envelope).await {
                Ok(()) => {
                    account_result.succeeded += 1;
                    let (summary, details) = mutation_log_entry(cmd, envelope);
                    if let Err(error) = log_mutation(state, envelope, summary, details).await {
                        tracing::warn!(%error, "failed to record mutation event");
                    }
                }
                Err(error) => {
                    account_result.skipped += (envelopes.len() - index) as u32;
                    account_result.error = Some(format!("account unavailable: {error}"));
                    break;
                }
            }
        }

        accounts.push(account_result);
    }

    accounts.sort_by(|left, right| {
        left.account_name
            .to_lowercase()
            .cmp(&right.account_name.to_lowercase())
            .then_with(|| left.account_id.as_str().cmp(&right.account_id.as_str()))
    });
    let succeeded = accounts.iter().map(|account| account.succeeded).sum();
    let skipped = accounts.iter().map(|account| account.skipped).sum();
    let failed = accounts.iter().map(|account| account.failed).sum();

    Ok(ResponseData::MutationResult {
        result: MutationResultData {
            requested: message_ids.len() as u32,
            succeeded,
            skipped,
            failed,
            accounts,
        },
    })
}

fn mutation_message_ids(cmd: &MutationCommand) -> &[mxr_core::MessageId] {
    match cmd {
        MutationCommand::Archive { message_ids }
        | MutationCommand::ReadAndArchive { message_ids }
        | MutationCommand::Trash { message_ids }
        | MutationCommand::Spam { message_ids }
        | MutationCommand::Star { message_ids, .. }
        | MutationCommand::SetRead { message_ids, .. }
        | MutationCommand::ModifyLabels { message_ids, .. }
        | MutationCommand::Move { message_ids, .. } => message_ids,
    }
}

async fn apply_mutation_to_envelope(
    state: &AppState,
    provider: &dyn mxr_core::MailSyncProvider,
    cmd: &MutationCommand,
    envelope: &Envelope,
) -> Result<(), String> {
    let message_id = &envelope.id;
    let provider_id = &envelope.provider_id;
    match cmd {
        MutationCommand::Archive { .. } => {
            provider
                .modify_labels(provider_id, &[], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(state, provider, message_id, &[], &["INBOX".to_string()])
                .await?;
        }
        MutationCommand::ReadAndArchive { .. } => {
            provider
                .set_read(provider_id, true)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_read(message_id, true, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
            provider
                .modify_labels(provider_id, &[], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(state, provider, message_id, &[], &["INBOX".to_string()])
                .await?;
        }
        MutationCommand::Trash { .. } => {
            provider
                .trash(provider_id)
                .await
                .map_err(|e| e.to_string())?;
            // Trash is provider-specific (Gmail relabels, IMAP moves+expunges).
            // Mirror the common case locally: drop INBOX, add TRASH if labels-capable;
            // otherwise let reconcile_label_mutation re-sync from the provider.
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &["TRASH".to_string()],
                &["INBOX".to_string()],
            )
            .await?;
        }
        MutationCommand::Spam { .. } => {
            provider
                .modify_labels(provider_id, &["SPAM".to_string()], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &["SPAM".to_string()],
                &["INBOX".to_string()],
            )
            .await?;
        }
        MutationCommand::Star { starred, .. } => {
            provider
                .set_starred(provider_id, *starred)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_starred(message_id, *starred, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
        }
        MutationCommand::SetRead { read, .. } => {
            provider
                .set_read(provider_id, *read)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_read(message_id, *read, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
        }
        MutationCommand::ModifyLabels { add, remove, .. } => {
            let labels = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
                .map_err(|e| e.to_string())?;
            let resolved_add = resolve_to_provider_ids(&labels, add);
            let resolved_remove = resolve_to_provider_ids(&labels, remove);
            provider
                .modify_labels(provider_id, &resolved_add, &resolved_remove)
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(state, provider, message_id, &resolved_add, &resolved_remove)
                .await?;
        }
        MutationCommand::Move { target_label, .. } => {
            let labels = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
                .map_err(|e| e.to_string())?;
            let resolved_target =
                resolve_to_provider_ids(&labels, std::slice::from_ref(target_label));
            provider
                .modify_labels(provider_id, &resolved_target, &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &resolved_target,
                &["INBOX".to_string()],
            )
            .await?;
        }
    }
    // Single point of search-index reconciliation: refresh the indexed envelope so
    // queries (`mxr search ...`) see the new flags/labels immediately.
    reindex_message_in_search(state, message_id).await
}

async fn reindex_message_in_search(
    state: &AppState,
    message_id: &mxr_core::MessageId,
) -> Result<(), String> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?;
    let Some(envelope) = envelope else {
        // Message was removed locally (e.g. IMAP delete); drop from index.
        return state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: Vec::new(),
                removed_message_ids: vec![message_id.clone()],
            })
            .await
            .map_err(|e| e.to_string());
    };
    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: vec![mxr_search::SearchIndexEntry {
                envelope,
                body: None,
            }],
            removed_message_ids: Vec::new(),
        })
        .await
        .map_err(|e| e.to_string())
}

fn mutation_log_entry(cmd: &MutationCommand, envelope: &Envelope) -> (String, Option<String>) {
    match cmd {
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
    }
}

pub(super) async fn snooze(
    state: &AppState,
    message_id: &mxr_core::MessageId,
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

pub(super) async fn unsnooze(state: &AppState, message_id: &mxr_core::MessageId) -> HandlerResult {
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

pub(super) async fn list_snoozed(state: &AppState) -> HandlerResult {
    let snoozed = state
        .store
        .list_snoozed()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SnoozedMessages { snoozed })
}

pub(super) async fn list_drafts(state: &AppState) -> HandlerResult {
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

pub(super) async fn save_draft(state: &AppState, draft: &Draft) -> HandlerResult {
    state
        .store
        .insert_draft(draft)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn delete_draft(state: &AppState, draft_id: &mxr_core::DraftId) -> HandlerResult {
    state
        .store
        .delete_draft(draft_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn prepare_reply(
    state: &AppState,
    message_id: &mxr_core::MessageId,
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

    let self_address = from.to_ascii_lowercase();
    // Envelope does not yet capture the Reply-To: header — using From: as the reply target
    // covers the common case. Capturing Reply-To: properly is a post-v1 envelope schema change.
    let reply_to = envelope.from.email.clone();

    let cc = if reply_all {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        seen.insert(reply_to.to_ascii_lowercase());
        if !self_address.is_empty() {
            seen.insert(self_address.clone());
        }
        envelope
            .to
            .iter()
            .chain(envelope.cc.iter())
            .map(|address| address.email.clone())
            .filter(|email| {
                let key = email.to_ascii_lowercase();
                seen.insert(key)
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        String::new()
    };

    Ok(ResponseData::ReplyContext {
        context: ReplyContext {
            account_id: envelope.account_id.clone(),
            in_reply_to: envelope.message_id_header.clone().unwrap_or_default(),
            references: build_reply_references(&envelope),
            reply_to,
            cc,
            subject: envelope.subject.clone(),
            from,
            thread_context,
            // Provider-native thread hint is sourced from a future Envelope.provider_thread_id
            // field; today In-Reply-To/References headers do the heavy lifting for threading.
            thread_id: None,
        },
    })
}

pub(super) async fn prepare_forward(
    state: &AppState,
    message_id: &mxr_core::MessageId,
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
            account_id: envelope.account_id.clone(),
            subject: envelope.subject.clone(),
            from,
            forwarded_content,
        },
    })
}

async fn resolve_from_address(state: &AppState, draft: &Draft) -> Address {
    let account = state
        .store
        .get_account(&draft.account_id)
        .await
        .ok()
        .flatten();
    Address {
        name: account.as_ref().map(|account| account.name.clone()),
        email: account
            .as_ref()
            .map(|account| account.email.clone())
            .unwrap_or_else(|| "user@example.com".to_string()),
    }
}

pub(super) async fn send_draft(state: &AppState, draft: &Draft) -> HandlerResult {
    let sender = state.send_provider_for_account(&draft.account_id)?;
    let from = resolve_from_address(state, draft).await;
    let receipt = sender.send(draft, &from).await.map_err(|e| e.to_string())?;
    // Ingest synthetic Sent envelope so the message is searchable as `is:sent`
    // immediately, without waiting for the next sync. Failures here are
    // non-fatal: the send already succeeded on the wire and we surface the
    // local-ingest error to the caller via tracing.
    if let Err(e) = ingest_sent_message(state, draft, &from, &receipt).await {
        tracing::warn!(error = %e, "ingest_sent_message failed after send_draft");
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn send_stored_draft(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
) -> HandlerResult {
    let draft = state
        .store
        .get_draft(draft_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Draft not found: {draft_id}"))?;

    // Compare-and-set: only the unique `Draft` -> `Sending` transition is
    // allowed to invoke the provider. A draft already in `Sending` (likely a
    // crashed prior attempt) or `Sent` (already delivered) refuses.
    let advanced = state
        .store
        .cas_draft_status(draft_id, DraftStatus::Draft, DraftStatus::Sending)
        .await
        .map_err(|e| e.to_string())?;
    if !advanced {
        let current = state
            .store
            .get_draft_status(draft_id)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(DraftStatus::Draft);
        return Err(match current {
            DraftStatus::Sent => "draft already sent".to_string(),
            DraftStatus::Sending => {
                "draft is already being sent (resolve via `mxr drafts resolve`)".to_string()
            }
            DraftStatus::Draft => "draft state mismatch; retry".to_string(),
        });
    }

    let sender = match state.send_provider_for_account(&draft.account_id) {
        Ok(s) => s,
        Err(e) => {
            // Revert; the daemon never invoked the provider.
            let _ = state
                .store
                .update_draft_status(draft_id, DraftStatus::Draft)
                .await;
            return Err(e);
        }
    };
    let from = resolve_from_address(state, &draft).await;

    let receipt = match sender.send(&draft, &from).await {
        Ok(r) => r,
        Err(e) => {
            let _ = state
                .store
                .update_draft_status(draft_id, DraftStatus::Draft)
                .await;
            return Err(e.to_string());
        }
    };

    // Persist the Message-ID we used; helps IMAP-side dedupe on next sync.
    let _ = state
        .store
        .set_draft_message_id_header(draft_id, &receipt.rfc2822_message_id)
        .await;

    if let Err(e) = ingest_sent_message(state, &draft, &from, &receipt).await {
        // Local ingest failed. Send already happened on the wire; we move the
        // draft to `Sent` (idempotent, prevents resend) but bubble the error so
        // the user can re-sync.
        let _ = state
            .store
            .update_draft_status(draft_id, DraftStatus::Sent)
            .await;
        return Err(format!("send succeeded but local ingest failed: {e}"));
    }

    let _ = state
        .store
        .update_draft_status(draft_id, DraftStatus::Sent)
        .await;
    let _ = state.store.delete_draft(draft_id).await;
    Ok(ResponseData::Ack)
}

/// Insert a synthetic envelope + body into the local store immediately after a
/// successful provider send so the message is searchable as `is:sent` without
/// waiting for the next sync. Keyed by `provider_message_id` (Gmail) or the
/// rendered Message-ID header (SMTP) so the next sync's `upsert_envelope` is
/// idempotent for Gmail (same UUID v5) — see `MessageId::from_scoped_provider_id`.
async fn ingest_sent_message(
    state: &AppState,
    draft: &Draft,
    from: &Address,
    receipt: &SendReceipt,
) -> Result<(), String> {
    let (provider_namespace, provider_id_value) =
        if let Some(gmail_id) = receipt.provider_message_id.as_deref() {
            ("gmail", gmail_id.to_string())
        } else {
            ("smtp-local", receipt.rfc2822_message_id.clone())
        };
    let message_id = mxr_core::MessageId::from_scoped_provider_id(
        &draft.account_id,
        provider_namespace,
        &provider_id_value,
    );

    let snippet: String = draft
        .body_markdown
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();

    let envelope = Envelope {
        id: message_id.clone(),
        account_id: draft.account_id.clone(),
        provider_id: provider_id_value,
        thread_id: mxr_core::ThreadId::new(),
        message_id_header: Some(receipt.rfc2822_message_id.clone()),
        in_reply_to: draft.reply_headers.as_ref().map(|h| h.in_reply_to.clone()),
        references: draft
            .reply_headers
            .as_ref()
            .map(|h| h.references.clone())
            .unwrap_or_default(),
        from: from.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        date: receipt.sent_at,
        flags: MessageFlags::SENT | MessageFlags::READ,
        snippet: snippet.clone(),
        has_attachments: !draft.attachments.is_empty(),
        size_bytes: draft.body_markdown.len() as u64,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: if provider_namespace == "gmail" {
            vec!["SENT".to_string()]
        } else {
            Vec::new()
        },
    };

    state
        .store
        .upsert_envelope(&envelope)
        .await
        .map_err(|e| e.to_string())?;

    let body = MessageBody {
        message_id: message_id.clone(),
        text_plain: Some(draft.body_markdown.clone()),
        text_html: None,
        attachments: Vec::new(),
        fetched_at: receipt.sent_at,
        metadata: MessageMetadata::default(),
    };
    state
        .store
        .insert_body(&body)
        .await
        .map_err(|e| e.to_string())?;

    // Apply Gmail SENT label if the account already knows about it (it will,
    // post-first-sync). Missing label is non-fatal: `is:sent` queries match on
    // MessageFlags::SENT regardless of the labels junction.
    if provider_namespace == "gmail" {
        if let Ok(Some(sent_label)) = state
            .store
            .find_label_by_provider_id(&draft.account_id, "SENT")
            .await
        {
            let _ = state
                .store
                .set_message_labels(
                    &message_id,
                    std::slice::from_ref(&sent_label.id),
                    mxr_core::EventSource::User,
                )
                .await;
        }
    }

    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: vec![mxr_search::SearchIndexEntry {
                envelope,
                body: Some(body),
            }],
            removed_message_ids: Vec::new(),
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub(super) async fn save_draft_to_server(state: &AppState, draft: &Draft) -> HandlerResult {
    let sender = state.send_provider_for_account(&draft.account_id)?;
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
    state: &AppState,
    message_id: &mxr_core::MessageId,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;
    match &envelope.unsubscribe {
        UnsubscribeMethod::Mailto { address, subject } => {
            let sender = state.send_provider_for_account(&envelope.account_id)?;
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
                id: mxr_core::DraftId::new(),
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

/// Resolve label references (names or provider IDs) to provider IDs.
///
/// Both the TUI and CLI send label display names (e.g. "Follow Up") but
/// the Gmail API requires provider IDs (e.g. "Label_123"). This function
/// looks up each reference in the account's label list and returns the
/// corresponding provider_id. If no match is found the original string
/// is passed through (handles system labels like "INBOX", "SPAM" where
/// name == provider_id).
fn resolve_to_provider_ids(labels: &[mxr_core::types::Label], refs: &[String]) -> Vec<String> {
    refs.iter()
        .map(|label_ref| {
            labels
                .iter()
                .find(|l| l.name == *label_ref || l.provider_id == *label_ref)
                .map(|l| l.provider_id.clone())
                .unwrap_or_else(|| label_ref.clone())
        })
        .collect()
}

pub(super) async fn set_flags(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    flags: mxr_core::MessageFlags,
) -> HandlerResult {
    state
        .store
        .update_flags(message_id, flags, mxr_core::EventSource::User)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}
