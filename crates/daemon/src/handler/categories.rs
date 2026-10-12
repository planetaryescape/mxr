use crate::state::AppState;
use mxr_core::{AccountId, CategoryId, MessageId};
use mxr_protocol::{CategoryDeletePreview, CategoryMembershipPreview, ResponseData};
use mxr_store::{CategoryMembershipChange, CategoryMembershipVersion};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const MAX_PREVIEW_MESSAGES: usize = 200;
const MAX_PREVIEW_CATEGORIES: usize = 40;
const PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);

enum CategoryPreview {
    Membership {
        account_id: Option<AccountId>,
        message_ids: Vec<MessageId>,
        versions: Vec<CategoryMembershipVersion>,
        add_category_ids: Vec<CategoryId>,
        remove_category_ids: Vec<CategoryId>,
        accounts: Vec<AccountId>,
    },
    Delete {
        category_id: CategoryId,
        name: String,
        versions: Vec<CategoryMembershipVersion>,
    },
}

struct TimedPreview {
    created: Instant,
    value: CategoryPreview,
}

#[derive(Default)]
pub(crate) struct CategoryPreviews(Mutex<HashMap<String, TimedPreview>>);

impl CategoryPreviews {
    fn insert(&self, value: CategoryPreview) -> String {
        let token = uuid::Uuid::now_v7().to_string();
        let mut entries = self.0.lock();
        entries.retain(|_, preview| preview.created.elapsed() < PREVIEW_TTL);
        while entries.len() >= 256 {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, preview)| preview.created)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
        entries.insert(
            token.clone(),
            TimedPreview {
                created: Instant::now(),
                value,
            },
        );
        token
    }

    fn take(&self, token: &str) -> Result<CategoryPreview, String> {
        let mut entries = self.0.lock();
        let expired = entries
            .get(token)
            .is_some_and(|preview| preview.created.elapsed() >= PREVIEW_TTL);
        if expired {
            entries.remove(token);
        }
        entries
            .remove(token)
            .map(|preview| preview.value)
            .ok_or_else(|| "category preview expired or was already used; preview again".into())
    }

    pub(crate) fn membership_scope_for_token(
        &self,
        token: &str,
    ) -> Result<(Vec<AccountId>, Vec<CategoryId>), String> {
        let entries = self.0.lock();
        let entry = entries
            .get(token)
            .ok_or_else(|| "category preview expired or was already used".to_string())?;
        if entry.created.elapsed() >= PREVIEW_TTL {
            return Err("category preview expired".into());
        }
        match &entry.value {
            CategoryPreview::Membership {
                accounts, versions, ..
            } => {
                let mut category_ids: Vec<_> = versions
                    .iter()
                    .map(|version| version.category_id.clone())
                    .collect();
                category_ids.sort_by_key(ToString::to_string);
                category_ids.dedup();
                Ok((accounts.clone(), category_ids))
            }
            CategoryPreview::Delete { .. } => Err("category delete preview is global".into()),
        }
    }
}

