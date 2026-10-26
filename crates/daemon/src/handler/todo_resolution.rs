//! Evidence-backed resolution offers from related later mail.

use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_protocol::TodoResolutionData;
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{
    NewTodoResolutionSuggestion, TodoRecord, TodoResolutionAction, TodoResolutionSuggestion,
};
use regex::Regex;

mod worker;
pub(super) use worker::drain;

const MAX_QUOTE_CHARS: usize = 500;

type Candidate = (
    TodoResolutionAction,
    String,
    String,
    Option<String>,
    &'static str,
);

#[derive(Debug, Clone)]
struct RuleResolution {
    action: TodoResolutionAction,
    quote: String,
    quote_location: &'static str,
}

pub(super) async fn scan_messages(state: &AppState, message_ids: &[MessageId]) {
    if message_ids.is_empty() || !state.config_snapshot().todo.enabled {
        return;
    }
    let mut expanded_ids = message_ids.to_vec();
    let initial_rows = match state.store.list_todo_scan_rows(message_ids).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "to-do resolution scan could not read message headers");
            return;
        }
    };
    'messages: for row in &initial_rows {
        if row.outbound
            || MessageFlags::from_bits_truncate(row.flags)
                .intersects(MessageFlags::SPAM | MessageFlags::TRASH)
        {
            continue;
        }
        let Ok(tasks) = state
            .store
            .list_open_todos_for_resolution_context(
                &row.account_id,
                &row.from_email,
                &row.thread_id,
            )
            .await
        else {
            continue;
        };
        for task in tasks
            .iter()
            .filter(|task| task.source_message_id.as_ref() == Some(&row.id))
        {
            let candidates = match state
                .store
                .list_todo_resolution_candidate_messages(&task.id, 40)
                .await
            {
                Ok(candidates) => candidates,
                Err(error) => {
                    tracing::warn!(todo_id = %task.id, %error, "cached resolution candidates could not be listed");
                    continue;
                }
            };
            for candidate in candidates {
                if !expanded_ids.contains(&candidate) {
                    expanded_ids.push(candidate);
                    if expanded_ids.len() >= 200 {
                        break 'messages;
                    }
                }
            }
        }
    }
    let rows = if expanded_ids.len() == message_ids.len() {
        initial_rows
    } else {
        match state.store.list_todo_scan_rows(&expanded_ids).await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "to-do resolution scan could not read cached candidate headers");
                initial_rows
            }
        }
    };
    for row in rows {
        let flags = MessageFlags::from_bits_truncate(row.flags);
        if row.outbound || flags.intersects(MessageFlags::SPAM | MessageFlags::TRASH) {
            continue;
        }
        let tasks = match state
            .store
            .list_open_todos_for_resolution_context(
                &row.account_id,
                &row.from_email,
                &row.thread_id,
            )
            .await
        {
            Ok(tasks) if !tasks.is_empty() => tasks,
            Ok(_) => continue,
            Err(error) => {
                tracing::warn!(message = %row.id, %error, "to-do resolution scan could not read tasks");
                continue;
            }
        };
        let Some(body) = (match state.store.get_body(&row.id).await {
            Ok(body) => body,
            Err(error) => {
                tracing::warn!(message = %row.id, %error, "to-do resolution scan could not read body");
                continue;
            }
        }) else {
            continue;
        };
        let text = clean(
            body.text_plain.as_deref(),
            body.text_html.as_deref(),
            &ReaderConfig::default(),
        )
        .content;
        for task in tasks {
            if task.source_message_id.as_ref() == Some(&row.id)
                || task.source_date.is_some_and(|date| {
                    row.date < date
                        || (row.date == date
                            && task
                                .source_message_id
                                .as_ref()
                                .is_some_and(|source| row.id.as_str() <= source.as_str()))
                })
            {
                continue;
            }
            let task_fingerprint = mxr_store::todo_resolution_fingerprint(&task);
            let contradiction = rule_contradiction(&task, &row.subject, &text);
            if contradiction {
                if let Err(error) = state
                    .store
                    .record_todo_resolution_contradiction(
                        &task.id,
                        &task.account_id,
                        &row.id,
                        row.date,
                        &task_fingerprint,
                    )
                    .await
                {
                    tracing::warn!(todo_id = %task.id, %error, "to-do resolution contradiction was not stored");
                }
                continue;
            }
            let rule = rule_resolution(&task, &row.subject, &text, row.date);
            let candidate = match rule {
                Some(rule) => Some((
                    rule.action,
                    rule.quote,
                    rule.quote_location.to_string(),
                    None,
                    "rule",
                )),
                None => {
                    if task.has_stored_asks() {
                        continue;
                    }
                    match state.store.todo_resolution_input(&task.id, &row.id).await {
                        Ok(Some(input)) => {
                            if let Err(error) = state
                                .store
                                .enqueue_todo_resolution(&input, Utc::now())
                                .await
                            {
                                tracing::warn!(todo_id = %task.id, %error, "to-do resolution work was not queued");
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            tracing::warn!(todo_id = %task.id, %error, "to-do resolution input could not be read");
                        }
                    }
                    continue;
                }
            };
            let Some((action, quote, quote_location, due_words, source_method)) = candidate else {
                continue;
            };
            if let Err(error) = offer_resolution(
                state,
                &task,
                &row.id,
                &row.thread_id,
                row.date,
                (action, quote, quote_location, due_words, source_method),
                None,
            )
            .await
            {
                tracing::warn!(todo_id = %task.id, %error, "to-do resolution offer was not stored");
            }
        }
    }
}

