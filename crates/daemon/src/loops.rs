use crate::state::AppState;
use mxr_protocol::*;
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub async fn sync_loop(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
        {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Sync completed: {} messages", count);
                    let event = IpcMessage {
                        id: 0,
                        payload: IpcPayload::Event(DaemonEvent::SyncCompleted {
                            account_id: state.provider.account_id().clone(),
                            messages_synced: count,
                        }),
                    };
                    let _ = state.event_tx.send(event);
                }
            }
            Err(e) => {
                tracing::error!("Sync error: {}", e);
                let event = IpcMessage {
                    id: 0,
                    payload: IpcPayload::Event(DaemonEvent::SyncError {
                        account_id: state.provider.account_id().clone(),
                        error: e.to_string(),
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