pub(crate) async fn list(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<ResponseData, String> {
    Ok(ResponseData::Categories {
        categories: state.store.list_personal_categories(account_id).await?,
    })
}

pub(crate) async fn list_messages(
    state: &AppState,
    category_id: &CategoryId,
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> Result<ResponseData, String> {
    state.store.get_personal_category(category_id).await?;
    let (message_ids, total) = state
        .store
        .list_category_message_ids(category_id, account_id, limit, offset)
        .await?;
    let envelopes = state
        .store
        .list_envelopes_by_ids(&message_ids)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::CategoryMessages {
        envelopes,
        total,
        has_more: offset.saturating_add(limit.min(500)) < total,
    })
}

pub(crate) async fn membership_states(
    state: &AppState,
    account_id: Option<&AccountId>,
    message_ids: &[MessageId],
    category_ids: &[CategoryId],
) -> Result<ResponseData, String> {
    if message_ids.is_empty() || message_ids.len() > MAX_PREVIEW_MESSAGES {
        return Err(format!(
            "select between 1 and {MAX_PREVIEW_MESSAGES} messages"
        ));
    }
    if message_ids.iter().collect::<HashSet<_>>().len() != message_ids.len() {
        return Err("message selection contains duplicates".into());
    }
    if category_ids.len() > MAX_PREVIEW_CATEGORIES {
        return Err(format!(
            "request at most {MAX_PREVIEW_CATEGORIES} categories"
        ));
    }
    Ok(ResponseData::CategoryMembershipStates {
        states: state
            .store
            .category_membership_states(account_id, message_ids, category_ids)
            .await?,
    })
}

pub(crate) async fn create(state: &AppState, name: &str) -> Result<ResponseData, String> {
    let _gate = state.rule_mutation_gate.lock().await;
    let category = state.store.create_personal_category(name).await?;
    Ok(ResponseData::Categories {
        categories: vec![category],
    })
}

pub(crate) async fn preview_name(
    state: &AppState,
    category_id: Option<&CategoryId>,
    name: &str,
) -> Result<ResponseData, String> {
    let (normalized_name, available) = state
        .store
        .preview_personal_category_name(category_id, name)
        .await?;
    Ok(ResponseData::CategoryNamePreview {
        normalized_name,
        available,
    })
}

pub(crate) async fn rename(
    state: &AppState,
    category_id: &CategoryId,
    new_name: &str,
) -> Result<ResponseData, String> {
    let _gate = state.rule_mutation_gate.lock().await;
    let category = state
        .store
        .rename_personal_category(category_id, new_name)
        .await?;
    Ok(ResponseData::Categories {
        categories: vec![category],
    })
}

pub(crate) async fn preview_membership(
    state: &AppState,
    account_id: Option<&AccountId>,
    message_ids: &[MessageId],
    add_category_ids: &[CategoryId],
    remove_category_ids: &[CategoryId],
) -> Result<ResponseData, String> {
    if message_ids.is_empty() || message_ids.len() > MAX_PREVIEW_MESSAGES {
        return Err(format!(
            "select between 1 and {MAX_PREVIEW_MESSAGES} messages"
        ));
    }
    if message_ids.iter().collect::<HashSet<_>>().len() != message_ids.len() {
        return Err("message selection contains duplicates".into());
    }
    let mut category_ids = add_category_ids.to_vec();
    category_ids.extend(remove_category_ids.iter().cloned());
    if add_category_ids.iter().collect::<HashSet<_>>().len() != add_category_ids.len()
        || remove_category_ids.iter().collect::<HashSet<_>>().len() != remove_category_ids.len()
    {
        return Err("category selection contains duplicates".into());
    }
    category_ids.sort_by_key(ToString::to_string);
    category_ids.dedup();
    if category_ids.is_empty() || category_ids.len() > MAX_PREVIEW_CATEGORIES {
        return Err(format!(
            "select between 1 and {MAX_PREVIEW_CATEGORIES} categories to change"
        ));
    }
    if add_category_ids
        .iter()
        .any(|id| remove_category_ids.contains(id))
    {
        return Err("a category cannot be both added and removed".into());
    }
    let versions = state
        .store
        .category_membership_versions(account_id, message_ids, &category_ids)
        .await?;
    let mut accounts = Vec::new();
    for message_id in message_ids {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("message not found: {message_id}"))?;
        if !accounts.contains(&envelope.account_id) {
            accounts.push(envelope.account_id);
        }
    }
    let (mut added, mut removed) = (0, 0);
    for version in &versions {
        let target = if add_category_ids.contains(&version.category_id) {
            Some(true)
        } else if remove_category_ids.contains(&version.category_id) {
            Some(false)
        } else {
            None
        };
        match target {
            Some(true) if !version.assigned => added += 1,
            Some(false) if version.assigned => removed += 1,
            _ => {}
        }
    }
    let effects = versions
        .iter()
        .map(|v| mxr_protocol::CategoryAssignmentEffect {
            message_id: v.message_id.clone(),
            category_id: v.category_id.clone(),
            before: v.assigned,
            after: add_category_ids.contains(&v.category_id),
            reason: match v.manual_decision {
                Some(true) => "Manual inclusion".into(),
                Some(false) => "Manual exclusion".into(),
                None => v
                    .sources
                    .iter()
                    .min_by_key(|s| (s.priority, &s.rule_id))
                    .map_or_else(
                        || "No category rule source".into(),
                        |s| {
                            format!(
                                "{} by {}",
                                if s.assigned { "Added" } else { "Removed" },
                                s.rule_name
                            )
                        },
                    ),
            },
        })
        .collect();
    let token = state.category_previews.insert(CategoryPreview::Membership {
        account_id: account_id.cloned(),
        message_ids: message_ids.to_vec(),
        versions,
        add_category_ids: add_category_ids.to_vec(),
        remove_category_ids: remove_category_ids.to_vec(),
        accounts,
    });
    Ok(ResponseData::CategoryMembershipPreview {
        preview: CategoryMembershipPreview {
            effects,
            category_ids,
            message_count: message_ids.len() as u32,
            added,
            removed,
            preview_token: token,
        },
    })
}

