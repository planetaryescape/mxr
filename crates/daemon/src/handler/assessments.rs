//! The canonical home resolver and bounded initialization of derived answers.
use super::{
    arrivals,
    desk::{self_matcher, Senders},
    desk_lanes::is_outbound,
    mail_kind, HandlerError,
};
use crate::state::AppState;
use mxr_core::{
    assessment::{AssessmentClaim, MailAssessment, CLASSIFIER_POLICY_VERSION},
    AccountId, MessageId,
};
use mxr_protocol::{KindRuleData, SenderKindData};
use mxr_store::DeskMessage;
use std::collections::HashMap;

#[cfg(test)]
type BeforeCommit = std::sync::Arc<
    dyn Fn()
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), HandlerError>> + Send>>
        + Send
        + Sync,
>;

#[cfg(test)]
tokio::task_local! {
    pub(super) static BEFORE_COMMIT: BeforeCommit;
}

pub(super) fn describe(assessment: &MailAssessment) -> mxr_protocol::MailKindData {
    let kind = match assessment.home.as_str() {
        "reading" => SenderKindData::Reading,
        "updates" => SenderKindData::PaperTrail,
        "messages" => SenderKindData::People,
        _ => SenderKindData::ScreenedOut,
    };
    mxr_protocol::MailKindData {
        kind,
        rule: serde_json::from_value(serde_json::Value::String(assessment.deciding.rule.clone()))
            .unwrap_or(KindRuleData::Person),
        reason: assessment.deciding.reason.clone(),
        corrected: matches!(assessment.deciding.source.as_str(), "you" | "teaching_rule"),
    }
}

fn input_fingerprint(
    account: &AccountId,
    thread: &[DeskMessage],
    senders: &Senders,
    is_self: &dyn Fn(&str) -> bool,
    shape: super::conversation_shape::ShapeConfig,
    spam: &std::collections::HashSet<MessageId>,
) -> String {
    let observed: Vec<_> = thread
        .iter()
        .map(|m| {
            let signals = senders.signals(m, is_self);
            let owned: Vec<_> = std::iter::once(&m.from)
                .chain(&m.to)
                .chain(&m.cc)
                .chain(&m.bcc)
                .map(|a| (a.email.to_ascii_lowercase(), is_self(&a.email)))
                .collect();
            let e = &m.evidence;
            // Hash observations explicitly: Debug redacts content, and live
            // inbox/snooze/read state does not change an email's purpose.
            serde_json::json!({
                "message": [m.id, m.seq, m.thread_id, m.direction, m.date, m.from, m.to, m.cc, m.bcc,
                    m.subject, m.list_id, m.unsubscribe, m.trashed, m.flags.bits() & (mxr_core::MessageFlags::SPAM | mxr_core::MessageFlags::TRASH).bits(), m.is_delivery, m.is_invite],
                "evidence": [e.list_id, e.previous_text, e.text, e.unsubscribe, e.auto_generated,
                    e.bulk, e.is_reply, e.thread_correspondence],
                "sender": [signals.sender.wrote_to, signals.sender.templated,
                    signals.written_to, format!("{:?}", signals.decision), format!("{:?}", signals.moved),
                    format!("{:?}", signals.treatment)],
                "recipient": [signals.reply_request, signals.recipients, signals.addressed],
                "owned": owned,
                "spam_boundary": spam.contains(&m.id),
            }).to_string()
        })
        .collect();
    mxr_store::assessment_fingerprint(&format!(
        "{CLASSIFIER_POLICY_VERSION}|{account}|{shape:?}|{observed:?}"
    ))
}

async fn spam_boundaries(
    state: &AppState,
    messages: &[DeskMessage],
) -> Result<std::collections::HashSet<MessageId>, HandlerError> {
    let ids: Vec<_> = messages
        .iter()
        .filter(|m| m.trashed)
        .map(|m| m.id.clone())
        .collect();
    Ok(state
        .store
        .list_envelopes_by_ids(&ids)
        .await?
        .into_iter()
        .filter(|m| {
            m.flags.contains(mxr_core::MessageFlags::SPAM)
                || m.label_provider_ids.iter().any(|label| label == "SPAM")
        })
        .map(|m| m.id)
        .collect())
}

