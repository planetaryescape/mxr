use super::{
    find_label_by_name, materialize_attachment_file, open_local_file,
    populate_envelope_label_provider_ids, HandlerResult,
};
use crate::state::AppState;
use mxr_core::i18n::SendableCalendarPartstat;
use mxr_core::id::{AccountId, AttachmentId, LabelId, MessageId, ThreadId};
use mxr_core::types::{
    Account, Address, BodyPartSource, CalendarAttendee, CalendarMetadata, CalendarPartstat,
    CalendarReplyMessage, Envelope, HtmlImageAsset, HtmlImageAssetStatus, HtmlImageSourceKind,
    MessageBody,
};
use mxr_protocol::{
    BodyFailure, CalendarInviteActionData, CalendarInviteData, CalendarInviteResponsePreview,
    CalendarInviteResponseResult, ResponseData,
};
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_REMOTE_ASSET_BYTES: u64 = 25 * 1024 * 1024;

fn resolve_account_id(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<AccountId, String> {
    account_id
        .cloned()
        .or_else(|| state.default_account_id_opt())
        .ok_or_else(|| "No sync-capable accounts configured".to_string())
}

fn sync_account_matches_config_key(account: &Account, config_key: &str) -> bool {
    account
        .sync_backend
        .as_ref()
        .is_some_and(|backend| backend.config_key == config_key)
}

async fn resolve_local_account_id(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<Option<AccountId>, String> {
    if let Some(account_id) = account_id {
        return Ok(Some(account_id.clone()));
    }

    if let Some(account_id) = state.default_account_id_opt() {
        return Ok(Some(account_id));
    }

    let accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;
    let config = state.config_snapshot();
    if let Some(default_key) = config.general.default_account.as_deref() {
        if let Some(account) = accounts.iter().find(|account| {
            account.enabled && sync_account_matches_config_key(account, default_key)
        }) {
            return Ok(Some(account.id.clone()));
        }
    }

    Ok(accounts
        .iter()
        .find(|account| account.enabled && account.sync_backend.is_some())
        .or_else(|| accounts.iter().find(|account| account.enabled))
        .map(|account| account.id.clone()))
}

pub(super) async fn list_envelopes(
    state: &AppState,
    label_id: Option<&LabelId>,
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> HandlerResult {
    let result = if let Some(label_id) = label_id {
        tracing::debug!(label_id = %label_id, limit, offset, "listing envelopes by label");
        state
            .store
            .list_envelopes_by_label(label_id, limit, offset)
            .await
    } else {
        let Some(default_account_id) = resolve_local_account_id(state, account_id).await? else {
            return Ok(ResponseData::Envelopes {
                envelopes: Vec::new(),
            });
        };
        state
            .store
            .list_envelopes_by_account(&default_account_id, limit, offset)
            .await
    };

    let mut envelopes = result?;
    let enabled_accounts = state
        .store
        .list_accounts()
        .await?
        .into_iter()
        .map(|account| account.id)
        .collect::<HashSet<_>>();
    envelopes.retain(|envelope| enabled_accounts.contains(&envelope.account_id));
    for envelope in &mut envelopes {
        if let Ok(labels) = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
        {
            let _ = populate_envelope_label_provider_ids(state, envelope, &labels).await;
        }
    }

    tracing::debug!(
        count = envelopes.len(),
        by_label = label_id.is_some(),
        "listed envelopes"
    );
    Ok(ResponseData::Envelopes { envelopes })
}

pub(super) async fn list_envelopes_by_ids(
    state: &AppState,
    message_ids: &[MessageId],
) -> HandlerResult {
    let mut envelopes = state.store.list_envelopes_by_ids(message_ids).await?;
    for envelope in &mut envelopes {
        if let Ok(labels) = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
        {
            let _ = populate_envelope_label_provider_ids(state, envelope, &labels).await;
        }
    }
    Ok(ResponseData::Envelopes { envelopes })
}

pub(super) async fn get_envelope(state: &AppState, message_id: &MessageId) -> HandlerResult {
    match state.store.get_envelope(message_id).await? {
        Some(mut envelope) => {
            if let Ok(labels) = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
            {
                let _ = populate_envelope_label_provider_ids(state, &mut envelope, &labels).await;
            }
            Ok(ResponseData::Envelope { envelope })
        }
        None => Err(format!("Envelope not found: {message_id}").into()),
    }
}

pub(super) async fn get_body(state: &AppState, message_id: &MessageId) -> HandlerResult {
    let body = load_body_for_message(state, message_id).await?;
    Ok(ResponseData::Body { body })
}

pub(super) async fn get_invite(state: &AppState, message_id: &MessageId) -> HandlerResult {
    let invite = state
        .store
        .get_calendar_invite_for_message(message_id)
        .await?
        .ok_or_else(|| format!("Calendar invite not found: {message_id}"))?;
    Ok(ResponseData::Invite {
        invite: CalendarInviteData {
            id: invite.id,
            account_id: invite.account_id,
            message_id: invite.message_id,
            metadata: invite.metadata,
            created_at: invite.created_at,
            updated_at: invite.updated_at,
        },
    })
}

pub(super) async fn list_invites(state: &AppState, limit: u32) -> HandlerResult {
    let records = state.store.list_calendar_invites(limit).await?;
    // Cache account addresses so a page of invites across a handful of
    // accounts doesn't issue one address query per row.
    let mut address_cache: std::collections::HashMap<AccountId, Vec<String>> =
        std::collections::HashMap::new();
    let mut invites = Vec::with_capacity(records.len());
    for invite in records {
        let mut metadata = invite.metadata;
        let addresses = match address_cache.get(&invite.account_id) {
            Some(addresses) => addresses.clone(),
            None => {
                let addresses = state
                    .store
                    .list_account_addresses(&invite.account_id)
                    .await?
                    .into_iter()
                    .map(|a| a.email)
                    .collect::<Vec<_>>();
                address_cache.insert(invite.account_id.clone(), addresses.clone());
                addresses
            }
        };
        // Derive the viewer's own RSVP status so the invites list can show
        // it without each client re-walking attendees. Mirrors the GetBody
        // enrichment in `enrich_calendar_viewer_fields`.
        if let Some((email, partstat)) =
            mxr_mail_parse::matching_attendee_lenient(&metadata, &addresses).map(|a| {
                (
                    a.email.clone(),
                    a.partstat.as_deref().and_then(CalendarPartstat::parse),
                )
            })
        {
            metadata.viewer_attendee_email = Some(email);
            metadata.viewer_partstat = partstat;
        }
        invites.push(CalendarInviteData {
            id: invite.id,
            account_id: invite.account_id,
            message_id: invite.message_id,
            metadata,
            created_at: invite.created_at,
            updated_at: invite.updated_at,
        });
    }
    Ok(ResponseData::Invites { invites })
}

pub(super) async fn backfill_invites(state: &AppState) -> HandlerResult {
    // First recover attachment-only invites: messages whose `.ics` arrived as
    // an attachment with no inline `text/calendar` part have no parsed calendar
    // metadata in their stored body, so the row rebuild below cannot see them.
    // Re-hydrate them from the provider (whose body fetch now parses calendar
    // attachments) before rebuilding rows from body metadata.
    let rehydrated = rehydrate_attachment_only_invites(state).await;
    let backfilled = state.store.backfill_calendar_invites_from_bodies().await?;
    Ok(ResponseData::CalendarInviteBackfill {
        backfilled,
        rehydrated,
    })
}

/// Re-fetch messages that carry a calendar attachment but have no parsed
/// calendar metadata, persisting the re-parsed body (which now populates
/// `metadata.calendar` and the `calendar_invites` row). Returns the count of
/// messages that gained calendar metadata. Per-message failures are logged and
/// skipped so one bad message never aborts the whole pass.
async fn rehydrate_attachment_only_invites(state: &AppState) -> u64 {
    let page_size: u32 = 200;
    let mut offset: u32 = 0;
    let mut rehydrated: u64 = 0;
    loop {
        let envelopes = match state
            .store
            .list_all_envelopes_paginated(page_size, offset)
            .await
        {
            Ok(envelopes) => envelopes,
            Err(error) => {
                tracing::warn!("backfill_invites: list envelopes failed: {error}");
                break;
            }
        };
        if envelopes.is_empty() {
            break;
        }
        let batch_len = envelopes.len() as u32;
        for envelope in &envelopes {
            let body = match state.store.get_body(&envelope.id).await {
                Ok(Some(body)) => body,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(message_id = %envelope.id, "backfill_invites: get_body failed: {error}");
                    continue;
                }
            };
            // Already has calendar metadata, or has no calendar attachment to
            // recover from — nothing to re-hydrate.
            if body.metadata.calendar.is_some()
                || !body.attachments.iter().any(|att| att.is_calendar())
            {
                continue;
            }
            let Some(provider) = state.sync_provider_for_account(&envelope.account_id) else {
                continue;
            };
            match provider.fetch_message(&envelope.provider_id).await {
                Ok(Some(synced)) if synced.body.metadata.calendar.is_some() => {
                    if let Err(error) = state.sync_engine.persist_synced_message(&synced).await {
                        tracing::warn!(message_id = %envelope.id, "backfill_invites: persist failed: {error}");
                        continue;
                    }
                    rehydrated += 1;
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(message_id = %envelope.id, "backfill_invites: provider fetch failed: {error}");
                }
            }
        }
        if batch_len < page_size {
            break;
        }
        offset += page_size;
    }
    tracing::info!(
        rehydrated,
        "backfill_invites: re-hydrated attachment-only invites"
    );
    rehydrated
}

pub(super) async fn respond_invite(
    state: &AppState,
    message_id: &MessageId,
    action: CalendarInviteActionData,
    dry_run: bool,
) -> HandlerResult {
    let preview = build_invite_response_preview(state, message_id, action).await?;
    if dry_run {
        return Ok(ResponseData::InviteResponsePreview { preview });
    }

    let account_id = preview_account_id(state, message_id).await?;
    let account = state
        .store
        .get_account(&account_id)
        .await?
        .ok_or_else(|| "Account not found for calendar invite".to_string())?;
    let from = Address {
        name: Some(account.name),
        email: account.email,
    };
    let sender = state.send_provider_for_account(&account_id)?;
    let rfc2822_message_id = mxr_outbound::email::generate_message_id(&from);
    let reply = CalendarReplyMessage {
        to: Address {
            name: None,
            email: preview.organizer_email.clone(),
        },
        subject: preview.subject.clone(),
        body_text: preview.body_text.clone(),
        ics: preview.ics.clone(),
    };
    let receipt = sender
        .send_calendar_reply(&reply, &from, &rfc2822_message_id)
        .await?;
    state
        .store
        .update_calendar_invite_partstat(message_id, &preview.attendee_email, action.partstat())
        .await?;

    Ok(ResponseData::InviteResponseSent {
        result: CalendarInviteResponseResult {
            message_id: message_id.clone(),
            action,
            provider_message_id: receipt.provider_message_id,
            rfc2822_message_id: receipt.rfc2822_message_id,
        },
    })
}

pub(super) async fn prepare_invite_response(
    state: &AppState,
    message_id: &MessageId,
    action: CalendarInviteActionData,
) -> HandlerResult {
    let preview = build_invite_response_preview(state, message_id, action).await?;
    Ok(ResponseData::InviteResponsePreview { preview })
}

pub(super) async fn mark_invite_answered(
    state: &AppState,
    message_id: &MessageId,
    attendee_email: &str,
    partstat: CalendarPartstat,
) -> HandlerResult {
    state
        .store
        .update_calendar_invite_partstat(message_id, attendee_email, partstat.as_ical())
        .await?;
    Ok(ResponseData::Acknowledged)
}

async fn preview_account_id(state: &AppState, message_id: &MessageId) -> Result<AccountId, String> {
    state
        .store
        .get_calendar_invite_for_message(message_id)
        .await
        .map_err(|e| e.to_string())?
        .map(|invite| invite.account_id)
        .ok_or_else(|| format!("Calendar invite not found: {message_id}"))
}

async fn build_invite_response_preview(
    state: &AppState,
    message_id: &MessageId,
    action: CalendarInviteActionData,
) -> Result<CalendarInviteResponsePreview, String> {
    let invite = state
        .store
        .get_calendar_invite_for_message(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Calendar invite not found: {message_id}"))?;
    let account = state
        .store
        .get_account(&invite.account_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Account not found for calendar invite".to_string())?;

    let mut calendar = invite.metadata;
    if calendar
        .warnings
        .iter()
        .any(|warning| warning.contains("could not be parsed"))
    {
        return Err("Calendar invite has fatal parser warnings; cannot answer safely".to_string());
    }
    if !calendar
        .method
        .as_deref()
        .is_some_and(|method| method.eq_ignore_ascii_case("REQUEST"))
    {
        return Err("Only METHOD:REQUEST invites can be answered".to_string());
    }
    let organizer = calendar
        .organizer
        .as_ref()
        .ok_or_else(|| "Calendar invite has no organizer".to_string())?;
    let organizer_email = organizer.email.clone();
    let uid = calendar
        .uid
        .as_deref()
        .ok_or_else(|| "Calendar invite has no UID".to_string())?
        .to_string();
    if state
        .store
        .calendar_invite_has_different_organizer(
            &invite.account_id,
            message_id,
            &uid,
            calendar.recurrence_id.as_deref(),
            &organizer_email,
        )
        .await
        .map_err(|e| e.to_string())?
    {
        calendar.warnings.push(
            "calendar invite has same UID as another invite with a different organizer".into(),
        );
    }
    if state
        .store
        .calendar_invite_has_newer_sequence(
            &invite.account_id,
            message_id,
            &uid,
            calendar.recurrence_id.as_deref(),
            calendar.sequence,
        )
        .await
        .map_err(|e| e.to_string())?
    {
        return Err("Calendar invite has a newer update; answer the latest invite".to_string());
    }
    let mut account_emails = state
        .store
        .list_account_addresses(&invite.account_id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|address| address.email)
        .collect::<Vec<_>>();
    if !account_emails
        .iter()
        .any(|email| email.eq_ignore_ascii_case(&account.email))
    {
        account_emails.push(account.email.clone());
    }
    let attendee = mxr_mail_parse::matching_attendee_strict(&calendar, &account_emails)
        .map_err(|err| err.to_string())?;

    let partstat = match action {
        CalendarInviteActionData::Accept => SendableCalendarPartstat::Accepted,
        CalendarInviteActionData::Tentative => SendableCalendarPartstat::Tentative,
        CalendarInviteActionData::Decline => SendableCalendarPartstat::Declined,
    };
    let subject = format!(
        "{}{}",
        state.locale.invite_subject_prefix_for(partstat),
        calendar.summary.as_deref().unwrap_or("calendar invite"),
    );
    let body_text = state.locale.invite_body_for(partstat, &account.email);
    let ics = build_reply_ics(&calendar, &uid, organizer_email.as_str(), attendee, action);

    Ok(CalendarInviteResponsePreview {
        message_id: message_id.clone(),
        action,
        attendee_email: attendee.email.clone(),
        organizer_email,
        subject,
        body_text,
        ics,
        warnings: calendar.warnings,
    })
}

fn build_reply_ics(
    calendar: &CalendarMetadata,
    uid: &str,
    organizer_email: &str,
    attendee: &CalendarAttendee,
    action: CalendarInviteActionData,
) -> String {
    let now = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let cn = attendee
        .name
        .as_deref()
        .map(|name| format!(";CN={}", escape_param(name)))
        .unwrap_or_default();
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "PRODID:-//mxr//calendar email//EN".to_string(),
        "VERSION:2.0".to_string(),
        "METHOD:REPLY".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{}", escape_text(uid)),
        format!("DTSTAMP:{now}"),
    ];
    if let Some(sequence) = calendar.sequence {
        lines.push(format!("SEQUENCE:{sequence}"));
    }
    if let Some(recurrence_id) = calendar.recurrence_id.as_deref() {
        lines.push(format!("RECURRENCE-ID:{}", escape_text(recurrence_id)));
    }
    if let Some(summary) = calendar.summary.as_deref() {
        lines.push(format!("SUMMARY:{}", escape_text(summary)));
    }
    lines.push(format!("ORGANIZER:mailto:{organizer_email}"));
    lines.push(format!(
        "ATTENDEE{cn};PARTSTAT={}:mailto:{}",
        action.partstat(),
        attendee.email
    ));
    lines.push("END:VEVENT".to_string());
    lines.push("END:VCALENDAR".to_string());
    lines.join("\r\n") + "\r\n"
}

fn escape_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn escape_param(value: &str) -> String {
    value.replace('"', "'")
}

pub(super) async fn get_html_image_assets(
    state: &AppState,
    message_id: &MessageId,
    allow_remote: bool,
) -> HandlerResult {
    let body = load_body_for_message(state, message_id).await?;
    let Some(html) = body.text_html.as_deref() else {
        return Ok(ResponseData::HtmlImageAssets {
            message_id: message_id.clone(),
            assets: Vec::new(),
        });
    };

    let mut assets = Vec::new();
    for source in collect_html_image_sources(html) {
        assets
            .push(resolve_html_image_asset(state, message_id, &body, &source, allow_remote).await);
    }

    Ok(ResponseData::HtmlImageAssets {
        message_id: message_id.clone(),
        assets,
    })
}

pub(super) async fn download_attachment(
    state: &AppState,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
    destination: Option<&std::path::Path>,
) -> HandlerResult {
    let file = match destination {
        Some(path) => {
            super::materialize_attachment_to_path(state, message_id, attachment_id, path).await?
        }
        None => materialize_attachment_file(state, message_id, attachment_id).await?,
    };
    Ok(ResponseData::AttachmentFile { file })
}

pub(super) async fn open_attachment(
    state: &AppState,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> HandlerResult {
    let file = materialize_attachment_file(state, message_id, attachment_id).await?;
    open_local_file(&file.path).map_err(|e| e.to_string())?;
    Ok(ResponseData::AttachmentFile { file })
}

fn collect_html_image_sources(html: &str) -> Vec<String> {
    static IMG_SELECTOR: std::sync::OnceLock<Selector> = std::sync::OnceLock::new();
    let selector = IMG_SELECTOR.get_or_init(|| Selector::parse("img").expect("valid img selector"));

    let document = Html::parse_fragment(html);
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    for image in document.select(selector) {
        let Some(src) = image.value().attr("src").map(str::trim) else {
            continue;
        };
        if src.is_empty() || !seen.insert(src.to_string()) {
            continue;
        }
        sources.push(src.to_string());
    }
    sources
}

async fn load_body_for_message(
    state: &AppState,
    message_id: &MessageId,
) -> Result<MessageBody, String> {
    let body = if let Some(body) = state
        .store
        .get_body(message_id)
        .await
        .map_err(|error| error.to_string())?
    {
        normalize_body_if_needed(state, message_id, body).await?
    } else {
        hydrate_body_from_provider(state, message_id).await?
    };
    enrich_calendar_viewer_fields(state, message_id, body).await
}

async fn load_cached_body_for_message(
    state: &AppState,
    message_id: &MessageId,
) -> Result<MessageBody, String> {
    let mut body = state
        .store
        .get_body(message_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Body not found in local store: {message_id}"))?;
    body.ensure_best_effort_readable();
    enrich_calendar_viewer_fields(state, message_id, body).await
}

/// Derive `viewer_partstat`, `viewer_attendee_email`, and `is_update` on the
/// body's `metadata.calendar` so every client (TUI, web SPA) renders the
/// invite card consistently without re-walking attendees or stored invites.
async fn enrich_calendar_viewer_fields(
    state: &AppState,
    message_id: &MessageId,
    mut body: MessageBody,
) -> Result<MessageBody, String> {
    let Some(calendar) = body.metadata.calendar.as_mut() else {
        return Ok(body);
    };

    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|err| err.to_string())?;
    let Some(envelope) = envelope else {
        return Ok(body);
    };

    let addresses = state
        .store
        .list_account_addresses(&envelope.account_id)
        .await
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(|a| a.email)
        .collect::<Vec<_>>();

    if let Some((email, partstat)) = mxr_mail_parse::matching_attendee_lenient(calendar, &addresses)
        .map(|a| {
            (
                a.email.clone(),
                a.partstat.as_deref().and_then(CalendarPartstat::parse),
            )
        })
    {
        calendar.viewer_attendee_email = Some(email);
        calendar.viewer_partstat = partstat;
    }

    if let Some(uid) = calendar.uid.as_deref() {
        let earlier = state
            .store
            .calendar_invite_has_earlier_sequence(
                &envelope.account_id,
                message_id,
                uid,
                calendar.recurrence_id.as_deref(),
                calendar.sequence,
            )
            .await
            .map_err(|err| err.to_string())?;
        calendar.is_update = earlier;
    }

    Ok(body)
}

async fn normalize_body_if_needed(
    state: &AppState,
    message_id: &MessageId,
    body: MessageBody,
) -> Result<MessageBody, String> {
    let envelope = if stored_body_needs_provider_repair(&body, None) {
        state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|error| error.to_string())?
    } else {
        None
    };

    if !stored_body_needs_provider_repair(&body, envelope.as_ref()) {
        return Ok(body);
    }

    if stored_body_has_authoritative_best_effort_summary(&body) {
        let Some(envelope) = envelope else {
            let mut normalized = body;
            normalized.ensure_best_effort_readable();
            return Ok(normalized);
        };

        return state
            .sync_engine
            .repair_body(&envelope, &body)
            .await
            .map_err(|error| error.to_string());
    }

    if let Ok(rehydrated) = hydrate_body_from_provider(state, message_id).await {
        return Ok(rehydrated);
    }

    let Some(envelope) = envelope else {
        let mut normalized = body;
        normalized.mark_best_effort_summary_source();
        normalized.ensure_best_effort_readable();
        return Ok(normalized);
    };

    let mut normalized = body;
    normalized.mark_best_effort_summary_source();

    state
        .sync_engine
        .repair_body(&envelope, &normalized)
        .await
        .map_err(|error| error.to_string())
}

fn stored_body_needs_provider_repair(body: &MessageBody, envelope: Option<&Envelope>) -> bool {
    if body.text_html.is_some() {
        return false;
    }

    if body.text_plain.is_none() || body.is_legacy_best_effort_plain_summary() {
        return true;
    }

    if body.metadata.text_plain_source != Some(BodyPartSource::BestEffortSummary) {
        return false;
    }

    best_effort_summary_looks_suspicious(body, envelope)
}

fn stored_body_has_authoritative_best_effort_summary(body: &MessageBody) -> bool {
    body.text_plain.is_none() && body.text_html.is_none() && body.metadata.calendar.is_some()
}

fn best_effort_summary_looks_suspicious(body: &MessageBody, envelope: Option<&Envelope>) -> bool {
    let snippet_suggests_body = envelope.is_some_and(|envelope| {
        let snippet = envelope.snippet.trim();
        !snippet.is_empty()
            && body
                .text_plain
                .as_deref()
                .is_none_or(|summary| !summary.contains(snippet))
    });

    if snippet_suggests_body {
        return true;
    }

    let Some(raw_headers) = body.metadata.raw_headers.as_deref() else {
        return false;
    };
    let raw_headers = raw_headers.to_ascii_lowercase();
    raw_headers.contains("content-type: multipart/alternative")
        || raw_headers.contains("content-type: text/plain")
        || raw_headers.contains("content-type: text/html")
}

async fn hydrate_body_from_provider(
    state: &AppState,
    message_id: &MessageId,
) -> Result<MessageBody, String> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Envelope not found: {message_id}"))?;
    let provider = state
        .sync_provider_for_account(&envelope.account_id)
        .ok_or_else(|| {
            format!(
                "Sync provider not found for account {}",
                envelope.account_id
            )
        })?;
    let synced = provider
        .fetch_message(&envelope.provider_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Body not available for {message_id}"))?;

    if synced.envelope.id != envelope.id {
        return Err(format!(
            "Provider body hydrate returned mismatched message id for {message_id}"
        ));
    }

    state
        .sync_engine
        .persist_synced_message(&synced)
        .await
        .map_err(|error| error.to_string())?;

    let mut body = synced.body;
    body.ensure_best_effort_readable();
    Ok(body)
}

async fn resolve_html_image_asset(
    state: &AppState,
    message_id: &MessageId,
    body: &MessageBody,
    source: &str,
    allow_remote: bool,
) -> HtmlImageAsset {
    let (kind, normalized_source) = classify_html_image_source(source);

    match kind {
        HtmlImageSourceKind::DataUri => {
            match materialize_data_uri_asset(state, message_id, normalized_source.as_str()).await {
                Ok((path, mime_type)) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Ready,
                    mime_type,
                    path: Some(path),
                    detail: None,
                },
                Err(detail) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: None,
                    path: None,
                    detail: Some(detail),
                },
            }
        }
        HtmlImageSourceKind::Remote => {
            if !allow_remote {
                return HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Blocked,
                    mime_type: None,
                    path: None,
                    detail: Some("remote content disabled".into()),
                };
            }

            match materialize_remote_asset(state, message_id, normalized_source.as_str()).await {
                Ok((path, mime_type)) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Ready,
                    mime_type,
                    path: Some(path),
                    detail: None,
                },
                Err(detail) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: None,
                    path: None,
                    detail: Some(detail),
                },
            }
        }
        HtmlImageSourceKind::Cid
        | HtmlImageSourceKind::ContentLocation
        | HtmlImageSourceKind::File => {
            let attachment =
                match find_html_image_attachment(body, normalized_source.as_str(), kind) {
                    Some(attachment) => attachment,
                    None => {
                        return HtmlImageAsset {
                            source: source.to_string(),
                            kind,
                            status: HtmlImageAssetStatus::Missing,
                            mime_type: None,
                            path: None,
                            detail: Some("no matching inline attachment".into()),
                        };
                    }
                };

            match materialize_attachment_file(state, message_id, &attachment.id).await {
                Ok(file) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Ready,
                    mime_type: Some(attachment.mime_type.clone()),
                    path: Some(PathBuf::from(file.path)),
                    detail: None,
                },
                Err(error) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: Some(attachment.mime_type.clone()),
                    path: None,
                    detail: Some(error.to_string()),
                },
            }
        }
    }
}