async fn offer_resolution(
    state: &AppState,
    task: &TodoRecord,
    message_id: &MessageId,
    thread_id: &mxr_core::id::ThreadId,
    date: DateTime<Utc>,
    candidate: Candidate,
    work: Option<&mxr_store::TodoResolutionWork>,
) -> Result<bool, sqlx::Error> {
    let (action, quote, quote_location, due_words, source_method) = candidate;
    if quote.chars().count() > MAX_QUOTE_CHARS {
        return Ok(true);
    }
    let proposed_due_at = match due_words.as_deref() {
        Some(words) => match super::todos::resolve_phrase(state, words, date, None) {
            Ok(at) if action == TodoResolutionAction::ReviseDate && at > date => Some(at),
            _ => return Ok(true),
        },
        None if action == TodoResolutionAction::Complete => None,
        None => return Ok(true),
    };
    let suggestion = NewTodoResolutionSuggestion {
        id: format!("todo_resolution_{}", uuid::Uuid::new_v4().simple()),
        todo_id: task.id.clone(),
        account_id: task.account_id.clone(),
        source_message_id: message_id.clone(),
        source_thread_id: thread_id.clone(),
        source_date: date,
        source_quote: quote,
        quote_location,
        source_method: source_method.into(),
        action,
        proposed_due_at,
        proposed_due_words: due_words,
        task_updated_at: task.updated_at,
        task_fingerprint: mxr_store::todo_resolution_fingerprint(task),
        created_at: Utc::now(),
    };
    if let Some(work) = work {
        state
            .store
            .offer_queued_todo_resolution(&suggestion, work)
            .await
    } else {
        state.store.offer_todo_resolution(&suggestion).await
    }
}

