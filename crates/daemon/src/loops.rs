use crate::state::AppState;
use mxr_core::types::SyncCursor;
use mxr_protocol::*;
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub async fn sync_loop(state: Arc<AppState>) {
    let base_interval = state.config.general.sync_interval.max(30);
    let mut backoff_secs: u64 = 0;

    // Always start syncing immediately — no initial delay.
    // The daemon accepts clients right away; messages appear as they sync.
    let mut skip_sleep = true;

    loop {
        if skip_sleep {
            skip_sleep = false;
        } else {
            let wait = if backoff_secs > 0 {
                tracing::info!("Rate limited, backing off {backoff_secs}s");
                backoff_secs
            } else {
                base_interval
            };
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }

        match state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
        {
            Ok(count) => {
                backoff_secs = 0; // Reset backoff on success
                if count > 0 {
                    tracing::info!("Sync completed: {count} messages");
                    let event = IpcMessage {
                        id: 0,
                        payload: IpcPayload::Event(DaemonEvent::SyncCompleted {
                            account_id: state.provider.account_id().clone(),
                            messages_synced: count,
                        }),
                    };
                    let _ = state.event_tx.send(event);

                    // Broadcast updated label counts so TUI sidebar refreshes live
                    if let Ok(labels) = state
                        .store
                        .list_labels_by_account(state.provider.account_id())
                        .await
                    {
                        let counts: Vec<_> = labels
                            .iter()
                            .map(|l| LabelCount {
                                label_id: l.id.clone(),
                                unread_count: l.unread_count,
                                total_count: l.total_count,
                            })
                            .collect();
                        let counts_event = IpcMessage {
                            id: 0,
                            payload: IpcPayload::Event(DaemonEvent::LabelCountsUpdated { counts }),
                        };
                        let _ = state.event_tx.send(counts_event);
                    }
                }

                // Fast-cycle during backfill: re-sync after 2s instead of full interval
                if let Ok(Some(cursor)) = state
                    .store
                    .get_sync_cursor(state.provider.account_id())
                    .await
                {
                    if matches!(cursor, SyncCursor::GmailBackfill { .. }) {
                        tracing::info!("Backfill in progress, re-syncing in 2s");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        skip_sleep = true;
                        continue;
                    }
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                // Parse rate limit retry-after if present
                if err_str.contains("Rate limited") {
                    // Extract retry_after from "retry after Xs"
                    let secs = err_str
                        .split("retry after ")
                        .nth(1)
                        .and_then(|s| s.trim_end_matches('s').parse::<u64>().ok())
                        .unwrap_or(120);
                    backoff_secs = secs + 10; // Add buffer
                } else {
                    // Exponential backoff for other errors, cap at 5 min
                    backoff_secs = (backoff_secs * 2).clamp(30, 300);
                }
                tracing::error!("Sync error: {err_str}");
                let event = IpcMessage {
                    id: 0,
                    payload: IpcPayload::Event(DaemonEvent::SyncError {
                        account_id: state.provider.account_id().clone(),
                        error: err_str,
                    }),
                };
                let _ = state.event_tx.send(event);
            }
        }
    }
}

pub async fn snooze_loop(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match state.sync_engine.check_snoozes().await {
            Ok(woken) => {
                for message_id in woken {
                    let event = IpcMessage {
                        id: 0,
                        payload: IpcPayload::Event(DaemonEvent::MessageUnsnoozed { message_id }),
                    };
                    let _ = state.event_tx.send(event);
                }
            }
            Err(e) => {
                tracing::error!("Snooze check error: {}", e);
            }
        }
    }
}
