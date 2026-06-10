#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::DaemonEvent;

pub fn event_matches_type(event: &DaemonEvent, event_type: Option<&str>) -> bool {
    let Some(event_type) = event_type else {
        return true;
    };

    match event_type {
        "sync" => matches!(
            event,
            DaemonEvent::SyncCompleted { .. } | DaemonEvent::SyncError { .. }
        ),
        "message" => matches!(event, DaemonEvent::NewMessages { .. }),
        "snooze" => matches!(event, DaemonEvent::MessageUnsnoozed { .. }),
        "operation" => matches!(
            event,
            DaemonEvent::OperationStarted { .. }
                | DaemonEvent::OperationProgress { .. }
                | DaemonEvent::OperationCompleted { .. }
                | DaemonEvent::OperationFailed { .. }
                | DaemonEvent::OperationCancelled { .. }
        ),
        "mutation" => matches!(event, DaemonEvent::MutationReconciliationFailed { .. }),
        "error" => matches!(
            event,
            DaemonEvent::SyncError { .. }
                | DaemonEvent::OperationFailed { .. }
                | DaemonEvent::MutationReconciliationFailed { .. }
        ),
        _ => false,
    }
}

pub fn render_event(event: &DaemonEvent, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json | OutputFormat::Jsonl => serde_json::to_string(event)?,
        _ => match event {
            DaemonEvent::SyncCompleted {
                account_id,
                messages_synced,
            } => format!(
                "sync account={account_id} messages_synced={messages_synced}"
            ),
            DaemonEvent::SyncError { account_id, error } => {
                format!("error account={account_id} {error}")
            }
            DaemonEvent::NewMessages { envelopes } => {
                format!("message new_messages={}", envelopes.len())
            }
            DaemonEvent::MessageUnsnoozed { message_id } => {
                format!("snooze message_unsnoozed={message_id}")
            }
            DaemonEvent::ReminderTriggered { sent_message_id } => {
                format!("reminder reminder_triggered={sent_message_id}")
            }
            DaemonEvent::LabelCountsUpdated { counts } => {
                format!("sync label_counts_updated={}", counts.len())
            }
            DaemonEvent::OperationStarted {
                operation_id,
                operation,
                message,
                ..
            } => format!("operation started id={operation_id} operation={operation} {message}"),
            DaemonEvent::OperationProgress {
                operation_id,
                operation,
                current,
                total,
                message,
                ..
            } => {
                let total = total.map_or_else(|| "?".into(), |value| value.to_string());
                format!(
                    "operation progress id={operation_id} operation={operation} current={current} total={total} {message}"
                )
            }
            DaemonEvent::OperationCompleted {
                operation_id,
                operation,
                message,
                ..
            } => format!("operation completed id={operation_id} operation={operation} {message}"),
            DaemonEvent::OperationFailed {
                operation_id,
                operation,
                error,
                retryable,
                ..
            } => {
                format!("operation failed id={operation_id} operation={operation} retryable={retryable} {error}")
            }
            DaemonEvent::OperationCancelled {
                operation_id,
                operation,
                message,
                ..
            } => format!("operation cancelled id={operation_id} operation={operation} {message}"),
            DaemonEvent::MutationReconciliationFailed {
                client_correlation_id,
                error_summary,
            } => format!(
                "mutation reconciliation_failed correlation_id={client_correlation_id} {error_summary}"
            ),
            DaemonEvent::EventsLagged { skipped } => {
                format!("events lagged skipped={skipped}")
            }
        },
    })
}

pub async fn run(event_type: Option<String>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    let mut client = IpcClient::connect().await?;

    loop {
        let event = client.next_event().await?;
        if event_matches_type(&event, event_type.as_deref()) {
            println!("{}", render_event(&event, fmt.clone())?);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::{id::AccountId, MessageId};

    #[test]
    fn mutation_filter_matches_reconciliation_failed() {
        let event = DaemonEvent::MutationReconciliationFailed {
            client_correlation_id: "9".into(),
            error_summary: "skipped".into(),
        };
        assert!(event_matches_type(&event, Some("mutation")));
        assert!(event_matches_type(&event, Some("error")));
        assert!(!event_matches_type(&event, Some("sync")));
    }

    #[test]
    fn sync_filter_matches_sync_events() {
        let event = DaemonEvent::SyncCompleted {
            account_id: AccountId::new(),
            messages_synced: 4,
        };
        assert!(event_matches_type(&event, Some("sync")));
        assert!(!event_matches_type(&event, Some("message")));
    }

    #[test]
    fn error_filter_matches_sync_error() {
        let event = DaemonEvent::SyncError {
            account_id: AccountId::new(),
            error: "boom".into(),
        };
        assert!(event_matches_type(&event, Some("error")));
    }

    #[test]
    fn render_table_event_is_human_readable() {
        let event = DaemonEvent::MessageUnsnoozed {
            message_id: MessageId::new(),
        };
        let rendered = render_event(&event, OutputFormat::Table).unwrap();
        assert!(rendered.contains("message_unsnoozed"));
    }
}
