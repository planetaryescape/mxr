use crate::Store;
use mxr_core::id::AccountId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

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
    pub fn start(store: Arc<Store>) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel(16);
        let pending = Arc::new(Mutex::new(HashSet::<AccountId>::new()));
        let handle = tokio::spawn({
            let pending = pending.clone();
            async move {
                while let Some(command) = rx.recv().await {
                    match command {
                        ContactsRefreshCommand::Enqueue { account_ids, resp } => {
                            if let Ok(mut pending) = pending.lock() {
                                pending.extend(account_ids);
                            }
                            let _ = resp.send(Ok(()));
                            sleep(Duration::from_secs(10)).await;
                            let drained = pending
                                .lock()
                                .map(|mut pending| pending.drain().collect::<Vec<_>>())
                                .unwrap_or_default();
                            if drained.is_empty() {
                                continue;
                            }
                            if let Err(error) = store.refresh_contacts().await {
                                tracing::warn!(%error, "contacts refresh failed");
                            }
                        }
                        ContactsRefreshCommand::Shutdown { resp } => {
                            let _ = resp.send(());
                            break;
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