fn rule_resolution(
    task: &TodoRecord,
    subject: &str,
    body: &str,
    date: DateTime<Utc>,
) -> Option<RuleResolution> {
    let matches_object = object_matches(task, subject, body);
    if !matches_object {
        return None;
    }
    if let Some(amount) = task.amount_minor {
        let evidence = format!("{subject}\n{body}");
        let amounts = mxr_todo::money::find_amounts(&evidence);
        if !amounts.is_empty()
            && !amounts.iter().any(|(_, found)| {
                found.minor == amount
                    && task
                        .currency
                        .as_deref()
                        .is_none_or(|currency| currency == found.currency)
            })
        {
            return None;
        }
    }
    let kind = mxr_todo::TodoKind::parse(&task.kind)?;
    let patterns: &[&str] = match kind {
        mxr_todo::TodoKind::Bill | mxr_todo::TodoKind::PaymentFailed => &[
            r"(?i)\b(payment (received|confirmation|confirmed|successful|complete)|thank(s| you) for (your )?payment|we('ve| have) received your payment|your receipt|receipt for|has been paid|paid in full)\b",
        ],
        mxr_todo::TodoKind::Renewal | mxr_todo::TodoKind::Document => &[
            r"(?i)\b(renewed|thank(s| you) for renewing|renewal (confirmation|confirmed)|policy documents)\b",
        ],
        mxr_todo::TodoKind::Verify => {
            if task
                .source_date
                .is_some_and(|source| date - source > Duration::days(1))
            {
                return None;
            }
            &[r"(?i)\b(verified|email (address )?confirmed|account (is )?(now )?active)\b"]
        }
        mxr_todo::TodoKind::Sign => {
            &[r"(?i)\b(completed|signed by all|all parties have signed|has been signed)\b"]
        }
        mxr_todo::TodoKind::Other => &[r"(?i)\b(booking|reservation) (is )?confirmed\b"],
        _ => return None,
    };
    let affirmative = [("subject", subject), ("body", body)]
        .into_iter()
        .filter_map(|(location, text)| find_safe_quote(patterns, text, location))
        .find(|(quote, _)| {
            object_matches_fields(&task.title, task.counterparty.as_deref(), "", quote)
        });
    if let Some((quote, quote_location)) = affirmative {
        return Some(RuleResolution {
            action: TodoResolutionAction::Complete,
            quote,
            quote_location,
        });
    }
    // A cancelled direct debit or failed bill payment does not discharge the
    // underlying obligation. Cancellation can resolve subscriptions and
    // other tasks, but payment obligations need affirmative payment evidence.
    if matches!(
        kind,
        mxr_todo::TodoKind::Bill | mxr_todo::TodoKind::PaymentFailed
    ) {
        return None;
    }
    let cancellation = [
        r"(?i)\b(cancelled|canceled|has been cancelled|has been canceled)\b",
        r"(?i)\b(no longer need|not required anymore|not needed anymore)\b",
    ];
    [("subject", subject), ("body", body)]
        .into_iter()
        .filter_map(|(location, text)| find_safe_quote(&cancellation, text, location))
        .find(|(quote, _)| {
            object_matches_fields(&task.title, task.counterparty.as_deref(), "", quote)
        })
        .map(|(quote, quote_location)| RuleResolution {
            action: TodoResolutionAction::Complete,
            quote,
            quote_location,
        })
}

fn find_safe_quote(
    patterns: &[&str],
    text: &str,
    location: &'static str,
) -> Option<(String, &'static str)> {
    for pattern in patterns {
        let regex = Regex::new(pattern).ok()?;
        for found in regex.find_iter(text) {
            let prefix = &text[..found.start()];
            let sentence_start = previous_quote_boundary(text, found.start()).unwrap_or(0);
            let context = prefix[sentence_start..].to_ascii_lowercase();
            let sentence_end = next_quote_boundary(text, found.end()).unwrap_or(text.len());
            let suffix = text[found.end()..sentence_end].to_ascii_lowercase();
            let quote = text[sentence_start..sentence_end].trim();
            if [
                "not ",
                "never ",
                "if ",
                "unless ",
                "when ",
                "will ",
                "may ",
                "might ",
                "could ",
                "hasn't ",
                "has not ",
                "haven't ",
                "have not ",
                "didn't ",
                "did not ",
                "wasn't ",
                "was not ",
                "weren't ",
                "were not ",
            ]
            .iter()
            .any(|word| context.contains(word))
                || [" failed", " unsuccessful", " not received", " not paid"]
                    .iter()
                    .any(|word| suffix.contains(word))
            {
                continue;
            }
            if quote.chars().count() <= MAX_QUOTE_CHARS {
                return Some((quote.to_string(), location));
            }
        }
    }
    None
}