pub(crate) async fn apply_membership(
    state: &AppState,
    token: &str,
) -> Result<ResponseData, String> {
    let _gate = state.rule_mutation_gate.lock().await;
    let preview = state.category_previews.take(token)?;
    let CategoryPreview::Membership {
        account_id,
        message_ids,
        versions,
        add_category_ids,
        remove_category_ids,
        ..
    } = preview
    else {
        return Err("preview token is for category deletion".into());
    };
    let mut category_ids: Vec<_> = versions.iter().map(|v| v.category_id.clone()).collect();
    category_ids.sort_by_key(ToString::to_string);
    category_ids.dedup();
    let current = state
        .store
        .category_membership_versions(account_id.as_ref(), &message_ids, &category_ids)
        .await?;
    if current != versions {
        return Err("category memberships changed after preview; preview again".into());
    }
    let changes: Vec<_> = versions
        .into_iter()
        .filter_map(|before| {
            let assigned = if add_category_ids.contains(&before.category_id) {
                Some(true)
            } else if remove_category_ids.contains(&before.category_id) {
                Some(false)
            } else {
                None
            }?;
            Some(CategoryMembershipChange { before, assigned })
        })
        .collect();
    let (mutation_id, added, removed) = state
        .store
        .apply_category_membership_changes(&changes)
        .await?;
    Ok(ResponseData::CategoryMembershipApplied {
        mutation_id,
        added,
        removed,
    })
}

pub(crate) async fn preview_delete(
    state: &AppState,
    category_id: &CategoryId,
) -> Result<ResponseData, String> {
    let _gate = state.rule_mutation_gate.lock().await;
    super::category_rules::reject_references(state, category_id).await?;
    let (name, _) = state.store.get_personal_category(category_id).await?;
    let versions = state
        .store
        .all_category_membership_versions(category_id)
        .await?;
    let token = state.category_previews.insert(CategoryPreview::Delete {
        category_id: category_id.clone(),
        name: name.clone(),
        versions: versions.clone(),
    });
    Ok(ResponseData::CategoryDeletePreview {
        preview: CategoryDeletePreview {
            category_id: category_id.clone(),
            name,
            affected_messages: versions.len() as u32,
            preview_token: token,
        },
    })
}

pub(crate) async fn delete(state: &AppState, token: &str) -> Result<ResponseData, String> {
    let _gate = state.rule_mutation_gate.lock().await;
    let preview = state.category_previews.take(token)?;
    let CategoryPreview::Delete {
        category_id,
        name: expected_name,
        versions,
    } = preview
    else {
        return Err("preview token is for category membership".into());
    };
    super::category_rules::reject_references(state, &category_id).await?;
    let name = state
        .store
        .delete_personal_category_if_unchanged(&category_id, &expected_name, &versions)
        .await?;
    Ok(ResponseData::CategoryDeleted { category_id, name })
}

pub(crate) async fn undo_membership(
    state: &AppState,
    mutation_id: &str,
) -> Result<ResponseData, String> {
    let _gate = state.rule_mutation_gate.lock().await;
    let (restored, skipped) = state.store.undo_category_membership(mutation_id).await?;
    Ok(ResponseData::CategoryMembershipUndone { restored, skipped })
}