fn classify_html_image_source(source: &str) -> (HtmlImageSourceKind, String) {
    let trimmed = source.trim();
    if trimmed.starts_with("data:") {
        return (HtmlImageSourceKind::DataUri, trimmed.to_string());
    }
    if let Some(remote) = normalize_remote_source(trimmed) {
        return (HtmlImageSourceKind::Remote, remote);
    }
    if let Some(cid) = trimmed.strip_prefix("cid:") {
        return (HtmlImageSourceKind::Cid, normalize_content_id(cid));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return (HtmlImageSourceKind::ContentLocation, trimmed.to_string());
    }
    (HtmlImageSourceKind::File, trimmed.to_string())
}

fn normalize_remote_source(source: &str) -> Option<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        return Some(source.to_string());
    }
    source
        .strip_prefix("//")
        .map(|rest| format!("https://{rest}"))
}

fn normalize_content_id(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

fn find_html_image_attachment<'a>(
    body: &'a MessageBody,
    source: &str,
    kind: HtmlImageSourceKind,
) -> Option<&'a mxr_core::types::AttachmentMeta> {
    match kind {
        HtmlImageSourceKind::Cid => body.attachments.iter().find(|attachment| {
            attachment
                .content_id
                .as_deref()
                .map(normalize_content_id)
                .is_some_and(|content_id| content_id.eq_ignore_ascii_case(source))
        }),
        HtmlImageSourceKind::ContentLocation => body.attachments.iter().find(|attachment| {
            attachment
                .content_location
                .as_deref()
                .is_some_and(|location| locations_match(location, source))
                || filename_matches(&attachment.filename, source)
        }),
        HtmlImageSourceKind::File => body
            .attachments
            .iter()
            .find(|attachment| filename_matches(&attachment.filename, source)),
        HtmlImageSourceKind::DataUri | HtmlImageSourceKind::Remote => None,
    }
}

