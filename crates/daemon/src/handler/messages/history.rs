//! Incremental thread facts for All. Turns, people links, Done, ranking and
//! previews remain live; no complete view or time-dependent lane is persisted.
use super::*;
use mxr_store::{ConversationSummaryWrite, DeskDismissal};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Latest {
    pub id: MessageId,
    pub date: DateTime<Utc>,
    pub subject: String,
    pub outbound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Summary {
    pub thread_id: ThreadId,
    pub shape: Shape,
    pub names: Vec<(String, String, DateTime<Utc>)>,
    pub latest: Option<Latest>,
    pub reply_to: Option<Latest>,
    pub message_ids: Vec<MessageId>,
    pub unread: bool,
    watermark: (i64, uuid::Uuid),
    valid_until: i64,
}

impl Summary {
    pub(super) fn from_thread(
        thread: &[DeskMessage],
        shape: Shape,
        own: &dyn Fn(&str) -> bool,
        now: DateTime<Utc>,
    ) -> Self {
        let current = current_messages(thread, now);
        let latest = |m: &DeskMessage| Latest {
            id: m.id.clone(),
            date: m.date,
            subject: m.subject.clone(),
            outbound: is_outbound(m, own),
        };
        let mut names = Vec::new();
        for m in thread {
            let addresses: Vec<_> = if is_outbound(m, own) {
                m.to.iter().chain(&m.cc).collect()
            } else {
                vec![&m.from]
            };
            for a in addresses {
                if let Some(name) = a
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && !n.contains('@'))
                {
                    names.push((a.email.clone(), name.to_string(), m.date));
                }
            }
        }
        Self {
            thread_id: thread[0].thread_id.clone(),
            shape,
            names,
            latest: last_stored(current, |m| !m.trashed).map(latest),
            reply_to: last_stored(current, |m| !m.trashed && !is_outbound(m, own)).map(latest),
            message_ids: thread.iter().map(|m| m.id.clone()).collect(),
            unread: thread.iter().any(|m| {
                !is_outbound(m, own)
                    && m.in_inbox
                    && !m.flags.contains(mxr_core::MessageFlags::READ)
            }),
            watermark: thread
                .iter()
                .map(|m| (m.date.timestamp(), *m.id.as_uuid()))
                .max()
                .unwrap_or_default(),
            // The only clock-dependent facts are the newest eligible stored
            // messages. Retire at the first future date becoming eligible.
            valid_until: thread
                .iter()
                .filter(|m| m.date > now + Duration::days(1))
                .map(|m| m.date.timestamp() - 86_400)
                .min()
                .unwrap_or(i64::MAX),
        }
    }

    pub(super) fn covered_by(&self, mark: &DeskDismissal) -> bool {
        match &mark.covered {
            Some(ids) => self
                .message_ids
                .iter()
                .all(|id| ids.binary_search(id.as_uuid()).is_ok()),
            None => {
                self.message_ids.len() <= mark.through_count
                    && self.watermark <= (mark.through_date, mark.through_id)
            }
        }
    }

    pub(super) fn topic(
        &self,
        account: &AccountId,
        people: &People,
        turn: Option<DeskRowData>,
        key: Option<RowKey>,
        done: bool,
    ) -> Option<Topic> {
        super::summary_topic(self, account, people, turn, key, done)
    }
}

async fn context(state: &AppState, account: &AccountId) -> Result<String, HandlerError> {
    let mut own: Vec<_> = state
        .owned_addresses(account)
        .await?
        .iter()
        .map(str::to_string)
        .collect();
    own.sort();
    Ok(serde_json::to_string(&(
        "conversation-summary-v1",
        mxr_core::assessment::CLASSIFIER_POLICY_VERSION,
        mxr_core::asks::ASKS_POLICY_VERSION,
        mxr_store::asks_context_token().map_err(sqlx::Error::Io)?,
        super::super::conversation_shape::shape_config(state).large_thread_recipients,
        own,
    ))?)
}

pub(super) async fn load(
    state: &AppState,
    account: &AccountId,
    now: DateTime<Utc>,
) -> Result<Vec<Summary>, HandlerError> {
    let context = context(state, account).await?;
    let candidates = state
        .store
        .conversation_summaries::<Summary>(account, &context, now)
        .await?;
    let mut summaries = Vec::with_capacity(candidates.len());
    let mut missing = HashMap::new();
    for candidate in candidates {
        match candidate.json {
            Some(summary) => summaries.push(summary),
            None => {
                missing.insert(candidate.revision.thread_id.clone(), candidate.revision);
            }
        }
    }
    if missing.is_empty() {
        return Ok(summaries);
    }
    let mut ids: Vec<_> = missing.keys().cloned().collect();
    ids.sort_by_key(ThreadId::as_str);
    let mut messages = Vec::new();
    for chunk in ids.chunks(500) {
        messages.extend(state.store.desk_history_in_threads(account, chunk).await?);
    }
    messages.sort_by(|a, b| {
        a.thread_id
            .as_uuid()
            .cmp(b.thread_id.as_uuid())
            .then(a.date.cmp(&b.date))
            .then(a.seq.cmp(&b.seq))
    });
    let senders = Senders::load(state, account, &messages).await?;
    let own = self_matcher(state, account).await?;
    let pending = senders.pending_threads(&messages, &own);
    let timers = DeskTimers::default();
    let dismissed = HashMap::new();
    let writes = {
        let inputs = AccountInputs {
            pending_threads: &pending,
            assessments: &senders.assessments,
            asks: &senders.asks,
            account_id: account,
            messages: &messages,
            contacts: &senders.contacts,
            written_to: &senders.written_to,
            screener: &senders.screener,
            dismissed: &dismissed,
            timers: &timers,
            is_self: &own,
            moves: &senders.moves,
            treatments: &senders.treatments,
            shape: super::super::conversation_shape::shape_config(state),
            now,
        };
        let mut writes = Vec::new();
        for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
            if pending.contains(&thread[0].thread_id) {
                continue;
            }
            let summary = Summary::from_thread(thread, inputs.shape(thread), &own, now);
            if let Some(revision) = missing.remove(&summary.thread_id) {
                writes.push(ConversationSummaryWrite {
                    revision,
                    valid_from: now.timestamp(),
                    valid_until: summary.valid_until,
                    json: serde_json::to_string(&summary)?,
                });
            }
            summaries.push(summary);
        }
        writes
    };
    state
        .store
        .save_conversation_summaries(account, &context, &writes)
        .await?;
    Ok(summaries)
}

pub(crate) async fn warm(state: &AppState) -> Result<(), HandlerError> {
    for account in scoped_cached_accounts(state, None).await? {
        load(state, &account, Utc::now()).await?;
        // A missing facet materialized by the first pass changes its view
        // token, correctly refusing that pass's optimistic synopsis write.
        load(state, &account, Utc::now()).await?;
    }
    Ok(())
}