/// Validate stored answers without judging or writing them. A mutation can
/// make a row pending before the background reconciler reaches its thread.
/// Unrelated account changes never invalidate an unchanged fingerprint.
pub(super) async fn fresh_stored(
    state: &AppState,
    account: &AccountId,
    mut stored: HashMap<MessageId, MailAssessment>,
) -> Result<HashMap<MessageId, MailAssessment>, HandlerError> {
    let _config = state.assessment_config_gate.read().await;
    let ids: Vec<_> = stored.keys().cloned().collect();
    let envelopes = state.store.list_envelopes_by_ids(&ids).await?;
    let mut threads: Vec<_> = envelopes
        .iter()
        .filter(|m| &m.account_id == account)
        .map(|m| m.thread_id.clone())
        .collect();
    threads.sort_by_key(mxr_core::ThreadId::as_str);
    threads.dedup();
    for _ in 0..3 {
        let revision = state
            .store
            .assessment_thread_revisions(account, &threads)
            .await?;
        let mut messages = Vec::new();
        for chunk in threads.chunks(super::modes::MEMBERSHIP_MAX_THREADS) {
            messages.extend(state.store.desk_messages_in_threads(account, chunk).await?);
        }
        let spam = spam_boundaries(state, &messages).await?;
        let senders = Senders::load_raw(state, account, &messages).await?;
        let own = self_matcher(state, account).await?;
        let shape = super::conversation_shape::shape_config(state);
        let mut valid = std::collections::HashSet::new();
        for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
            let fingerprint = input_fingerprint(account, thread, &senders, &own, shape, &spam);
            valid.extend(
                thread
                    .iter()
                    .filter(|m| {
                        stored
                            .get(&m.id)
                            .is_some_and(|a| a.input_fingerprint == fingerprint)
                    })
                    .map(|m| m.id.clone()),
            );
        }
        if revision
            == state
                .store
                .assessment_thread_revisions(account, &threads)
                .await?
        {
            stored.retain(|id, _| valid.contains(id));
            return Ok(stored);
        }
    }
    // No coherent input snapshot was available; report pending, never guess.
    Ok(HashMap::new())
}

/// No rule gate is acquired here: correction/arrival callers already own it.
/// Re-read full threads, rather than fingerprinting a caller's partial window.
pub(super) async fn materialize(
    state: &AppState,
    account: &AccountId,
    requested: &[DeskMessage],
) -> Result<HashMap<MessageId, MailAssessment>, HandlerError> {
    let mut threads: Vec<_> = requested.iter().map(|m| m.thread_id.clone()).collect();
    threads.sort_by_key(mxr_core::ThreadId::as_str);
    threads.dedup();
    materialize_threads(state, account, &threads, || async {
        #[cfg(test)]
        if let Ok(hook) = BEFORE_COMMIT.try_with(std::sync::Arc::clone) {
            hook().await?;
        }
        Ok(())
    })
    .await
}

