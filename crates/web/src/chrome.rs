use super::envelope_list::{
    list_envelopes, list_envelopes_by_message_ids, run_saved_search, search_envelopes, slugify,
    sorted_saved_searches,
};
use super::*;
use crate::mxr_core::{LabelKind, MessageFlags, SavedSearch, SubscriptionSummary};
use serde_json::json;

#[derive(Debug)]
pub(crate) struct BridgeChrome {
    // Web-specific shaping stays here. The daemon returns reusable runtime data;
    // the bridge assembles shell/sidebar JSON for this client.
    pub(crate) shell: serde_json::Value,
    pub(crate) sidebar: serde_json::Value,
    pub(crate) labels: Vec<Label>,
    pub(crate) inbox_label_id: Option<crate::mxr_core::LabelId>,
    pub(crate) searches: Vec<SavedSearch>,
    pub(crate) subscriptions: Vec<SubscriptionSummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MessageRowView {
    pub(crate) id: String,
    pub(crate) thread_id: String,
    pub(crate) provider_id: String,
    pub(crate) sender: String,
    pub(crate) sender_detail: Option<String>,
    pub(crate) subject: String,
    pub(crate) snippet: String,
    pub(crate) date_label: String,
    pub(crate) unread: bool,
    pub(crate) starred: bool,
    pub(crate) has_attachments: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct MessageGroupView {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) rows: Vec<MessageRowView>,
}

pub(crate) struct MailboxSelection {
    pub(crate) lens_label: String,
    pub(crate) counts: serde_json::Value,
    pub(crate) envelopes: Vec<Envelope>,
}

pub(crate) async fn build_bridge_chrome(
    socket_path: &Path,
    active_lens: &MailboxLensRequest,
) -> Result<BridgeChrome, BridgeError> {
    let (accounts, total_messages, sync_statuses, repair_required) =
        match ipc_request(socket_path, Request::GetStatus).await? {
            ResponseData::Status {
                accounts,
                total_messages,
                sync_statuses,
                repair_required,
                ..
            } => (accounts, total_messages, sync_statuses, repair_required),
            _ => return Err(BridgeError::UnexpectedResponse),
        };

    let labels = match ipc_request(socket_path, Request::ListLabels { account_id: None }).await? {
        ResponseData::Labels { labels } => labels,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let searches = match ipc_request(socket_path, Request::ListSavedSearches).await? {
        ResponseData::SavedSearches { searches } => searches,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let subscriptions = match ipc_request(
        socket_path,
        Request::ListSubscriptions {
            account_id: None,
            limit: 8,
        },
    )
    .await?
    {
        ResponseData::Subscriptions { subscriptions } => subscriptions,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let sync_label = if sync_statuses.iter().any(|status| status.sync_in_progress) {
        "Syncing"
    } else if sync_statuses
        .iter()
        .any(|status| !status.healthy || status.last_error.is_some())
    {
        "Needs attention"
    } else {
        "Synced"
    };

    let status_message = if repair_required {
        "Repair required before mailbox opens".to_string()
    } else if sync_statuses
        .iter()
        .any(|status| status.last_error.is_some())
    {
        "Last sync needs attention".to_string()
    } else {
        "Local-first and ready".to_string()
    };

    Ok(BridgeChrome {
        shell: json!({
            "accountLabel": accounts.first().cloned().unwrap_or_else(|| "local".to_string()),
            "syncLabel": sync_label,
            "statusMessage": status_message,
            "commandHint": "Ctrl-p",
        }),
        sidebar: json!({ "sections": build_sidebar_sections(&labels, &searches, &subscriptions, total_messages, active_lens) }),
        inbox_label_id: find_inbox_label(&labels).map(|label| label.id.clone()),
        labels,
        searches,
        subscriptions,
    })
}

pub(crate) async fn ack_mutation(
    socket_path: &Path,
    mutation: crate::mxr_protocol::MutationCommand,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ack_request(socket_path, Request::Mutation(mutation)).await
}

pub(crate) async fn ack_request(
    socket_path: &Path,
    request: Request,
) -> Result<Json<serde_json::Value>, BridgeError> {
    match ipc_request(socket_path, request).await? {
        ResponseData::Ack => Ok(Json(serde_json::json!({ "ok": true }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) fn find_inbox_label(labels: &[Label]) -> Option<&Label> {
    labels
        .iter()
        .find(|label| matches_system_label(label, "Inbox"))
}

pub(crate) fn matches_system_label(label: &Label, expected: &str) -> bool {
    matches!(label.kind, LabelKind::System) && label.name.eq_ignore_ascii_case(expected)
}

pub(crate) fn mailbox_counts(labels: &[Label], envelopes: &[Envelope]) -> serde_json::Value {
    if let Some(inbox) = find_inbox_label(labels) {
        json!({
            "unread": inbox.unread_count,
            "total": inbox.total_count,
        })
    } else {
        json!({
            "unread": envelopes
                .iter()
                .filter(|envelope| !envelope.flags.contains(MessageFlags::READ))
                .count(),
            "total": envelopes.len(),
        })
    }
}

pub(crate) fn derived_counts(envelopes: &[Envelope]) -> serde_json::Value {
    json!({
        "unread": envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(MessageFlags::READ))
            .count(),
        "total": envelopes.len(),
    })
}

pub(crate) fn build_sidebar_sections(
    labels: &[Label],
    searches: &[SavedSearch],
    subscriptions: &[SubscriptionSummary],
    total_messages: u32,
    active_lens: &MailboxLensRequest,
) -> Vec<serde_json::Value> {
    let all_mail_total = labels
        .iter()
        .find(|label| matches_system_label(label, "All Mail"))
        .map_or(total_messages, |label| label.total_count);
    let all_mail_unread = labels
        .iter()
        .find(|label| matches_system_label(label, "All Mail"))
        .map(|label| label.unread_count)
        .unwrap_or_default();

    let mut system_items = Vec::new();
    for name in ["Inbox", "Starred", "Sent", "Drafts", "Spam", "Trash"] {
        if let Some(label) = labels
            .iter()
            .find(|label| matches_system_label(label, name))
        {
            system_items.push(json!({
                "id": slugify(&label.name),
                "label": label.name,
                "unread": label.unread_count,
                "total": label.total_count,
                "active": active_lens.kind == MailboxLensKind::Label
                    && active_lens.label_id.as_deref() == Some(&label.id.to_string())
                    || active_lens.kind == MailboxLensKind::Inbox && name == "Inbox",
                "lens": {
                    "kind": if name == "Inbox" { "inbox" } else { "label" },
                    "labelId": if name == "Inbox" {
                        None::<String>
                    } else {
                        Some(label.id.to_string())
                    },
                },
            }));
        }
    }
    system_items.push(json!({
        "id": "all-mail",
        "label": "All Mail",
        "unread": all_mail_unread,
        "total": all_mail_total,
        "active": active_lens.kind == MailboxLensKind::AllMail,
        "lens": { "kind": "all_mail" },
    }));

    let user_labels = labels
        .iter()
        .filter(|label| !matches!(label.kind, LabelKind::System))
        .map(|label| {
            json!({
                "id": slugify(&label.name),
                "label": label.name,
                "unread": label.unread_count,
                "total": label.total_count,
                "active": active_lens.kind == MailboxLensKind::Label
                    && active_lens.label_id.as_deref() == Some(&label.id.to_string()),
                "lens": {
                    "kind": "label",
                    "labelId": label.id.to_string(),
                },
            })
        })
        .collect::<Vec<_>>();

    let saved_search_items = sorted_saved_searches(searches.to_vec())
        .into_iter()
        .map(|search| {
            json!({
                "id": format!("saved-search-{}", slugify(&search.name)),
                "label": search.name,
                "unread": 0,
                "total": 0,
                "active": active_lens.kind == MailboxLensKind::SavedSearch
                    && active_lens.saved_search.as_deref() == Some(search.name.as_str()),
                "lens": {
                    "kind": "saved_search",
                    "savedSearch": search.name,
                },
            })
        })
        .collect::<Vec<_>>();

    system_items.push(json!({
        "id": "subscriptions",
        "label": "Subscriptions",
        "unread": subscriptions
            .iter()
            .filter(|subscription| !subscription.latest_flags.contains(MessageFlags::READ))
            .count(),
        "total": subscriptions.len(),
        "active": active_lens.kind == MailboxLensKind::Subscription,
        "lens": { "kind": "subscription" },
    }));

    let mut sections = vec![json!({
        "id": "system",
        "title": "System",
        "items": system_items,
    })];
    if !user_labels.is_empty() {
        sections.push(json!({
            "id": "labels",
            "title": "Labels",
            "items": user_labels,
        }));
    }
    if !saved_search_items.is_empty() {
        sections.push(json!({
            "id": "saved-searches",
            "title": "Saved Searches",
            "items": saved_search_items,
        }));
    }
    sections
}

pub(crate) async fn load_mailbox_selection(
    socket_path: &Path,
    chrome: &BridgeChrome,
    lens: &MailboxLensRequest,
    limit: u32,
    offset: u32,
) -> Result<MailboxSelection, BridgeError> {
    match lens.kind {
        MailboxLensKind::Inbox => {
            let envelopes =
                list_envelopes(socket_path, chrome.inbox_label_id.clone(), limit, offset).await?;
            Ok(MailboxSelection {
                lens_label: find_inbox_label(&chrome.labels)
                    .map_or_else(|| "Inbox".to_string(), |label| label.name.clone()),
                counts: mailbox_counts(&chrome.labels, &envelopes),
                envelopes,
            })
        }
        MailboxLensKind::AllMail => {
            let envelopes = list_envelopes(socket_path, None, limit, offset).await?;
            let counts = chrome
                .labels
                .iter()
                .find(|label| matches_system_label(label, "All Mail"))
                .map_or_else(
                    || derived_counts(&envelopes),
                    |label| {
                        json!({
                            "unread": label.unread_count,
                            "total": label.total_count,
                        })
                    },
                );
            Ok(MailboxSelection {
                lens_label: "All Mail".to_string(),
                counts,
                envelopes,
            })
        }
        MailboxLensKind::Label => {
            let label_id = lens
                .label_id
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("label lens missing label_id".into()))
                .and_then(parse_label_id)?;
            let envelopes =
                list_envelopes(socket_path, Some(label_id.clone()), limit, offset).await?;
            let label = chrome
                .labels
                .iter()
                .find(|candidate| candidate.id == label_id);
            Ok(MailboxSelection {
                lens_label: label.map_or_else(|| "Label".to_string(), |label| label.name.clone()),
                counts: label.map_or_else(
                    || derived_counts(&envelopes),
                    |label| {
                        json!({
                            "unread": label.unread_count,
                            "total": label.total_count,
                        })
                    },
                ),
                envelopes,
            })
        }
        MailboxLensKind::SavedSearch => {
            let name = lens
                .saved_search
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("saved search lens missing saved_search".into()))?;
            let envelopes = run_saved_search(socket_path, name, limit).await?;
            Ok(MailboxSelection {
                lens_label: chrome
                    .searches
                    .iter()
                    .find(|search| search.name == name)
                    .map_or_else(|| name.to_string(), |search| search.name.clone()),
                counts: derived_counts(&envelopes),
                envelopes,
            })
        }
        MailboxLensKind::Subscription => {
            if let Some(sender_email) = lens.sender_email.as_deref() {
                let envelopes = search_envelopes(socket_path, sender_email, limit).await?;
                return Ok(MailboxSelection {
                    lens_label: chrome
                        .subscriptions
                        .iter()
                        .find(|subscription| subscription.sender_email == sender_email)
                        .and_then(|subscription| subscription.sender_name.clone())
                        .unwrap_or_else(|| sender_email.to_string()),
                    counts: derived_counts(&envelopes),
                    envelopes,
                });
            }

            let message_ids = chrome
                .subscriptions
                .iter()
                .take(limit as usize)
                .map(|subscription| subscription.latest_message_id.clone())
                .collect::<Vec<_>>();
            let envelopes = list_envelopes_by_message_ids(socket_path, &message_ids).await?;
            Ok(MailboxSelection {
                lens_label: "Subscriptions".to_string(),
                counts: json!({
                    "unread": chrome
                        .subscriptions
                        .iter()
                        .filter(|subscription| !subscription.latest_flags.contains(MessageFlags::READ))
                        .count(),
                    "total": chrome.subscriptions.len(),
                }),
                envelopes,
            })
        }
    }
}
