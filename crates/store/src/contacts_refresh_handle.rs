use crate::Store;
use mxr_core::id::AccountId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::Duration;

/// Debounce window after the most recent enqueue before the worker
/// actually runs `refresh_contacts`. Coalesces a burst of sync ticks
/// during heavy traffic into a single refresh — the user pays the
/// writer cost once per quiet period instead of once per sync tick.
const DEBOUNCE: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct ContactsRefreshHandle {
    tx: mpsc::Sender<ContactsRefreshCommand>,
}

enum ContactsRefreshCommand {
    Enqueue {
        account_ids: Vec<AccountId>,
        resp: oneshot::Sender<Result<(), String>>,
    },
    Shutdown {
        resp: oneshot::Sender<()>,
    },
}

impl ContactsRefreshHandle {
    /// `background_db` gates the refresh below the reader-pool size. The
    /// `refresh_contacts` aggregate scans the whole `messages` table and
    /// pins a reader connection for its full duration; holding a permit
    /// across it guarantees interactive/status queries keep their
    /// headroom.
    pub fn start(store: Arc<Store>, background_db: Arc<Semaphore>) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel(16);
        let pending = Arc::new(Mutex::new(HashSet::<AccountId>::new()));
        let handle = tokio::spawn({
            let pending = pending.clone();
            async move {
                // Trailing-edge debounce: each enqueue resets the timer.
                // The previous design slept *inside* the command handler,
                // which blocked the mpsc queue for the full debounce
                // window and caused new enqueues to backpressure (or
                // worse, force the sender — including the post-sync
                // fan-out — to wait synchronously). With a `select!`
                // loop we keep draining commands at line rate while the
                // refresh is gated on its own timer.
                let mut debounce: Option<tokio::time::Instant> = None;
                loop {
                    if let Some(deadline) = debounce {
                        tokio::select! {
                            biased;
                            command = rx.recv() => match command {
                                Some(ContactsRefreshCommand::Enqueue { account_ids, resp }) => {
                                    if let Ok(mut pending) = pending.lock() {
                                        pending.extend(account_ids);
                                    }
                                    let _ = resp.send(Ok(()));
                                    debounce = Some(tokio::time::Instant::now() + DEBOUNCE);
                                }
                                Some(ContactsRefreshCommand::Shutdown { resp }) => {
                                    let _ = resp.send(());
                                    return;
                                }
                                None => return,
                            },
                            _ = tokio::time::sleep_until(deadline) => {
                                debounce = None;
                                let drained = pending
                                    .lock()
                                    .map(|mut pending| pending.drain().collect::<Vec<_>>())
                                    .unwrap_or_default();
                                if drained.is_empty() {
                                    continue;
                                }
                                let _bg_permit = background_db.acquire().await;
                                if let Err(error) = store.refresh_contacts().await {
                                    tracing::warn!(%error, "contacts refresh failed");
                                }
                            }
                        }
                    } else {
                        match rx.recv().await {
                            Some(ContactsRefreshCommand::Enqueue { account_ids, resp }) => {
                                if let Ok(mut pending) = pending.lock() {
                                    pending.extend(account_ids);
                                }
                                let _ = resp.send(Ok(()));
                                debounce = Some(tokio::time::Instant::now() + DEBOUNCE);
                            }
                            Some(ContactsRefreshCommand::Shutdown { resp }) => {
                                let _ = resp.send(());
                                return;
                            }
                            None => return,
                        }
                    }
                }
            }
        });
        (Self { tx }, handle)
    }

    pub async fn enqueue_accounts(&self, account_ids: &[AccountId]) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContactsRefreshCommand::Enqueue {
                account_ids: account_ids.to_vec(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| "contacts refresh worker unavailable".to_string())?;
        resp_rx
            .await
            .map_err(|_| "contacts refresh worker stopped".to_string())?
    }

    pub async fn request_shutdown(&self) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContactsRefreshCommand::Shutdown { resp: resp_tx })
            .await
            .map_err(|_| "contacts refresh worker unavailable".to_string())?;
        resp_rx
            .await
            .map_err(|_| "contacts refresh worker stopped".to_string())
    }
}