pub(super) async fn materialize_threads<F, Fut>(
    state: &AppState,
    account: &AccountId,
    threads: &[mxr_core::ThreadId],
    mut before_commit: F,
) -> Result<HashMap<MessageId, MailAssessment>, HandlerError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), HandlerError>>,
{
    let _config = state.assessment_config_gate.read().await;
    for _ in 0..3 {
        let revision = state
            .store
            .assessment_thread_revisions(account, threads)
            .await?;
        let mut messages = Vec::new();
        for chunk in threads.chunks(super::modes::MEMBERSHIP_MAX_THREADS) {
            messages.extend(state.store.desk_messages_in_threads(account, chunk).await?);
        }
        let spam = spam_boundaries(state, &messages).await?;
        let senders = Senders::load_raw(state, account, &messages).await?;
        let is_self = self_matcher(state, account).await?;
        let shape = super::conversation_shape::shape_config(state);
        let ids: Vec<_> = messages.iter().map(|m| m.id.clone()).collect();
        let (stored, _) = state
            .store
            .assessment_facets_for_messages(account, &ids)
            .await?;
        let mut answers = Vec::new();
        for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
            let fingerprint = input_fingerprint(account, thread, &senders, &is_self, shape, &spam);
            for message in thread {
                if is_outbound(message, &is_self) {
                    continue;
                }
                if stored
                    .get(&message.id)
                    .is_some_and(|a| a.input_fingerprint == fingerprint)
                {
                    continue;
                }
                let signals = senders.signals(message, &is_self);
                let who = mail_kind::describe(&signals);
                let mut placement = arrivals::place_message(
                    message,
                    thread,
                    &senders,
                    &is_self,
                    shape,
                    spam.contains(&message.id),
                );
                if message.trashed && placement.mode != "spam" {
                    placement.mode = "trash".into();
                    placement.rule = "trash".into();
                    placement.reason = "in Trash".into();
                    placement.not_sure = None;
                }
                let source = match placement.rule.as_str() {
                    "spam" | "trash" => "boundary",
                    "decision" | "moved" => "you",
                    "custom_rule" => "teaching_rule",
                    _ => "rule",
                };
                let fallback = source == "rule"
                    && placement.rule != "copied"
                    && mail_kind::unknown_purpose(&signals)
                    && !matches!(placement.mode.as_str(), "reading" | "screened_out")
                    && !message.is_delivery
                    && !message.is_invite;
                if fallback && placement.not_sure.is_none() {
                    placement.not_sure = Some("fallback".into());
                }
                if fallback {
                    placement.reason = if placement.mode == "messages" {
                        "No rule decided; Messages for now"
                    } else {
                        "No rule decided; Updates for now"
                    }
                    .into();
                }
                let deciding = AssessmentClaim {
                    rule: if fallback {
                        "fallback".into()
                    } else {
                        placement.rule
                    },
                    reason: placement.reason,
                    source: if fallback { "fallback" } else { source }.into(),
                    evidence: vec![
                        format!("message:{}:cached_headers_and_body", message.id),
                        format!("thread:{}:shape", message.thread_id),
                        format!(
                            "sender:{}:account_history_and_teaching",
                            message.from.email.to_ascii_lowercase()
                        ),
                    ],
                };
                let overridden = if deciding.reason != who.reason {
                    vec![AssessmentClaim {
                        rule: placement_rule(who.rule),
                        reason: who.reason,
                        source: "who".into(),
                        evidence: vec![format!("message:{}:sender_and_purpose", message.id)],
                    }]
                } else {
                    Vec::new()
                };
                let assessment = MailAssessment {
                    message_id: message.id.clone(),
                    account_id: account.clone(),
                    policy_version: CLASSIFIER_POLICY_VERSION.into(),
                    facts: mail_kind::observed_facts(&signals),
                    home: placement.mode,
                    who: match who.kind {
                        SenderKindData::People => "person",
                        SenderKindData::Reading => "list",
                        SenderKindData::PaperTrail => "machine",
                        SenderKindData::ScreenedOut => "screened_out",
                    }
                    .into(),
                    deciding,
                    overridden,
                    not_sure: placement.not_sure,
                    fallback,
                    pending_body: message.evidence.text.is_none(),
                    input_fingerprint: fingerprint.clone(),
                };
                answers.push(assessment);
            }
        }
        before_commit().await?;
        let asks =
            super::ask_assessments::prepare(state, account, &messages, &senders, &is_self).await?;
        if !state
            .store
            .save_assessment_facets(account, &revision, &answers, &asks)
            .await?
        {
            continue;
        }
        replay_projections(state, account).await?;
        let mut persisted = state
            .store
            .assessments_for_messages(account, CLASSIFIER_POLICY_VERSION, &ids)
            .await?;
        let inbound: std::collections::HashSet<_> = messages
            .iter()
            .filter(|m| !is_outbound(m, &is_self))
            .map(|m| &m.id)
            .collect();
        persisted.retain(|id, _| inbound.contains(id));
        if revision
            == state
                .store
                .assessment_thread_revisions(account, threads)
                .await?
        {
            return Ok(persisted);
        }
    }
    // Relevant inputs kept moving. Readers must expose pending, never a
    // newly computed home or a stale stored answer. Dirty work stays queued.
    Ok(HashMap::new())
}

/// Synthetic fixtures that bypass sync also cover historical/no-arrivals mail.
#[cfg(test)]
pub(crate) async fn reconcile_account(
    state: &AppState,
    account: &AccountId,
) -> Result<(), HandlerError> {
    let mut after: Option<String> = None;
    loop {
        let threads = state
            .store
            .assessment_threads_page(account, after.as_deref(), 100)
            .await?;
        let Some(last) = threads.last() else {
            break;
        };
        after = Some(last.as_str());
        materialize_threads(state, account, &threads, || async { Ok(()) }).await?;
        tokio::task::yield_now().await;
    }
    // A thread page can contain more than 100 messages. Finish its durable
    // projection pages through the same bounded seam as the background worker.
    let mut after = None;
    while let Some(last) = reconcile_dirty_page(state, account, after.as_deref()).await? {
        after = Some(last);
    }
    Ok(())
}

