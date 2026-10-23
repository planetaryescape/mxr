//! Relationship commitments are a stored-asks projection, committed with it.
use crate::{decode_json, encode_json};
use mxr_core::asks::{AskAnswer, AskAssessment};
use mxr_core::Address;
use sqlx::Row;

pub(crate) async fn write(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    answer: &AskAssessment,
) -> Result<(), sqlx::Error> {
    let row = sqlx::query("SELECT thread_id, from_email, to_addrs, date FROM messages WHERE id = ? AND account_id = ?")
        .bind(answer.message_id.as_str()).bind(answer.account_id.as_str()).fetch_one(&mut **tx).await?;
    let thread: String = row.try_get("thread_id")?;
    let from: String = row.try_get("from_email")?;
    let recipients: Vec<Address> = decode_json(row.try_get("to_addrs")?)?;
    let date: i64 = row.try_get("date")?;
    let mut current_ids = Vec::new();
    for unit in &answer.units {
        let direction = match unit.answer {
            AskAnswer::YouPromised => "yours",
            AskAnswer::TheyPromisedYou | AskAnswer::YouAskedThem => "theirs",
            _ => continue,
        };
        if answer.pending_body {
            continue;
        }
        let Some(tags) = unit.tags.as_ref() else {
            continue;
        };
        let Some(quote) = answer.quote(unit) else {
            continue;
        };
        let email = if answer.facts.owner_authored {
            let Some(to) = recipients.iter().find(|a| {
                !answer
                    .facts
                    .owner_addresses
                    .iter()
                    .any(|own| own.eq_ignore_ascii_case(&a.email))
            }) else {
                continue;
            };
            to.email.to_lowercase()
        } else {
            from.to_lowercase()
        };
        let due = tags.due.and_then(|i| answer.facts.dates.get(i));
        let object = tags
            .reference
            .and_then(|i| answer.facts.references.get(i))
            .map(|r| format!("{}:{}", r.kind, r.value))
            .unwrap_or_else(|| quote.to_lowercase());
        let period = due
            .map(|d| d.at.format("%Y-%m").to_string())
            .unwrap_or_default();
        let id = format!(
            "ask_{}",
            crate::assessment_fingerprint(&format!(
                "{}|{email}|{direction}|{:?}|{object}|{period}",
                answer.account_id, tags.action
            ))
        );
        current_ids.push(id.clone());
        sqlx::query("INSERT INTO contact_commitments (id, account_id, email, thread_id, direction, status, who_owes, what, by_when, by_when_source, evidence_msg_id, extracted_at, resolved_at)
            VALUES (?, ?, ?, ?, ?, 'open', ?, ?, ?, ?, ?, ?, NULL)
            ON CONFLICT(id) DO UPDATE SET status = 'open', thread_id = excluded.thread_id, what = excluded.what, by_when = excluded.by_when, by_when_source = excluded.by_when_source, evidence_msg_id = excluded.evidence_msg_id, extracted_at = excluded.extracted_at
            WHERE contact_commitments.status != 'resolved' AND (SELECT rowid FROM messages WHERE id = contact_commitments.evidence_msg_id) <= (SELECT rowid FROM messages WHERE id = excluded.evidence_msg_id)")
            .bind(&id).bind(answer.account_id.as_str()).bind(&email).bind(&thread).bind(direction)
            .bind(if direction == "yours" { "you" } else { &email }).bind(quote).bind(due.map(|d| d.at.timestamp())).bind(due.map(|_| "words"))
            .bind(answer.message_id.as_str()).bind(date).execute(&mut **tx).await?;
    }
    // Retractions withdraw only automatically projected open rows for this
    // source; resolved and explicit manual history stay intact.
    sqlx::query("UPDATE contact_commitments SET status = 'expired' WHERE account_id = ? AND evidence_msg_id = ? AND id LIKE 'ask_%' AND status = 'open' AND id NOT IN (SELECT value FROM json_each(?))")
        .bind(answer.account_id.as_str()).bind(answer.message_id.as_str()).bind(encode_json(&current_ids)?).execute(&mut **tx).await?;
    Ok(())
}