fn previous_quote_boundary(text: &str, before: usize) -> Option<usize> {
    text[..before]
        .char_indices()
        .rfind(|(index, character)| is_quote_boundary(text, *index, *character))
        .map(|(index, character)| index + character.len_utf8())
}

fn next_quote_boundary(text: &str, from: usize) -> Option<usize> {
    text[from..]
        .char_indices()
        .find(|(offset, character)| is_quote_boundary(text, from + *offset, *character))
        .map(|(offset, character)| from + offset + character.len_utf8())
}

fn is_quote_boundary(text: &str, index: usize, character: char) -> bool {
    match character {
        '.' => {
            let previous_is_digit = text[..index]
                .chars()
                .next_back()
                .is_some_and(|neighbor| neighbor.is_ascii_digit());
            let next_is_digit = text[index + character.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|neighbor| neighbor.is_ascii_digit());
            !(previous_is_digit && next_is_digit)
        }
        '!' | '?' | '\n' => true,
        _ => false,
    }
}

fn object_matches(task: &TodoRecord, subject: &str, body: &str) -> bool {
    object_matches_fields(&task.title, task.counterparty.as_deref(), subject, body)
}

fn object_matches_fields(
    title: &str,
    counterparty: Option<&str>,
    subject: &str,
    body: &str,
) -> bool {
    let evidence = format!("{subject}\n{body}").to_ascii_lowercase();
    let merchant = counterparty
        .unwrap_or_default()
        .split_whitespace()
        .map(clean_token)
        .collect::<Vec<_>>();
    let object = title
        .split_whitespace()
        .skip(1)
        .map(clean_token)
        .filter(|token| {
            token.chars().count() >= 3
                && !merchant.contains(token)
                && !matches!(
                    token.as_str(),
                    "the"
                        | "all"
                        | "customer"
                        | "customers"
                        | "your"
                        | "you"
                        | "this"
                        | "that"
                        | "with"
                        | "from"
                        | "payment"
                        | "account"
                        | "service"
                        | "subscription"
                        | "renewal"
                        | "plan"
                        | "membership"
                )
        })
        .collect::<Vec<_>>();
    let evidence_tokens = evidence
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    !object.is_empty()
        && object.into_iter().all(|token| {
            evidence_tokens.iter().any(|evidence_token| {
                *evidence_token == token
                    || evidence_token
                        .strip_suffix('s')
                        .is_some_and(|singular| singular == token)
            })
        })
}

fn rule_contradiction(task: &TodoRecord, subject: &str, body: &str) -> bool {
    let kind = mxr_todo::TodoKind::parse(&task.kind);
    let patterns: &[&str] = match kind {
        Some(mxr_todo::TodoKind::Bill | mxr_todo::TodoKind::PaymentFailed) => &[
            r"(?i)\b(payment (has not|hasn't|was not|wasn't) received|payment failed|not paid|never paid|hasn't been paid|has not been paid)\b",
        ],
        Some(mxr_todo::TodoKind::Renewal) => &[
            r"(?i)\b(has not|hasn't|have not|haven't|was not|wasn't) been renewed\b",
            r"(?i)\brenewal (failed|was declined|was not completed)\b",
        ],
        Some(
            mxr_todo::TodoKind::Verify | mxr_todo::TodoKind::Sign | mxr_todo::TodoKind::Document,
        ) => &[
            r"(?i)\b(has not|hasn't|have not|haven't|was not|wasn't) (been )?(verified|signed|completed|processed)\b",
        ],
        _ => &[],
    };
    has_object_bound_contradiction(
        &task.title,
        task.counterparty.as_deref(),
        patterns,
        subject,
        body,
    )
}

