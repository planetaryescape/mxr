use crate::stylometry::{aggregate_metrics, WeightedText};
use anyhow::Result;
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::MessageDirection;
use mxr_store::{ContactStyleRecord, Store};
use serde_json::json;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

#[derive(Clone)]
pub struct RelationshipServiceHandle {
    tx: mpsc::Sender<RelationshipCommand>,
}

enum RelationshipCommand {
    EnqueueMessages {
        message_ids: Vec<MessageId>,
        resp: oneshot::Sender<Result<()>>,
    },
    RebuildContact {
        account_id: AccountId,
        email: String,
        resp: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        resp: oneshot::Sender<()>,
    },
}

impl RelationshipServiceHandle {
    pub fn start(store: Arc<Store>) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel(32);
        let handle = tokio::spawn(async move {
            let mut pending = VecDeque::<(AccountId, String)>::new();
            let mut pending_keys = HashSet::<(String, String)>::new();
            loop {
                while let Some((account_id, email)) = pending.pop_front() {
                    pending_keys.remove(&(account_id.as_str(), email.clone()));
                    sleep(Duration::from_millis(250)).await;
                    if let Err(error) = rebuild_contact_style(&store, &account_id, &email).await {
                        tracing::warn!(%account_id, %email, %error, "relationship profile refresh failed");
                    }
                }
                let Some(command) = rx.recv().await else {
                    break;
                };
                match command {
                    RelationshipCommand::EnqueueMessages { message_ids, resp } => {
                        match store.relationship_contacts_for_messages(&message_ids).await {
                            Ok(contacts) => {
                                for (account_id, email) in contacts {
                                    let key = (account_id.as_str(), email.clone());
                                    if pending_keys.insert(key) {
                                        pending.push_back((account_id, email));
                                    }
                                }
                                let _ = resp.send(Ok(()));
                            }
                            Err(error) => {
                                let _ = resp.send(Err(error.into()));
                            }
                        }
                    }
                    RelationshipCommand::RebuildContact {
                        account_id,
                        email,
                        resp,
                    } => {
                        let result = rebuild_contact_style(&store, &account_id, &email).await;
                        let _ = resp.send(result);
                    }
                    RelationshipCommand::Shutdown { resp } => {
                        let _ = resp.send(());
                        break;
                    }
                }
            }
        });
        (Self { tx }, handle)
    }

    pub async fn enqueue_contacts_from_messages(&self, message_ids: &[MessageId]) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(RelationshipCommand::EnqueueMessages {
                message_ids: message_ids.to_vec(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("relationship service unavailable"))?;
        resp_rx
            .await
            .map_err(|_| anyhow::anyhow!("relationship service stopped"))?
    }

    pub async fn rebuild_contact(&self, account_id: AccountId, email: String) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(RelationshipCommand::RebuildContact {
                account_id,
                email,
                resp: resp_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("relationship service unavailable"))?;
        resp_rx
            .await
            .map_err(|_| anyhow::anyhow!("relationship service stopped"))?
    }

    pub async fn request_shutdown(&self) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(RelationshipCommand::Shutdown { resp: resp_tx })
            .await
            .map_err(|_| anyhow::anyhow!("relationship service unavailable"))?;
        resp_rx
            .await
            .map_err(|_| anyhow::anyhow!("relationship service stopped"))?;
        Ok(())
    }
}

pub async fn rebuild_contact_style(
    store: &Store,
    account_id: &AccountId,
    email: &str,
) -> Result<()> {
    let samples = store
        .recent_contact_messages(account_id, email, 100)
        .await?;
    let mut yours = Vec::new();
    let mut theirs = Vec::new();
    for sample in &samples {
        let text = WeightedText {
            id: sample.message_id.as_str(),
            text: &sample.body,
            date: sample.date,
        };
        if sample.direction == MessageDirection::Outbound {
            yours.push(text);
        } else if sample.from_email.eq_ignore_ascii_case(email) {
            theirs.push(text);
        }
    }
    if yours.len() < 1 || theirs.is_empty() || yours.len() + theirs.len() < 5 {
        return Ok(());
    }
    let (your_metrics, your_hash) = aggregate_metrics(&yours);
    let (their_metrics, their_hash) = aggregate_metrics(&theirs);
    let source_hash = format!("{your_hash}:{their_hash}");
    let record = ContactStyleRecord {
        account_id: account_id.clone(),
        email: email.to_ascii_lowercase(),
        formality_score: your_metrics.formality_score,
        formality_score_theirs: their_metrics.formality_score,
        avg_sentence_len: your_metrics.avg_sentence_len,
        avg_sentence_len_theirs: their_metrics.avg_sentence_len,
        msg_count_used: yours.len() as u32,
        msg_count_used_theirs: theirs.len() as u32,
        metrics_json: serde_json::to_string(&json!(your_metrics))?,
        metrics_json_theirs: serde_json::to_string(&json!(their_metrics))?,
        computed_at: chrono::Utc::now(),
        source_hash,
    };
    store.upsert_contact_style(&record).await?;
    Ok(())
}