fn locations_match(left: &str, right: &str) -> bool {
    left == right
        || normalize_location_tail(left)
            .zip(normalize_location_tail(right))
            .is_some_and(|(left_tail, right_tail)| left_tail.eq_ignore_ascii_case(&right_tail))
}

fn normalize_location_tail(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        return url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .map(std::string::ToString::to_string);
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|segment| segment.to_str())
        .map(std::string::ToString::to_string)
}

fn filename_matches(filename: &str, source: &str) -> bool {
    Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(source))
}

async fn materialize_data_uri_asset(
    state: &AppState,
    message_id: &MessageId,
    source: &str,
) -> Result<(PathBuf, Option<String>), String> {
    let data_url = data_url::DataUrl::process(source).map_err(|error| error.to_string())?;
    let mime_type = Some(data_url.mime_type().to_string());
    let bytes = match data_url.decode_to_vec() {
        Ok((bytes, _)) => bytes,
        Err(error) => decode_base64_data_uri_fallback(source).map_err(|fallback_error| {
            format!("{error}; fallback decode failed: {fallback_error}")
        })?,
    };

    let target_dir = html_image_dir(state, message_id).join("data");
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|error| error.to_string())?;
    let path = target_dir
        .join(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, source.as_bytes()).to_string());
    if !path.exists() {
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|error| error.to_string())?;
        set_private_file_permissions(&path)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok((path, mime_type))
}

