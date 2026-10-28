use super::envelope_list::{
    list_envelopes, list_envelopes_by_message_ids, run_saved_search, search_envelopes, slugify,
    sorted_saved_searches,
};
use super::*;
use mxr_core::{
    Label, LabelKind, MessageFlags, PersonalCategory, Role, SavedSearch, SubscriptionSummary,
};
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct BridgeChrome {
    // Web-specific shaping stays here. The daemon returns reusable runtime data;
    // the bridge assembles shell/sidebar JSON for this client.
    pub(crate) shell: serde_json::Value,
    pub(crate) sidebar: serde_json::Value,
    pub(crate) labels: Vec<Label>,
    pub(crate) categories: Vec<PersonalCategory>,
    pub(crate) inbox_label_id: Option<mxr_core::LabelId>,
    pub(crate) searches: Vec<SavedSearch>,
    pub(crate) subscriptions: Vec<SubscriptionSummary>,
    /// Every system label of every account in scope: all enabled accounts'
    /// without an account scope, the named account's otherwise. `labels`
    /// holds only the default (or named) account's, for user labels.
    pub(crate) scope_labels: Vec<Label>,
    /// Unread and total per system mailbox over the accounts in scope, from
    /// the daemon. Empty when the daemon predates `SystemMailboxCounts`, so
    /// callers fall back to the label's own counts.
    pub(crate) unified_counts: HashMap<Role, (u32, u32)>,
    /// All messages across accounts, used for All Mail only when the daemon
    /// sends no counts and no account is chosen.
    pub(crate) all_accounts_total: Option<u32>,
}

impl BridgeChrome {
    /// The label a system mailbox is reached through: the default (or named)
    /// account's, else any in-scope account's. Navigation is built from
    /// roles, not from one account's label names.
    pub(crate) fn mailbox_label(&self, role: Role) -> Option<&Label> {
        self.labels
            .iter()
            .chain(self.scope_labels.iter())
            .find(|label| label.system_mailbox_role() == Some(role))
    }

    /// A label by id from any account in scope.
    pub(crate) fn label_by_id(&self, id: &mxr_core::LabelId) -> Option<&Label> {
        self.labels
            .iter()
            .chain(self.scope_labels.iter())
            .find(|label| &label.id == id)
    }

    /// Counts for a system mailbox: the daemon's sum over the accounts in
    /// scope, else the label's own. `None` when neither is known.
    pub(crate) fn role_counts(&self, role: Role) -> Option<(u32, u32)> {
        self.unified_counts.get(&role).copied().or_else(|| {
            self.mailbox_label(role)
                .map(|label| (label.unread_count, label.total_count))
        })
    }

    /// Counts to show for `label`: the mailbox's counts for a system label,
    /// the label's own otherwise.
    pub(crate) fn label_counts(&self, label: &Label) -> (u32, u32) {
        label
            .system_mailbox_role()
            .and_then(|role| self.unified_counts.get(&role).copied())
            .unwrap_or((label.unread_count, label.total_count))
    }
}

/// `label_role` for listing `label` when no account is chosen: its system
/// mailbox across every account. `None` with an account (the label id is
/// already that account's) or for user labels.
pub(crate) fn unscoped_label_role(
    account_id: Option<&AccountId>,
    label: Option<&Label>,
) -> Option<String> {
    if account_id.is_some() {
        return None;
    }
    label?
        .system_mailbox_role()
        .and_then(Role::system_mailbox_wire_name)
        .map(str::to_string)
}

#[derive(Debug, Serialize)]
pub(crate) struct MessageLabelView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    pub(crate) color: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MessageRowView {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) account_id: String,
    pub(crate) thread_id: String,
    pub(crate) provider_id: String,
    pub(crate) sender: String,
    pub(crate) sender_detail: Option<String>,
    pub(crate) subject: String,
    pub(crate) snippet: String,
    pub(crate) date: DateTime<Utc>,
    pub(crate) date_label: String,
    pub(crate) date_full: String,
    pub(crate) date_relative: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) to: Vec<mxr_core::Address>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) cc: Vec<mxr_core::Address>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) bcc: Vec<mxr_core::Address>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) labels: Vec<MessageLabelView>,
    pub(crate) unread: bool,
    pub(crate) starred: bool,
    pub(crate) has_attachments: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message_count: Option<u32>,
    /// Thread rows only: ids of the thread's messages in the current lens,
    /// oldest first, so a thread-level action can target all of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message_ids: Option<Vec<String>>,
    /// Thread rows only: ids of every starred message in the thread, in or
    /// out of the lens (a starred reply in Sent). `starred` is true when any
    /// is, so unstarring the row clears exactly these.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) starred_message_ids: Option<Vec<String>>,
    /// Thread rows only: distinct senders in the thread, capped at
    /// `THREAD_ROW_PARTICIPANT_LIMIT`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) participants: Option<Vec<mxr_core::Address>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attachment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attachment_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attachment_size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) open_commitment_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) triage_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) triage_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) triage_line: Option<String>,
}

