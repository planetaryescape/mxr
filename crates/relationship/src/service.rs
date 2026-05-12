use crate::stylometry::{aggregate_metrics, WeightedText};
use anyhow::Result;
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::MessageDirection;
use mxr_llm::LlmRuntime;
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{ContactStyleRecord, Store, UserVoiceProfileRecord, UserVoiceRegisterMode};
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
    pub fn start(store: Arc<Store>, llm: Arc<LlmRuntime>) -> (Self, JoinHandle<()>) {
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
                    if let Err(error) = crate::summary::generate_relationship_summary(
                        &store,
                        &llm,
                        &account_id,
                        &email,
                    )
                    .await
                    {
                        tracing::warn!(%account_id, %email, %error, "relationship summary refresh failed");
                    }
                    if let Err(error) =
                        crate::commitments::extract_commitments(&store, &llm, &account_id, &email)
                            .await
                    {
                        tracing::warn!(%account_id, %email, %error, "commitment extraction failed");
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
                        let result =
                            rebuild_contact_style_with_options(&store, &account_id, &email, false)
                                .await;
                        if result.is_ok() {
                            if let Err(error) = crate::summary::generate_relationship_summary(
                                &store,
                                &llm,
                                &account_id,
                                &email,
                            )
                            .await
                            {
                                tracing::warn!(%account_id, %email, %error, "relationship summary rebuild failed");
                            }
                            if let Err(error) = crate::commitments::extract_commitments(
                                &store,
                                &llm,
                                &account_id,
                                &email,
                            )
                            .await
                            {
                                tracing::warn!(%account_id, %email, %error, "commitment rebuild failed");
                            }
                        }
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
    rebuild_contact_style_with_options(store, account_id, email, true).await
}

async fn rebuild_contact_style_with_options(
    store: &Store,
    account_id: &AccountId,
    email: &str,
    detect_drift: bool,
) -> Result<()> {
    let samples = store
        .recent_contact_messages(account_id, email, 100)
        .await?;
    let mut yours_cleaned = Vec::new();
    let mut theirs_cleaned = Vec::new();
    let reader_config = ReaderConfig::default();
    for sample in &samples {
        if sample.is_list_sender {
            continue;
        }
        let cleaned = clean(Some(&sample.body), None, &reader_config).content;
        if sample.direction == MessageDirection::Outbound {
            yours_cleaned.push((sample.message_id.as_str(), cleaned, sample.date));
        } else if sample.from_email.eq_ignore_ascii_case(email) {
            theirs_cleaned.push((sample.message_id.as_str(), cleaned, sample.date));
        }
    }
    let yours: Vec<_> = yours_cleaned
        .iter()
        .map(|(id, text, date)| WeightedText {
            id: id.clone(),
            text,
            date: *date,
        })
        .collect();
    let theirs: Vec<_> = theirs_cleaned
        .iter()
        .map(|(id, text, date)| WeightedText {
            id: id.clone(),
            text,
            date: *date,
        })
        .collect();
    if yours.len() < 5 || theirs.is_empty() {
        return Ok(());
    }
    let (your_metrics, your_hash) = aggregate_metrics(&yours);
    let (their_metrics, their_hash) = aggregate_metrics(&theirs);
    let source_hash = format!("{your_hash}:{their_hash}");
    let existing = store.get_contact_style(account_id, email).await?;
    if let Some(existing) = existing.as_ref() {
        if existing.source_hash == source_hash {
            return Ok(());
        }
    }
    let drift_reason = if detect_drift && existing.is_some() {
        style_drift_reason(&yours_cleaned, &theirs_cleaned)
    } else {
        None
    };
    let drift_detected_at = drift_reason.as_ref().map(|_| chrono::Utc::now());
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
        drift_detected: drift_reason.is_some(),
        drift_reason: drift_reason.clone(),
        drift_detected_at,
    };
    store.upsert_contact_style(&record).await?;
    if let Some(reason) = drift_reason {
        store
            .insert_event(
                "warning",
                "relationship",
                "Relationship voice drift detected",
                Some(account_id),
                Some(&format!("email={} reason={}", email, reason)),
            )
            .await?;
    }
    Ok(())
}

fn style_drift_reason<Y, T>(
    yours: &[(Y, String, DateTime<Utc>)],
    theirs: &[(T, String, DateTime<Utc>)],
) -> Option<String> {
    let mut deltas = Vec::new();
    deltas.extend(direction_drift_reasons("your", yours));
    deltas.extend(direction_drift_reasons("their", theirs));
    (!deltas.is_empty()).then(|| deltas.join("; "))
}

