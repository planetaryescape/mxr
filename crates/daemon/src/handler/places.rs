//! Reading and Paper trail: mail that isn't from people, grouped by sender,
//! with the reason each sender is there, one-key corrections, pins, and a
//! sweep that archives everything unpinned.
//!
//! Kinds come from `mail_kind`, the same classifier the desk uses. The
//! sweep's preview and the sweep itself share `sweep_selection`, and the
//! archive runs as an ordinary mutation job, so undo works as it does for
//! any bulk archive.

use super::mail_kind::{self, KindSignals};
use super::{mutations, HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::{AccountAddressLookup, UnsubscribeMethod};
use mxr_core::MessageFlags;
use mxr_protocol::{
    MailKindData, MailPlaceData, MutationCommand, PlaceBundleData, PlaceMessageData, ResponseData,
    SenderKindData, SweepPreviewData, SweepSenderData,
};
use mxr_store::{PlaceMessage, ScreenerDecision, ScreenerDisposition};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Subjects shown in a sweep preview.
const SWEEP_SAMPLE_SUBJECTS: usize = 5;
/// Upper bound on messages listed per bundle, whatever the client asks.
const MAX_MESSAGES_PER_BUNDLE: u32 = 200;
/// Upper bound on bundles per page.
const MAX_BUNDLES_PER_PAGE: u32 = 500;

/// The kind of mail a place holds.
const fn place_kind(place: MailPlaceData) -> SenderKindData {
    match place {
        MailPlaceData::Reading => SenderKindData::Reading,
        MailPlaceData::PaperTrail => SenderKindData::PaperTrail,
    }
}

/// Everything deciding kinds needs about one account, read once.
struct AccountKinds {
    decisions: HashMap<String, ScreenerDisposition>,
    list_senders: HashSet<String>,
    addresses: Arc<mxr_core::types::InMemoryAccountAddressLookup>,
    account_id: AccountId,
    account_email: Option<String>,
}

impl AccountKinds {
    async fn load(
        state: &AppState,
        account_id: &AccountId,
        senders: &[String],
    ) -> Result<Self, HandlerError> {
        let store = &state.store;
        let decisions = store
            .list_screener_decisions(account_id)
            .await?
            .into_iter()
            .map(|d| (d.sender_email.to_ascii_lowercase(), d.disposition))
            .collect();
        let list_senders = store
            .desk_contacts(account_id, senders)
            .await?
            .into_iter()
            .filter(|contact| contact.is_list_sender)
            .map(|contact| contact.email.to_ascii_lowercase())
            .collect();
        let account_email = store
            .get_account(account_id)
            .await?
            .map(|account| account.email.to_ascii_lowercase());
        Ok(Self {
            decisions,
            list_senders,
            addresses: state.account_addresses.clone(),
            account_id: account_id.clone(),
            account_email,
        })
    }

    fn is_self(&self, email: &str) -> bool {
        self.addresses.is_account_address(&self.account_id, email)
            || self
                .account_email
                .as_deref()
                .is_some_and(|own| own.eq_ignore_ascii_case(email))
    }

    fn signals<'a>(&self, message: &'a PlaceMessage) -> KindSignals<'a> {
        let key = message.from_email.to_ascii_lowercase();
        KindSignals {
            email: &message.from_email,
            has_list_id: message.list_id.is_some(),
            has_unsubscribe: !matches!(message.unsubscribe, UnsubscribeMethod::None),
            is_delivery: message.is_delivery,
            is_invite: message.is_invite,
            list_sender: self.list_senders.contains(&key),
            decision: self.decisions.get(&key).copied(),
        }
    }

    /// Mail you sent (or sent to yourself) never belongs to a place.
    fn is_outbound(&self, message: &PlaceMessage) -> bool {
        message.direction == "outbound"
            || (message.direction != "inbound" && self.is_self(&message.from_email))
    }
}

/// One message in a place, with its classification.
struct Placed {
    message: PlaceMessage,
    kind: MailKindData,
}

/// The inbox mail of `accounts` (from `sender_email` only, when given)
/// that belongs in `place`. Deliveries and invites have places of their own
/// (and an invite may still need an answer), so they stay out, as they do
/// from the desk's paper-trail count.
async fn place_messages(
    state: &AppState,
    accounts: &[AccountId],
    place: MailPlaceData,
    sender_email: Option<&str>,
) -> Result<Vec<Placed>, HandlerError> {
    let mut placed = Vec::new();
    for account_id in accounts {
        let candidates = state.store.place_candidates(account_id).await?;
        let mut senders: Vec<String> = candidates
            .iter()
            .map(|m| m.from_email.to_ascii_lowercase())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        senders.sort_unstable();
        let kinds = AccountKinds::load(state, account_id, &senders).await?;
        for message in candidates {
            if message.snoozed
                || message.is_delivery
                || message.is_invite
                || !same_sender(&message, sender_email)
                || kinds.is_outbound(&message)
            {
                continue;
            }
            // Classify first; only mail that stays gets its reason written.
            let signals = kinds.signals(&message);
            if mail_kind::classify(&signals).kind.to_data() != place_kind(place) {
                continue;
            }
            let kind = mail_kind::describe(&signals);
            placed.push(Placed { message, kind });
        }
    }
    Ok(placed)
}