fn decode_base64_data_uri_fallback(source: &str) -> Result<Vec<u8>, String> {
    let (header, body) = source
        .split_once(',')
        .ok_or_else(|| "data url is missing body delimiter".to_string())?;
    if !header.to_ascii_lowercase().contains(";base64") {
        return Err("fallback only supports base64 data urls".into());
    }

    let encoded = body
        .split('#')
        .next()
        .unwrap_or(body)
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();

    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&encoded))
        .map_err(|error| error.to_string())
}

async fn materialize_remote_asset(
    state: &AppState,
    message_id: &MessageId,
    source: &str,
) -> Result<(PathBuf, Option<String>), String> {
    let target_dir = html_image_dir(state, message_id).join("remote");
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|error| error.to_string())?;
    let path = target_dir
        .join(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, source.as_bytes()).to_string());
    if path.exists() {
        return Ok((path, None));
    }

    let response = reqwest::Client::new()
        .get(source)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("remote image fetch failed: {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_REMOTE_ASSET_BYTES)
    {
        return Err(format!(
            "remote image exceeds {} byte limit",
            MAX_REMOTE_ASSET_BYTES
        ));
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string());
    let bytes = read_capped_remote_asset(response).await?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| error.to_string())?;
    set_private_file_permissions(&path)
        .await
        .map_err(|error| error.to_string())?;
    Ok((path, mime_type))
}