fn direction_drift_reasons<I>(label: &str, samples: &[(I, String, DateTime<Utc>)]) -> Vec<String> {
    if samples.len() < 8 {
        return Vec::new();
    }
    let mut ordered = samples.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(_, _, date)| *date);
    let metrics = ordered
        .iter()
        .map(|(_, text, _)| crate::stylometry::compute_metrics(text))
        .collect::<Vec<_>>();
    let split = metrics.len().saturating_sub(3);
    let prior = &metrics[..split];
    let recent = &metrics[split..];
    let mut reasons = Vec::new();
    if let Some((from, to)) = metric_shift(
        prior
            .iter()
            .map(|metrics| metrics.formality_score)
            .collect(),
        recent
            .iter()
            .map(|metrics| metrics.formality_score)
            .collect(),
        0.15,
    ) {
        reasons.push(format!(
            "{label} formality shifted from {from:.2} to {to:.2}"
        ));
    }
    if let Some((from, to)) = metric_shift(
        prior
            .iter()
            .map(|metrics| metrics.avg_sentence_len)
            .collect(),
        recent
            .iter()
            .map(|metrics| metrics.avg_sentence_len)
            .collect(),
        4.0,
    ) {
        reasons.push(format!(
            "{label} sentence length shifted from {from:.1} to {to:.1}"
        ));
    }
    reasons
}

fn metric_shift(prior: Vec<f64>, recent: Vec<f64>, floor: f64) -> Option<(f64, f64)> {
    if prior.len() < 5 || recent.len() < 3 {
        return None;
    }
    let prior_mean = mean(&prior);
    let recent_mean = mean(&recent);
    let variance = prior
        .iter()
        .map(|value| (value - prior_mean).powi(2))
        .sum::<f64>()
        / prior.len() as f64;
    let threshold = (variance.sqrt() * 2.0).max(floor);
    let above = recent.iter().all(|value| *value > prior_mean + threshold);
    let below = recent.iter().all(|value| *value < prior_mean - threshold);
    (above || below).then_some((prior_mean, recent_mean))
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

pub async fn rebuild_user_voice_profile(store: &Store, account_id: &AccountId) -> Result<bool> {
    let samples = store.recent_user_voice_messages(account_id, 500).await?;
    if samples.len() < 20 {
        return Ok(false);
    }
    let reader_config = ReaderConfig::default();
    let cleaned = samples
        .iter()
        .map(|sample| {
            (
                sample.message_id.as_str(),
                clean(Some(&sample.body), None, &reader_config).content,
                sample.date,
            )
        })
        .collect::<Vec<_>>();
    let weighted = cleaned
        .iter()
        .map(|(id, text, date)| WeightedText {
            id: id.clone(),
            text,
            date: *date,
        })
        .collect::<Vec<_>>();
    let (metrics, source_hash) = aggregate_metrics(&weighted);
    if source_hash.is_empty() {
        return Ok(false);
    }
    if let Some(existing) = store.get_user_voice_profile(account_id).await? {
        if existing.source_hash == source_hash {
            return Ok(false);
        }
    }
    let per_message = cleaned
        .iter()
        .map(|(id, text, _)| (id.clone(), crate::stylometry::compute_metrics(text)))
        .collect::<Vec<_>>();
    let register_modes = crate::user_voice::build_register_modes(&per_message)
        .into_iter()
        .map(|mode| UserVoiceRegisterMode {
            name: mode.register.as_str().to_string(),
            formality_score: mode.metrics.formality_score,
            avg_sentence_len: mode.metrics.avg_sentence_len,
            exemplar_message_ids: mode.exemplar_message_ids,
        })
        .collect::<Vec<_>>();
    store
        .upsert_user_voice_profile(&UserVoiceProfileRecord {
            account_id: account_id.clone(),
            formality_score: metrics.formality_score,
            avg_sentence_len: metrics.avg_sentence_len,
            msg_count_used: samples.len() as u32,
            metrics_json: serde_json::to_string(&serde_json::json!(metrics))?,
            register_modes,
            computed_at: chrono::Utc::now(),
            source_hash,
        })
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::metric_shift;

    #[test]
    fn metric_shift_requires_three_recent_outliers() {
        let prior = vec![0.20, 0.21, 0.19, 0.20, 0.22, 0.18];

        assert!(
            metric_shift(prior.clone(), vec![0.20, 0.90, 0.91], 0.15).is_none(),
            "one normal recent sample should suppress drift"
        );
        assert_eq!(
            metric_shift(prior, vec![0.88, 0.90, 0.91], 0.15).map(|(_, to)| to > 0.85),
            Some(true),
            "three consecutive outliers should report drift"
        );
    }
}