async fn scoped_accounts(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<Vec<AccountId>, HandlerError> {
    Ok(match account_id {
        Some(id) => vec![id.clone()],
        None => state
            .store
            .list_accounts()
            .await?
            .into_iter()
            .filter(|account| account.enabled)
            .map(|account| account.id)
            .collect(),
    })
}

fn same_sender(message: &PlaceMessage, sender_email: Option<&str>) -> bool {
    sender_email.is_none_or(|wanted| {
        message
            .from_email
            .trim()
            .eq_ignore_ascii_case(wanted.trim())
    })
}

/// Newest message first; storage order breaks a tie.
fn newest_first(a: &Placed, b: &Placed) -> std::cmp::Ordering {
    b.message
        .date
        .cmp(&a.message.date)
        .then_with(|| b.message.seq.cmp(&a.message.seq))
}

fn non_empty(name: Option<&String>) -> Option<String> {
    name.filter(|n| !n.trim().is_empty()).cloned()
}

/// Which page of a place to list: bundles by `offset`/`limit`, and within
/// each bundle messages by `message_offset`/`messages_per_bundle`.
#[derive(Debug, Clone, Copy)]
pub(super) struct PlacePage {
    pub limit: u32,
    pub offset: u32,
    pub messages_per_bundle: u32,
    pub message_offset: u32,
}

pub(super) async fn list_place(
    state: &AppState,
    place: MailPlaceData,
    account_id: Option<&AccountId>,
    sender_email: Option<&str>,
    page: PlacePage,
) -> HandlerResult {
    let PlacePage {
        limit,
        offset,
        messages_per_bundle,
        message_offset,
    } = page;
    let started = std::time::Instant::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let placed = place_messages(state, &accounts, place, sender_email).await?;
    let total_messages = placed.len() as u32;

    let mut grouped: HashMap<(AccountId, String), Vec<Placed>> = HashMap::new();
    for item in placed {
        let key = (
            item.message.account_id.clone(),
            item.message.from_email.to_ascii_lowercase(),
        );
        grouped.entry(key).or_default().push(item);
    }
    let per_bundle = messages_per_bundle.min(MAX_MESSAGES_PER_BUNDLE) as usize;
    let mut bundles: Vec<PlaceBundleData> = grouped
        .into_values()
        .filter_map(|mut items| {
            items.sort_by(newest_first);
            let newest = items.first()?;
            let bundle_kind = newest.kind.clone();
            let newest_at = newest.message.date;
            let newest_subject = newest.message.subject.clone();
            let account_id = newest.message.account_id.clone();
            let sender_email = newest.message.from_email.to_ascii_lowercase();
            let sender_name = items
                .iter()
                .find_map(|item| non_empty(item.message.from_name.as_ref()));
            let unread_count = items
                .iter()
                .filter(|item| !item.message.flags.contains(MessageFlags::READ))
                .count() as u32;
            let pinned_count = items.iter().filter(|item| item.message.pinned).count() as u32;
            let message_count = items.len() as u32;
            // Pinned messages first so the first page shows them, then newest.
            items.sort_by_key(|item| !item.message.pinned);
            let messages = items
                .iter()
                .skip(message_offset as usize)
                .take(per_bundle)
                .map(|item| PlaceMessageData {
                    message_id: item.message.id.clone(),
                    thread_id: item.message.thread_id.clone(),
                    subject: item.message.subject.clone(),
                    snippet: item.message.snippet.clone(),
                    date: item.message.date,
                    unread: !item.message.flags.contains(MessageFlags::READ),
                    pinned: item.message.pinned,
                    starred: item.message.flags.contains(MessageFlags::STARRED),
                })
                .collect();
            Some(PlaceBundleData {
                account_id,
                sender_email,
                sender_name,
                kind: bundle_kind,
                message_count,
                unread_count,
                pinned_count,
                newest_at,
                newest_subject,
                messages,
            })
        })
        .collect();
    bundles.sort_by(|a, b| {
        b.newest_at
            .cmp(&a.newest_at)
            .then_with(|| a.sender_email.cmp(&b.sender_email))
            .then_with(|| a.account_id.as_str().cmp(&b.account_id.as_str()))
    });
    let total_bundles = bundles.len() as u32;
    let bundles: Vec<PlaceBundleData> = bundles
        .into_iter()
        .skip(offset as usize)
        .take(limit.min(MAX_BUNDLES_PER_PAGE) as usize)
        .collect();
    tracing::debug!(
        ?place,
        accounts = accounts.len(),
        total_messages,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "place listed"
    );
    Ok(ResponseData::Place {
        place,
        account_id: account_id.cloned(),
        bundles,
        total_bundles,
        total_messages,
        generated_at: Utc::now(),
    })
}

pub(super) async fn get_message_kind(state: &AppState, message_id: &MessageId) -> HandlerResult {
    let message = state
        .store
        .place_message(message_id)
        .await?
        .ok_or_else(|| HandlerError::from(format!("Message not found: {message_id}")))?;
    let sender = message.from_email.to_ascii_lowercase();
    let kinds =
        AccountKinds::load(state, &message.account_id, std::slice::from_ref(&sender)).await?;
    Ok(ResponseData::MessageKind {
        account_id: message.account_id.clone(),
        message_id: message.id.clone(),
        sender_email: sender,
        mail_kind: mail_kind::describe(&kinds.signals(&message)),
    })
}

pub(super) async fn set_sender_kind(
    state: &AppState,
    account_id: &AccountId,
    sender_email: &str,
    kind: Option<SenderKindData>,
) -> HandlerResult {
    let sender_email = sender_email.trim().to_ascii_lowercase();
    if sender_email.is_empty() {
        return Err(HandlerError::from(
            "sender email cannot be empty".to_string(),
        ));
    }
    let store = &state.store;
    let existing = store
        .get_screener_decision(account_id, &sender_email)
        .await?;
    let previous = existing
        .as_ref()
        .and_then(|decision| mail_kind::kind_for(decision.disposition));
    match kind {
        Some(kind) => {
            store
                .set_screener_decision(&ScreenerDecision {
                    account_id: account_id.clone(),
                    sender_email: sender_email.clone(),
                    disposition: mail_kind::disposition_for(kind),
                    // A routing label chosen in the screener survives a
                    // change of kind.
                    route_label: existing.and_then(|decision| decision.route_label),
                    decided_at: Utc::now(),
                })
                .await?;
        }
        None => {
            store
                .delete_screener_decision(account_id, &sender_email)
                .await?;
        }
    }
    Ok(ResponseData::SenderKindSet {
        account_id: account_id.clone(),
        sender_email,
        sender_kind: kind,
        previous,
    })
}

pub(super) async fn pin_messages(
    state: &AppState,
    message_ids: &[MessageId],
    pinned: bool,
) -> HandlerResult {
    let changed = state.store.set_message_pins(message_ids, pinned).await?;
    Ok(ResponseData::MessagesPinned {
        changed: changed as u32,
        pinned,
    })
}

/// How long a sweep preview can be committed.
const SWEEP_PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);
/// Previews kept at once; the oldest goes first.
const MAX_SWEEP_PREVIEWS: usize = 64;