pub(crate) const THREAD_ROW_PARTICIPANT_LIMIT: usize = 5;

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

/// The two lines of chrome the shell shows about sync: a short label and a
/// sentence.
///
/// A degraded status snapshot carries no sync statuses and no message count at
/// all — the daemon answered without reading its database. Every label derived
/// from those fields would then read as "nothing is happening, zero messages",
/// so say the daemon is busy rather than inventing an all-clear.
pub(crate) fn describe_sync_state(
    degraded: bool,
    repair_required: bool,
    sync_statuses: &[mxr_protocol::AccountSyncStatus],
) -> (&'static str, &'static str) {
    if degraded {
        return (
            "Daemon busy",
            "Daemon could not read the database in time — counts are unknown, not empty",
        );
    }
    let label = if sync_statuses.iter().any(|status| status.sync_in_progress) {
        "Syncing"
    } else if sync_statuses
        .iter()
        .any(|status| !status.healthy || status.last_error.is_some())
    {
        "Needs attention"
    } else {
        "Synced"
    };
    let message = if repair_required {
        "Repair required before mailbox opens"
    } else if sync_statuses
        .iter()
        .any(|status| status.last_error.is_some())
    {
        "Last sync needs attention"
    } else {
        "Local-first and ready"
    };
    (label, message)
}