fn placement_rule(rule: KindRuleData) -> String {
    serde_json::to_value(rule)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "person".into())
}

pub(crate) async fn reconciler_loop(
    state: std::sync::Arc<AppState>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
    let mut previous_shape = None;
    let mut previous_asks_context = None;
    loop {
        tokio::select! { _ = tick.tick() => {}, changed = shutdown.changed() => {
            if changed.is_err() || *shutdown.borrow_and_update() { break; } continue;
        }}
        let shape = super::conversation_shape::shape_config(&state);
        let asks_context = mxr_store::asks_context_token().ok();
        if previous_shape != Some(shape) || previous_asks_context != asks_context {
            if let Err(error) = state.store.invalidate_all_assessments().await {
                tracing::warn!(%error, "assessment initialization pending");
                continue;
            }
            previous_shape = Some(shape);
            previous_asks_context = asks_context;
        }
        let Ok(accounts) = state.store.list_accounts().await else {
            continue;
        };
        for account in accounts {
            // A busy early page must not starve later pages or accounts.
            let mut after = None;
            loop {
                let result = reconcile_dirty_page(&state, &account.id, after.as_deref()).await;
                match result {
                    Ok(Some(last)) => after = Some(last),
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(account = %account.id, %error, "assessment reconciliation pending");
                        break;
                    }
                }
                if *shutdown.borrow() {
                    return;
                }
                tokio::task::yield_now().await;
            }
            if let Err(error) = arrivals::place_pending(&state, &account.id).await {
                tracing::warn!(account = %account.id, %error, "arrival placement pending after assessment reconciliation");
            }
        }
    }
}

async fn replay_projections(state: &AppState, account: &AccountId) -> Result<usize, HandlerError> {
    if !state.config_snapshot().todo.enabled {
        return Ok(0);
    }
    let mut acknowledged = 0;
    let mut failure = None;
    for ticket in state.store.pending_asks_projections(account).await? {
        match mxr_todo::pass::project_stored_ask(
            &state.store,
            &super::todos::pass_config(state, chrono::Utc::now()),
            &ticket.answer.message_id,
        )
        .await
        {
            Ok(()) => {
                if state.store.acknowledge_asks_projection(&ticket).await? {
                    acknowledged += 1;
                }
            }
            Err(error) => {
                tracing::warn!(message = %ticket.answer.message_id, %error, "stored asks projection remains queued");
                failure.get_or_insert_with(|| HandlerError::from(error.to_string()));
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(acknowledged)
}

pub(super) async fn reconcile_dirty_page(
    state: &AppState,
    account: &AccountId,
    after: Option<&str>,
) -> Result<Option<String>, HandlerError> {
    let replayed = replay_projections(state, account).await?;
    let threads = state.store.dirty_assessment_threads(account, after).await?;
    let Some(last) = threads.last().map(mxr_core::ThreadId::as_str) else {
        // Yield between bounded projection pages even when no thread is dirty.
        // Keep the cursor so completing an outbox page never rescans the account.
        return Ok((replayed == 100).then(|| after.unwrap_or_default().to_string()));
    };
    materialize_threads(state, account, &threads, || async { Ok(()) }).await?;
    Ok(Some(last))
}

/// The sync page pays only for its threads; sender-history dependencies are
/// durably queued by the same transaction that changed the input.
pub(crate) async fn reconcile_messages(
    state: &AppState,
    account: &AccountId,
    ids: &[MessageId],
) -> Result<(), HandlerError> {
    let mut threads: Vec<_> = state
        .store
        .list_envelopes_by_ids(ids)
        .await?
        .into_iter()
        .filter(|m| &m.account_id == account)
        .map(|m| m.thread_id)
        .collect();
    threads.sort_by_key(mxr_core::ThreadId::as_str);
    threads.dedup();
    for chunk in threads.chunks(100) {
        materialize_threads(state, account, chunk, || async { Ok(()) }).await?;
    }
    Ok(())
}