/// What a sweep covers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SweepScope {
    place: MailPlaceData,
    account_id: Option<AccountId>,
    sender_email: Option<String>,
}

struct SweepPreviewEntry {
    scope: SweepScope,
    message_ids: HashSet<MessageId>,
    created: Instant,
}

/// Dry-run selections waiting to be committed, by preview token. The real
/// sweep archives only what its preview listed: mail that arrives, moves
/// into the place, or is pinned after the preview is never taken.
#[derive(Default)]
pub(crate) struct SweepPreviews {
    entries: Mutex<HashMap<String, SweepPreviewEntry>>,
}

impl SweepPreviews {
    fn insert(&self, scope: SweepScope, message_ids: HashSet<MessageId>) -> String {
        let token = uuid::Uuid::now_v7().to_string();
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.created.elapsed() < SWEEP_PREVIEW_TTL);
        while entries.len() >= MAX_SWEEP_PREVIEWS {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.created)
                .map(|(token, _)| token.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
        entries.insert(
            token.clone(),
            SweepPreviewEntry {
                scope,
                message_ids,
                created: Instant::now(),
            },
        );
        token
    }

    /// The preview behind `token` for `scope`, once: a token cannot be
    /// replayed, and a request for another scope leaves it usable.
    fn take(&self, token: &str, scope: &SweepScope) -> Result<SweepPreviewEntry, HandlerError> {
        let mut entries = self.entries.lock();
        match entries.get(token) {
            Some(entry) if entry.created.elapsed() >= SWEEP_PREVIEW_TTL => {
                entries.remove(token);
            }
            Some(entry) if &entry.scope != scope => {
                return Err(HandlerError::from(
                    "sweep: the preview_token belongs to a different sweep".to_string(),
                ));
            }
            Some(_) => {
                if let Some(entry) = entries.remove(token) {
                    return Ok(entry);
                }
            }
            None => {}
        }
        Err(HandlerError::from(
            "sweep: that preview expired or was already used; preview again".to_string(),
        ))
    }
}