async fn read_capped_remote_asset(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        let next_len = bytes.len().saturating_add(chunk.len());
        if next_len as u64 > MAX_REMOTE_ASSET_BYTES {
            return Err(format!(
                "remote image exceeds {} byte limit",
                MAX_REMOTE_ASSET_BYTES
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(unix)]
async fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await
}

#[cfg(not(unix))]
async fn set_private_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn html_image_dir(state: &AppState, message_id: &MessageId) -> PathBuf {
    state
        .attachment_dir()
        .join("_html_assets")
        .join(message_id.as_str())
}

pub(super) async fn list_bodies(state: &AppState, message_ids: &[MessageId]) -> HandlerResult {
    tracing::debug!(count = message_ids.len(), "ListBodies: fetching bodies");
    let mut bodies = Vec::with_capacity(message_ids.len());
    let mut failures = Vec::new();
    for id in message_ids {
        match load_cached_body_for_message(state, id).await {
            Ok(body) => bodies.push(body),
            Err(error) => {
                tracing::debug!(message_id = %id, error = %error, "ListBodies: body unavailable");
                failures.push(BodyFailure {
                    message_id: id.clone(),
                    error,
                });
            }
        }
    }
    Ok(ResponseData::Bodies { bodies, failures })
}

pub(super) async fn get_thread(state: &AppState, thread_id: &ThreadId) -> HandlerResult {
    let thread = state
        .store
        .get_thread(thread_id)
        .await?
        .ok_or_else(|| "Thread not found".to_string())?;
    let mut messages = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .unwrap_or_default();
    if let Ok(labels) = state.store.list_labels_by_account(&thread.account_id).await {
        for message in &mut messages {
            let _ = populate_envelope_label_provider_ids(state, message, &labels).await;
        }
    }
    let summary = super::summarize::valid_cached_summary(&state.store, thread_id, &messages).await;
    Ok(ResponseData::Thread {
        thread,
        messages,
        summary,
    })
}

pub(super) async fn list_threads(
    state: &AppState,
    account_id: Option<&AccountId>,
    label_id: Option<&LabelId>,
    limit: u32,
    offset: u32,
    sort: Option<mxr_core::types::SortOrder>,
) -> HandlerResult {
    let sort = sort.unwrap_or(mxr_core::types::SortOrder::DateDesc);
    let threads = state
        .store
        .list_threads(account_id, label_id, limit, offset, sort)
        .await?;
    Ok(ResponseData::Threads { threads })
}

pub(super) async fn list_labels(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let Some(account_id) = resolve_local_account_id(state, account_id).await? else {
        return Ok(ResponseData::Labels { labels: Vec::new() });
    };
    let labels = state.store.list_labels_by_account(&account_id).await?;
    Ok(ResponseData::Labels { labels })
}

pub(super) async fn create_label(
    state: &AppState,
    name: &str,
    color: Option<&str>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let provider = state.get_provider(Some(&account_id))?;
    let mut label = provider.create_label(name, color).await?;
    if label.account_id != account_id {
        label.account_id = account_id;
    }
    state.store.upsert_label(&label).await?;
    Ok(ResponseData::Label { label })
}

pub(super) async fn delete_label(
    state: &AppState,
    name: &str,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let label = find_label_by_name(state, &account_id, name).await?;
    let provider = state.get_provider(Some(&account_id))?;
    provider.delete_label(&label.provider_id).await?;
    state.store.delete_label(&label.id).await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn rename_label(
    state: &AppState,
    old: &str,
    new: &str,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let existing = find_label_by_name(state, &account_id, old).await?;
    let provider = state.get_provider(Some(&account_id))?;
    let mut label = provider.rename_label(&existing.provider_id, new).await?;
    if label.account_id != account_id {
        label.account_id = account_id.clone();
    }
    state.store.replace_label(&existing.id, &label).await?;
    Ok(ResponseData::Label { label })
}
