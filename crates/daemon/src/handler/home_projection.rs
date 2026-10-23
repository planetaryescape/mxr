//! Live specialist windows are projection state, never a change of home.
use super::{updates, updates_digest, HandlerError};
use crate::state::AppState;
use chrono::{DateTime, Local, Utc};
use mxr_core::{AccountId, MessageId};
use mxr_store::DeliveryListFilter;
use std::collections::HashSet;

pub(super) async fn released_specialists(
    state: &AppState,
    account: &AccountId,
    invites: &[MessageId],
    now: DateTime<Utc>,
) -> Result<HashSet<MessageId>, HandlerError> {
    let mut released = HashSet::new();
    let previous = updates::cuts(state).window(now, &Local).previous;
    for delivery in state.store.list_deliveries(DeliveryListFilter::All).await? {
        if &delivery.account_id != account {
            continue;
        }
        if delivery.dismissed_at.is_some()
            || updates_digest::parcel_line(&delivery, previous, &[], now, &Local).is_none()
        {
            released.extend(state.store.delivery_message_ids(&delivery.id).await?);
        }
    }
    for message in invites {
        let Some(invite) = state.store.get_calendar_invite_for_message(message).await? else {
            continue;
        };
        let end = invite
            .metadata
            .ends_at
            .as_deref()
            .or(invite.metadata.starts_at.as_deref())
            .and_then(|at| DateTime::parse_from_rfc3339(at).ok());
        if end.is_some_and(|at| at.with_timezone(&Utc) < now) {
            released.insert(message.clone());
        }
    }
    Ok(released)
}
