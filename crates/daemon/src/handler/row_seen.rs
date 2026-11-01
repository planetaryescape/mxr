//! Attention belongs to an exact row revision, never to the email's read flag.
//! Apply this after cache reads too, so another client's peek is visible at once.
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::AccountId;
use mxr_protocol::{ModeKindData, ResponseData, TodoData, UpdateLineData};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn revision(value: &impl serde::Serialize) -> Result<String, HandlerError> {
    Ok(
        Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn todo_revision(todo: &TodoData) -> Result<String, HandlerError> {
    // Not labels that change with the clock, completion, or provider flags.
    let evidence = todo
        .resolution
        .as_ref()
        .map(|r| (&r.message_id, &r.quote, &r.action, r.proposed_due_at));
    revision(&serde_json::json!([
        todo.source_message_id,
        todo.title,
        todo.kind,
        todo.verb,
        todo.counterparty,
        todo.amount,
        todo.due_at,
        todo.act_by_at,
        todo.scheduled_for,
        todo.looks_done,
        evidence,
        todo.fields,
        todo.action
    ]))
}

fn update_revision(line: &UpdateLineData) -> Result<String, HandlerError> {
    let mut messages = line
        .message_ids
        .iter()
        .map(mxr_core::MessageId::as_str)
        .collect::<Vec<_>>();
    messages.sort_unstable();
    // Cut/section/delta/why can change without new content. New folded mail,
    // a corrected fact, or a tracker transition is new information.
    revision(&(
        messages,
        &line.fact,
        &line.numbers,
        &line.tracker,
        &line.link,
    ))
}

pub(super) async fn mark(
    state: &AppState,
    account: &AccountId,
    mode: ModeKindData,
    row_id: &str,
    revision: &str,
    dry_run: bool,
) -> HandlerResult {
    let mode = match mode {
        ModeKindData::Todo => "todo",
        ModeKindData::Updates => "updates",
        _ => {
            return Err(HandlerError::InvalidRequest(
                "Only To do and Updates have row seen state".into(),
            ))
        }
    };
    if row_id.is_empty()
        || row_id.len() > 512
        || revision.len() != 64
        || !revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(HandlerError::InvalidRequest(
            "Use the row id and seen_revision returned by the daemon".into(),
        ));
    }
    if !state
        .store
        .list_accounts()
        .await?
        .iter()
        .any(|a| a.enabled && &a.id == account)
    {
        return Err(HandlerError::InvalidRequest(
            "No enabled account for this row".into(),
        ));
    }
    if !dry_run {
        state
            .store
            .mark_mode_row_seen(account, mode, row_id, revision, chrono::Utc::now())
            .await?;
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn decorate(state: &AppState, result: HandlerResult) -> HandlerResult {
    let mut data = result?;
    let attach_resolution = matches!(
        &data,
        ResponseData::TodoChange { .. } | ResponseData::Now { .. }
    );
    let mut todos = Vec::new();
    let mut lines = Vec::new();
    match &mut data {
        ResponseData::Now { now } => {
            todos.extend(now.due_soon.todos.iter_mut().map(|item| &mut item.todo));
            if let Some(updates) = &mut now.updates {
                lines.extend(&mut updates.lines);
            }
        }
        ResponseData::Todo { todo } => todos.push(todo),
        ResponseData::Todos { todos: rows } => todos.extend(rows),
        ResponseData::TodoRunway { runway } => {
            todos.extend(&mut runway.now);
            todos.extend(&mut runway.later);
            todos.extend(&mut runway.whenever);
            todos.extend(&mut runway.done_this_week);
            for week in &mut runway.coming_up {
                todos.extend(&mut week.todos);
            }
        }
        ResponseData::TodoCatchup { catchup } => todos.extend(&mut catchup.todos),
        ResponseData::TodoChange { change } => todos.extend(&mut change.changed),
        ResponseData::UpdatesDigest { digest } => {
            lines.extend(&mut digest.needs_a_look);
            lines.extend(&mut digest.changed);
            lines.extend(&mut digest.routine);
            lines.extend(&mut digest.since.lines);
        }
        _ => return Ok(data),
    }
    let mut revisions: HashMap<_, Vec<(String, String)>> = HashMap::new();
    for todo in &mut todos {
        // Mutation replies and list reads must describe the same revision;
        // some mutation paths build the task before attaching its evidence.
        // List responses already attached evidence, including the absence of
        // an offer. Do not repeat those queries on every cached list read.
        if attach_resolution && todo.resolution.is_none() {
            todo.resolution = state
                .store
                .latest_todo_resolution(&todo.id, &todo.account_id)
                .await?
                .map(|suggestion| super::todo_resolution::resolution_data(&suggestion));
        }
        let revision = todo_revision(todo)?;
        revisions
            .entry((todo.account_id.clone(), "todo"))
            .or_default()
            .push((todo.id.clone(), revision.clone()));
        todo.seen_revision = Some(revision);
    }
    for line in &mut lines {
        let revision = update_revision(line)?;
        revisions
            .entry((line.account_id.clone(), "updates"))
            .or_default()
            .push((line.id.clone(), revision.clone()));
        line.seen_revision = Some(revision);
    }
    // One indexed lookup per account/mode, bounded by this response's rows,
    // not by the lifetime history of acknowledged revisions.
    let mut seen = HashMap::new();
    for ((account, mode), rows) in revisions {
        seen.insert(
            (account.clone(), mode),
            state.store.mode_rows_seen_in(&account, mode, &rows).await?,
        );
    }
    for todo in todos {
        todo.unread = Some(
            !seen
                .get(&(todo.account_id.clone(), "todo"))
                .is_some_and(|rows| {
                    todo.seen_revision
                        .as_ref()
                        .is_some_and(|revision| rows.contains(&(todo.id.clone(), revision.clone())))
                }),
        );
    }
    for line in lines {
        line.unread = Some(
            !seen
                .get(&(line.account_id.clone(), "updates"))
                .is_some_and(|rows| {
                    line.seen_revision
                        .as_ref()
                        .is_some_and(|revision| rows.contains(&(line.id.clone(), revision.clone())))
                }),
        );
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tracker_without_email_gets_new_attention_only_for_new_content() {
        let mut line: UpdateLineData = serde_json::from_value(serde_json::json!({
            "id": "parcel:123", "account_id": AccountId::new(), "section": "changed",
            "source_key": "parcel:123", "source_name": "Parcel", "fact": "On its way",
            "fact_source": "tracker", "signal": "changed", "count": 1,
            "latest_at": "2026-10-01T08:00:00Z", "todo_title": "Check parcel",
            "why": "Parcel in transit", "setting": "every_digest",
            "tracker": { "kind": "parcel", "state": "in_transit", "state_label": "On its way",
                "outcome": "progress", "steps": [] }
        }))
        .unwrap();
        let opened = update_revision(&line).unwrap();
        line.code = Some("123456".into());
        assert_eq!(
            update_revision(&line).unwrap(),
            opened,
            "a clock-cleared code is not new mail"
        );
        line.code = None;
        line.section = mxr_protocol::UpdateSectionData::Routine;
        line.why = "In the next digest".into();
        line.unread = Some(false);
        assert_eq!(
            update_revision(&line).unwrap(),
            opened,
            "cut placement is not new content"
        );
        line.tracker.as_mut().unwrap().state = "out_for_delivery".into();
        assert_ne!(
            update_revision(&line).unwrap(),
            opened,
            "a new tracker state needs attention"
        );
    }

    #[tokio::test]
    async fn invalid_row_acknowledgements_write_nothing() {
        let (state, _) = AppState::in_memory_with_fake_config(mxr_config::MxrConfig::default())
            .await
            .unwrap();
        let account = state.default_account_id();
        let rev = "a".repeat(64);
        assert!(
            mark(&state, &account, ModeKindData::Messages, "row", &rev, false)
                .await
                .is_err()
        );
        assert!(mark(
            &state,
            &account,
            ModeKindData::Todo,
            "row",
            "not-a-revision",
            false
        )
        .await
        .is_err());
        assert!(mark(
            &state,
            &AccountId::new(),
            ModeKindData::Todo,
            "row",
            &rev,
            false
        )
        .await
        .is_err());
        assert!(state
            .store
            .mode_rows_seen(&account, "todo")
            .await
            .unwrap()
            .is_empty());
    }
}