fn has_object_bound_contradiction(
    title: &str,
    counterparty: Option<&str>,
    patterns: &[&str],
    subject: &str,
    body: &str,
) -> bool {
    [subject, body].into_iter().any(|text| {
        text.split(['.', '!', '?', '\n']).any(|sentence| {
            object_matches_fields(title, counterparty, "", sentence)
                && patterns.iter().any(|pattern| {
                    Regex::new(pattern)
                        .ok()
                        .is_some_and(|regex| regex.is_match(sentence))
                })
        })
    })
}

fn clean_token(token: &str) -> String {
    token
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_ascii_lowercase()
}

pub(super) fn resolution_data(suggestion: &TodoResolutionSuggestion) -> TodoResolutionData {
    TodoResolutionData {
        id: suggestion.id.clone(),
        message_id: suggestion.source_message_id.clone(),
        thread_id: suggestion.source_thread_id.clone(),
        source_date: suggestion.source_date,
        quote: suggestion.source_quote.clone(),
        quote_location: suggestion.quote_location.clone(),
        source_method: suggestion.source_method.clone(),
        action: suggestion.action.as_str().into(),
        proposed_due_at: suggestion.proposed_due_at,
        proposed_due_words: suggestion.proposed_due_words.clone(),
        status: suggestion.status.as_str().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merchant_name_alone_does_not_match_a_different_plan() {
        assert!(!object_matches_fields(
            "Renew JetBrains All Products",
            Some("JetBrains"),
            "JetBrains RustRover renewed",
            "Your JetBrains subscription is active.",
        ));
        let quote = find_safe_quote(
            &[r"(?i)\brenewed\b"],
            "Your All Products subscription expires next week. RustRover has been renewed successfully.",
            "body",
        ).unwrap();
        assert!(!object_matches_fields(
            "Renew JetBrains All Products",
            Some("JetBrains"),
            "",
            &quote.0,
        ));
        assert!(!object_matches_fields(
            "Renew JetBrains All Products",
            Some("JetBrains"),
            "All JetBrains customers: RustRover renewed",
            "The subscription remains active.",
        ));
        assert!(object_matches_fields(
            "Renew JetBrains All Products",
            Some("JetBrains"),
            "JetBrains All Products renewed",
            "Your plan is active.",
        ));
    }

    #[test]
    fn quote_keeps_the_verbatim_object_and_outcome_and_rejects_negation() {
        let renewed = [r"(?i)\brenewed\b"];
        let (quote, location) = find_safe_quote(
            &renewed,
            "JetBrains All Products has been renewed successfully.",
            "body",
        )
        .expect("affirmative quote");
        assert_eq!(location, "body");
        assert_eq!(
            quote,
            "JetBrains All Products has been renewed successfully."
        );
        assert!(find_safe_quote(
            &renewed,
            "JetBrains All Products hasn't been renewed.",
            "body",
        )
        .is_none());
    }

    #[test]
    fn decimal_amounts_do_not_split_the_quote_or_hide_negation() {
        let payment_received = [r"(?i)\bpayment received\b"];
        let text = "Camden Council Tax payment received for £142.00.";
        let (quote, _) = find_safe_quote(&payment_received, text, "body")
            .expect("affirmative quote with decimal amount");
        assert_eq!(quote, text);

        assert!(find_safe_quote(
            &payment_received,
            "Do not mark £142.00 complete: payment received.",
            "body",
        )
        .is_none());
        assert!(find_safe_quote(
            &payment_received,
            "Payment received for £142.00, but the transfer failed.",
            "body",
        )
        .is_none());
    }

    #[test]
    fn contradiction_must_refer_to_the_same_plan_as_the_task() {
        let patterns = [r"(?i)\b(has not|hasn't) been renewed\b"];
        assert!(!has_object_bound_contradiction(
            "Renew JetBrains All Products",
            Some("JetBrains"),
            &patterns,
            "Your All Products subscription remains active.",
            "RustRover hasn't been renewed.",
        ));
        assert!(has_object_bound_contradiction(
            "Renew JetBrains All Products",
            Some("JetBrains"),
            &patterns,
            "",
            "JetBrains All Products hasn't been renewed.",
        ));
    }
}