/// `account_id` scopes labels and subscriptions to one account; `None` keeps
/// the daemon default (its default account's labels).
pub(crate) async fn build_bridge_chrome(
    socket_path: &Path,
    active_lens: &MailboxLensRequest,
    account_id: Option<&AccountId>,
) -> Result<BridgeChrome, BridgeError> {
    let (accounts, total_messages, sync_statuses, repair_required, degraded) =
        match ipc_request(socket_path, Request::GetStatus).await? {
            ResponseData::Status {
                accounts,
                total_messages,
                sync_statuses,
                repair_required,
                degraded,
                ..
            } => (
                accounts,
                total_messages,
                sync_statuses,
                repair_required,
                degraded,
            ),
            _ => return Err(BridgeError::UnexpectedResponse),
        };

    let labels = match ipc_request(
        socket_path,
        Request::ListLabels {
            account_id: account_id.cloned(),
            all_accounts: false,
        },
    )
    .await?
    {
        ResponseData::Labels { labels } => labels,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    // Navigation is built from roles, so it needs every in-scope account's
    // system labels, not just the default account's. An older daemon that
    // ignores `all_accounts` returns the default account's, which is the
    // old behaviour.
    let scope_labels = if account_id.is_none() {
        match ipc_request(
            socket_path,
            Request::ListLabels {
                account_id: None,
                all_accounts: true,
            },
        )
        .await?
        {
            ResponseData::Labels { labels } => labels,
            _ => return Err(BridgeError::UnexpectedResponse),
        }
    } else {
        labels.clone()
    };

    // Counts come from the daemon so All Mail and Starred (no label on some
    // providers) and accounts without a given label still add up. A daemon
    // that predates the request errors; the label's own counts stand in.
    let unified_counts = match ipc_request(
        socket_path,
        Request::SystemMailboxCounts {
            account_id: account_id.cloned(),
        },
    )
    .await
    {
        Ok(ResponseData::SystemMailboxCounts { mailboxes }) => mailboxes
            .into_iter()
            .filter_map(|mailbox| {
                Some((
                    Role::from_system_mailbox_wire_name(&mailbox.role)?,
                    (mailbox.unread, mailbox.total),
                ))
            })
            .collect::<HashMap<_, _>>(),
        Ok(_) => return Err(BridgeError::UnexpectedResponse),
        Err(error) => {
            tracing::debug!(%error, "system mailbox counts unavailable; using label counts");
            HashMap::new()
        }
    };

    let categories = match ipc_request(
        socket_path,
        Request::ListCategories {
            account_id: account_id.cloned(),
        },
    )
    .await?
    {
        ResponseData::Categories { categories } => categories,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let searches = match ipc_request(socket_path, Request::ListSavedSearches).await? {
        ResponseData::SavedSearches { searches } => searches,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let subscriptions = match ipc_request(
        socket_path,
        Request::ListSubscriptions {
            account_id: account_id.cloned(),
            limit: 8,
        },
    )
    .await?
    {
        ResponseData::Subscriptions { subscriptions } => subscriptions,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let (sync_label, status_message) =
        describe_sync_state(degraded, repair_required, &sync_statuses);

    Ok(BridgeChrome {
        shell: json!({
            "accountLabel": accounts.first().cloned().unwrap_or_else(|| "local".to_string()),
            "syncLabel": sync_label,
            "statusMessage": status_message,
            "commandHint": "Ctrl-p",
        }),
        sidebar: json!({ "sections": build_sidebar_sections(&labels, &scope_labels, &categories, &searches, &subscriptions, total_messages, active_lens, &unified_counts) }),
        inbox_label_id: inbox_label_id(&labels, &scope_labels),
        labels,
        categories,
        searches,
        subscriptions,
        scope_labels,
        unified_counts,
        all_accounts_total: account_id.is_none().then_some(total_messages),
    })
}

pub(crate) async fn ack_mutation(
    socket_path: &Path,
    mutation: mxr_protocol::MutationCommand,
) -> Result<Json<serde_json::Value>, BridgeError> {
    match ipc_request(socket_path, Request::mutation(mutation)).await? {
        ResponseData::Ack => Ok(Json(serde_json::json!({ "ok": true }))),
        ResponseData::MutationResult { result } => Ok(Json(serde_json::json!({
            "ok": result.succeeded == result.requested && result.skipped == 0 && result.failed == 0,
            "result": result,
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
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

/// The Inbox label to send as `label_id`: the default (or named) account's,
/// else any in-scope account's.
fn inbox_label_id(labels: &[Label], scope_labels: &[Label]) -> Option<mxr_core::LabelId> {
    labels
        .iter()
        .chain(scope_labels)
        .find(|label| label.system_mailbox_role() == Some(Role::Inbox))
        .map(|label| label.id.clone())
}

pub(crate) fn mailbox_counts(chrome: &BridgeChrome, envelopes: &[Envelope]) -> serde_json::Value {
    if let Some((unread, total)) = chrome.role_counts(Role::Inbox) {
        json!({
            "unread": unread,
            "total": total,
        })
    } else {
        derived_counts(envelopes)
    }
}

/// Counts for the All Mail lens: the daemon's per-account message counts.
/// Never the page size. Without them (an older daemon), the account's All
/// Mail label if it has one, else all messages across accounts when no
/// account is chosen.
fn all_mail_counts(chrome: &BridgeChrome) -> serde_json::Value {
    let (unread, total) = chrome
        .role_counts(Role::AllMail)
        .unwrap_or((0, chrome.all_accounts_total.unwrap_or(0)));
    json!({ "unread": unread, "total": total })
}

/// A system label shows under its mailbox's title (`DRAFT` reads "Drafts");
/// other labels keep their own name.
pub(crate) fn lens_label_name(label: &Label) -> String {
    label
        .system_mailbox_role()
        .and_then(Role::system_mailbox_title)
        .map_or_else(|| label.name.clone(), str::to_string)
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

#[allow(
    clippy::too_many_arguments,
    reason = "one call site assembles the sidebar"
)]
pub(crate) fn build_sidebar_sections(
    labels: &[Label],
    scope_labels: &[Label],
    categories: &[PersonalCategory],
    searches: &[SavedSearch],
    subscriptions: &[SubscriptionSummary],
    total_messages: u32,
    active_lens: &MailboxLensRequest,
    unified_counts: &HashMap<Role, (u32, u32)>,
) -> Vec<serde_json::Value> {
    // The label a role is reached through: the default (or named) account's,
    // else any in-scope account's. A mailbox only another account has still
    // gets an item.
    let label_for = |role: Role| {
        labels
            .iter()
            .chain(scope_labels)
            .find(|label| label.system_mailbox_role() == Some(role))
    };
    let counts_for = |role: Role, label: Option<&Label>| {
        unified_counts
            .get(&role)
            .copied()
            .or_else(|| label.map(|label| (label.unread_count, label.total_count)))
    };
    let (all_mail_unread, all_mail_total) =
        counts_for(Role::AllMail, label_for(Role::AllMail)).unwrap_or((0, total_messages));

    let mut system_items = Vec::new();
    for role in [
        Role::Inbox,
        Role::Starred,
        Role::Sent,
        Role::Drafts,
        Role::Spam,
        Role::Trash,
    ] {
        let Some(title) = role.system_mailbox_title() else {
            continue;
        };
        if let Some(label) = label_for(role) {
            let (unread, total) = counts_for(role, Some(label)).unwrap_or_default();
            let is_inbox = role == Role::Inbox;
            system_items.push(json!({
                "id": slugify(title),
                "label": title,
                "unread": unread,
                "total": total,
                "active": active_lens.kind == MailboxLensKind::Label
                    && active_lens.label_id.as_deref() == Some(&label.id.to_string())
                    || active_lens.kind == MailboxLensKind::Inbox && is_inbox,
                "lens": {
                    "kind": if is_inbox { "inbox" } else { "label" },
                    "labelId": if is_inbox {
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
    if !categories.is_empty() {
        sections.push(json!({
            "id": "categories",
            "title": "Categories",
            "items": categories.iter().map(|category| json!({
                "id": category.id.to_string(),
                "label": &category.name,
                "unread": category.unread_count,
                "total": category.total_count,
                "active": active_lens.kind == MailboxLensKind::Category && active_lens.category_id.as_deref() == Some(&category.id.to_string()),
                "lens": { "kind": "category", "categoryId": category.id.to_string() },
            })).collect::<Vec<_>>(),
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
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> Result<MailboxSelection, BridgeError> {
    match lens.kind {
        MailboxLensKind::Inbox => {
            let envelopes = list_envelopes(
                socket_path,
                chrome.inbox_label_id.clone(),
                account_id,
                limit,
                offset,
                account_id
                    .is_none()
                    .then(|| Role::Inbox.system_mailbox_wire_name().map(str::to_string))
                    .flatten(),
            )
            .await?;
            Ok(MailboxSelection {
                lens_label: "Inbox".to_string(),
                counts: mailbox_counts(chrome, &envelopes),
                envelopes,
            })
        }
        MailboxLensKind::AllMail => {
            let envelopes = list_envelopes(
                socket_path,
                None,
                account_id,
                limit,
                offset,
                account_id
                    .is_none()
                    .then(|| Role::AllMail.system_mailbox_wire_name().map(str::to_string))
                    .flatten(),
            )
            .await?;
            let counts = all_mail_counts(chrome);
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
                .ok_or_else(|| BridgeError::BadRequest("label lens missing label_id".into()))
                .and_then(parse_label_id)?;
            let label = chrome.label_by_id(&label_id);
            let envelopes = list_envelopes(
                socket_path,
                Some(label_id.clone()),
                account_id,
                limit,
                offset,
                unscoped_label_role(account_id, label),
            )
            .await?;
            Ok(MailboxSelection {
                lens_label: label.map_or_else(|| "Label".to_string(), lens_label_name),
                counts: label.map_or_else(
                    || derived_counts(&envelopes),
                    |label| {
                        let (unread, total) = chrome.label_counts(label);
                        json!({
                            "unread": unread,
                            "total": total,
                        })
                    },
                ),
                envelopes,
            })
        }
        MailboxLensKind::Category => {
            let category_id: mxr_core::CategoryId = lens
                .category_id
                .as_deref()
                .ok_or_else(|| BridgeError::BadRequest("category lens missing category_id".into()))?
                .parse()
                .map_err(|_| BridgeError::BadRequest("invalid category_id".into()))?;
            let data = ipc_request(
                socket_path,
                Request::ListCategoryMessages {
                    category_id: category_id.clone(),
                    account_id: account_id.cloned(),
                    limit,
                    offset,
                },
            )
            .await?;
            let (envelopes, total) = match data {
                ResponseData::CategoryMessages {
                    envelopes, total, ..
                } => (envelopes, total),
                _ => return Err(BridgeError::UnexpectedResponse),
            };
            let category = chrome
                .categories
                .iter()
                .find(|item| item.id == category_id)
                .ok_or_else(|| {
                    BridgeError::BadRequest("category not found in this account".into())
                })?;
            Ok(MailboxSelection {
                lens_label: category.name.clone(),
                counts: json!({ "unread": category.unread_count, "total": total }),
                envelopes,
            })
        }
        MailboxLensKind::SavedSearch => {
            let name = lens.saved_search.as_deref().ok_or_else(|| {
                BridgeError::BadRequest("saved search lens missing saved_search".into())
            })?;
            let envelopes = run_saved_search(socket_path, name, account_id, limit).await?;
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
                // A subscription drilldown is a sender search; `Request::Search`
                // supports offset, so this lens paginates (unlike the saved-search
                // and subscription-overview lenses, whose IPC variants take no
                // offset).
                let envelopes =
                    search_envelopes(socket_path, sender_email, account_id, limit, offset).await?;
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
