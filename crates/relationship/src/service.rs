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
use tokio::sync::{mpsc, oneshot, Semaphore};
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
    EnqueueContacts {
        contacts: Vec<(AccountId, String)>,
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
    /// `background_db` gates this worker's DB-touching work below the
    /// reader-pool size so background relationship analytics can never
    /// starve interactive/status queries of connections. A permit is
    /// held across each per-contact unit.
    pub fn start(
        store: Arc<Store>,
        llm: Arc<LlmRuntime>,
        background_db: Arc<Semaphore>,
    ) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel(32);
        let handle = tokio::spawn(async move {
            let mut pending = VecDeque::<(AccountId, String)>::new();
            let mut pending_keys = HashSet::<(String, String)>::new();
            loop {
                while let Some((account_id, email)) = pending.pop_front() {
                    pending_keys.remove(&(account_id.as_str(), email.clone()));
                    sleep(Duration::from_millis(250)).await;
                    // Reserve background-DB headroom for the whole
                    // per-contact unit (style + summary + commitments).
                    let _bg_permit = background_db.acquire().await;
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
                                enqueue_contacts(&mut pending, &mut pending_keys, contacts);
                                let _ = resp.send(Ok(()));
                            }
                            Err(error) => {
                                let _ = resp.send(Err(error.into()));
                            }
                        }
                    }
                    RelationshipCommand::EnqueueContacts { contacts, resp } => {
                        enqueue_contacts(&mut pending, &mut pending_keys, contacts);
                        let _ = resp.send(Ok(()));
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

    pub async fn enqueue_contacts(&self, contacts: Vec<(AccountId, String)>) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(RelationshipCommand::EnqueueContacts {
                contacts,
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

fn enqueue_contacts(
    pending: &mut VecDeque<(AccountId, String)>,
    pending_keys: &mut HashSet<(String, String)>,
    contacts: Vec<(AccountId, String)>,
) {
    for (account_id, email) in contacts {
        let key = (account_id.as_str(), email.clone());
        if pending_keys.insert(key) {
            pending.push_back((account_id, email));
        }
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
                "warn",
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
    use super::*;
    use chrono::TimeZone;
    use mxr_core::id::{MessageId, ThreadId};
    use mxr_core::types::{
        Account, Address, BackendRef, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
        ProviderKind, UnsubscribeMethod,
    };
    use mxr_store::Store;

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

    #[tokio::test]
    async fn rebuild_contact_style_uses_clean_non_list_messages_and_skips_unchanged_rebuild() {
        let store = Store::in_memory().await.expect("store");
        let account = test_account();
        store.insert_account(&account).await.expect("account");
        let thread_id = ThreadId::new();
        let base = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        for index in 0..5 {
            insert_relationship_message(
                &store,
                &account,
                &thread_id,
                RelationshipMessageFixture {
                    direction: MessageDirection::Outbound,
                    contact_email: "alice@example.com",
                    body: "Hi Alice. I will send it today.\n> This quoted inbound paragraph has many extra words that should not shape my outbound voice metrics.",
                    date: base + chrono::Duration::minutes(index),
                    list_id: None,
                },
            )
            .await;
        }
        insert_relationship_message(
            &store,
            &account,
            &thread_id,
            RelationshipMessageFixture {
                direction: MessageDirection::Outbound,
                contact_email: "alice@example.com",
                body: "Newsletter list content should not count as direct relationship voice.",
                date: base + chrono::Duration::minutes(6),
                list_id: Some("list.example"),
            },
        )
        .await;
        insert_relationship_message(
            &store,
            &account,
            &thread_id,
            RelationshipMessageFixture {
                direction: MessageDirection::Inbound,
                contact_email: "alice@example.com",
                body: "Can you send it today?",
                date: base + chrono::Duration::minutes(7),
                list_id: None,
            },
        )
        .await;

        rebuild_contact_style(&store, &account.id, "alice@example.com")
            .await
            .expect("first rebuild");
        let first = store
            .get_contact_style(&account.id, "alice@example.com")
            .await
            .expect("style query")
            .expect("style");
        tokio::time::sleep(Duration::from_secs(1)).await;
        rebuild_contact_style(&store, &account.id, "alice@example.com")
            .await
            .expect("second rebuild");
        let second = store
            .get_contact_style(&account.id, "alice@example.com")
            .await
            .expect("style query")
            .expect("style");

        assert_eq!(first.msg_count_used, 5);
        assert!(
            first.avg_sentence_len < 8.0,
            "quoted text should be stripped before stylometry; got avg sentence length {}",
            first.avg_sentence_len
        );
        assert_eq!(
            second.computed_at, first.computed_at,
            "unchanged source hash should skip rewriting the style record"
        );
    }

    #[tokio::test]
    async fn rebuild_contact_style_flags_three_recent_voice_outliers_as_drift() {
        let store = Store::in_memory().await.expect("store");
        let account = test_account();
        store.insert_account(&account).await.expect("account");
        let thread_id = ThreadId::new();
        let base = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        for index in 0..8 {
            insert_relationship_message(
                &store,
                &account,
                &thread_id,
                RelationshipMessageFixture {
                    direction: MessageDirection::Outbound,
                    contact_email: "alice@example.com",
                    body: "ok.",
                    date: base + chrono::Duration::minutes(index),
                    list_id: None,
                },
            )
            .await;
        }
        insert_relationship_message(
            &store,
            &account,
            &thread_id,
            RelationshipMessageFixture {
                direction: MessageDirection::Inbound,
                contact_email: "alice@example.com",
                body: "Thanks.",
                date: base + chrono::Duration::minutes(9),
                list_id: None,
            },
        )
        .await;
        rebuild_contact_style(&store, &account.id, "alice@example.com")
            .await
            .expect("baseline rebuild");

        for index in 0..3 {
            insert_relationship_message(
                &store,
                &account,
                &thread_id,
                RelationshipMessageFixture {
                    direction: MessageDirection::Outbound,
                    contact_email: "alice@example.com",
                    body: "I can prepare the deployment update, summarize the dashboard results, confirm the owner, and send the final note before the deadline.",
                    date: base + chrono::Duration::minutes(20 + index),
                    list_id: None,
                },
            )
            .await;
        }
        rebuild_contact_style(&store, &account.id, "alice@example.com")
            .await
            .expect("drift rebuild");

        let style = store
            .get_contact_style(&account.id, "alice@example.com")
            .await
            .expect("style query")
            .expect("style");
        assert!(
            style.drift_detected,
            "three recent outliers should flag drift"
        );
        assert!(
            style
                .drift_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("sentence length")),
            "drift reason should explain the observable voice shift: {:?}",
            style.drift_reason
        );
    }

    fn test_account() -> Account {
        Account {
            id: AccountId::new(),
            name: "Test".to_string(),
            email: "me@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    struct RelationshipMessageFixture<'a> {
        direction: MessageDirection,
        contact_email: &'a str,
        body: &'a str,
        date: chrono::DateTime<Utc>,
        list_id: Option<&'a str>,
    }

    async fn insert_relationship_message(
        store: &Store,
        account: &Account,
        thread_id: &ThreadId,
        fixture: RelationshipMessageFixture<'_>,
    ) -> MessageId {
        let RelationshipMessageFixture {
            direction,
            contact_email,
            body,
            date,
            list_id,
        } = fixture;
        let message_id = MessageId::new();
        let (from, to) = match direction {
            MessageDirection::Outbound => (
                Address {
                    name: Some("Me".to_string()),
                    email: account.email.clone(),
                },
                Address {
                    name: Some("Alice".to_string()),
                    email: contact_email.to_string(),
                },
            ),
            _ => (
                Address {
                    name: Some("Alice".to_string()),
                    email: contact_email.to_string(),
                },
                Address {
                    name: Some("Me".to_string()),
                    email: account.email.clone(),
                },
            ),
        };
        let envelope = mxr_core::types::Envelope {
            id: message_id.clone(),
            account_id: account.id.clone(),
            provider_id: format!("provider-{}", message_id.as_str()),
            thread_id: thread_id.clone(),
            message_id_header: None,
            in_reply_to: None,
            references: Vec::new(),
            from,
            to: vec![to],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Relationship".to_string(),
            date,
            flags: MessageFlags::READ,
            snippet: body.to_string(),
            has_attachments: false,
            size_bytes: body.len() as u64,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
            keywords: std::collections::BTreeSet::new(),
        };
        store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .expect("envelope");
        store
            .insert_body(&MessageBody {
                message_id: message_id.clone(),
                text_plain: Some(body.to_string()),
                text_html: None,
                attachments: Vec::new(),
                fetched_at: date,
                metadata: MessageMetadata {
                    list_id: list_id.map(str::to_string),
                    ..Default::default()
                },
            })
            .await
            .expect("body");
        message_id
    }
}