/// What a sweep archives: every unpinned message of the place (or of one
/// sender's bundle in it), narrowed to `allowed` when committing a preview.
/// The preview and the sweep both come from here.
struct SweepSelection {
    message_ids: Vec<MessageId>,
    preview: SweepPreviewData,
}

async fn sweep_selection(
    state: &AppState,
    scope: &SweepScope,
    allowed: Option<&HashSet<MessageId>>,
) -> Result<SweepSelection, HandlerError> {
    let accounts = scoped_accounts(state, scope.account_id.as_ref()).await?;
    let placed =
        place_messages(state, &accounts, scope.place, scope.sender_email.as_deref()).await?;
    let (pinned, mut selected): (Vec<&Placed>, Vec<&Placed>) =
        placed.iter().partition(|item| item.message.pinned);
    if let Some(allowed) = allowed {
        selected.retain(|item| allowed.contains(&item.message.id));
    }
    let pinned_excluded = pinned.len() as u32;
    selected.sort_by(|a, b| newest_first(a, b));

    let mut senders: HashMap<(AccountId, String), SweepSenderData> = HashMap::new();
    for item in &selected {
        let email = item.message.from_email.to_ascii_lowercase();
        let entry = senders
            .entry((item.message.account_id.clone(), email.clone()))
            .or_insert_with(|| SweepSenderData {
                account_id: item.message.account_id.clone(),
                sender_email: email,
                sender_name: None,
                count: 0,
            });
        entry.count += 1;
        if entry.sender_name.is_none() {
            entry.sender_name = non_empty(item.message.from_name.as_ref());
        }
    }
    let mut senders: Vec<SweepSenderData> = senders.into_values().collect();
    senders.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.sender_email.cmp(&b.sender_email))
    });

    let preview = SweepPreviewData {
        place: scope.place,
        sender_email: scope.sender_email.clone(),
        count: selected.len() as u32,
        pinned_excluded,
        senders,
        sample_subjects: selected
            .iter()
            .take(SWEEP_SAMPLE_SUBJECTS)
            .map(|item| item.message.subject.clone())
            .collect(),
        preview_token: None,
    };
    Ok(SweepSelection {
        message_ids: selected
            .iter()
            .map(|item| item.message.id.clone())
            .collect(),
        preview,
    })
}

pub(super) async fn sweep_place(
    state: &Arc<AppState>,
    place: MailPlaceData,
    account_id: Option<&AccountId>,
    sender_email: Option<&str>,
    dry_run: bool,
    preview_token: Option<&str>,
) -> HandlerResult {
    let scope = SweepScope {
        place,
        account_id: account_id.cloned(),
        sender_email: sender_email.map(|email| email.trim().to_ascii_lowercase()),
    };
    if dry_run {
        let mut selection = sweep_selection(state, &scope, None).await?;
        if !selection.message_ids.is_empty() {
            let ids = selection.message_ids.into_iter().collect();
            selection.preview.preview_token = Some(state.sweep_previews.insert(scope, ids));
        }
        return Ok(ResponseData::PlaceSwept {
            preview: selection.preview,
            dry_run: true,
            job: None,
        });
    }

    let token = preview_token.ok_or_else(|| {
        HandlerError::from(
            "sweep: preview it first (dry_run) and pass its preview_token".to_string(),
        )
    })?;
    let previewed = state.sweep_previews.take(token, &scope)?;
    // Only what the preview listed, still in the place and still unpinned.
    let selection = sweep_selection(state, &scope, Some(&previewed.message_ids)).await?;
    if selection.message_ids.is_empty() {
        return Ok(ResponseData::PlaceSwept {
            preview: selection.preview,
            dry_run: false,
            job: None,
        });
    }
    let started = mutations::start_mutation_job(
        state.clone(),
        MutationCommand::Archive {
            message_ids: selection.message_ids,
        },
        None,
        mutations::ChunkGuard::SkipPinned,
    )
    .await?;
    let ResponseData::JobStarted { job } = started else {
        return Err(HandlerError::from(
            "sweep: the archive job did not start".to_string(),
        ));
    };
    Ok(ResponseData::PlaceSwept {
        preview: selection.preview,
        dry_run: false,
        job: Some(job),
    })
}
