//! Keeps background LLM work whole across endpoint outages and restarts.
//!
//! The background breaker and attempt ledger are saved to a small file in the
//! data directory, so a restart neither forgets an outage nor re-spends inputs
//! that already succeeded or are backing off. Inputs an outage put off (see
//! `FeatureLlmRuntime::record_background_failure`) are re-run once each when
//! the endpoint answers again, through the same relationship worker and
//! delivery scan that first ran them. Nothing else would: the relationship
//! worker only sees a contact again when new mail arrives, and the delivery
//! scan only runs on newly synced mail.

use crate::state::AppState;
use mxr_core::id::MessageId;
use mxr_llm::{BackgroundState, LlmFeature};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Name of the saved state in the data directory.
pub(crate) const STATE_FILE: &str = "llm-background.json";

/// How often changed state is saved and put-off work is looked at. Also
/// bounds how long a closed breaker's put-off work waits without a signal.
const TICK: Duration = Duration::from_secs(30);

/// The features whose put-off inputs are re-run here. Commitment extraction
/// records into the ledger too but has no background caller to re-run it.
const RECOVERABLE: [LlmFeature; 2] = [
    LlmFeature::RelationshipSummary,
    LlmFeature::DeliveryExtraction,
];

/// The state an earlier daemon saved, or `None` to start empty. An unreadable
/// or corrupt file costs at most one more try per input, so it is reported
/// and otherwise ignored; the next save replaces it.
pub(crate) fn load(path: &Path) -> Option<BackgroundState> {
    let text = match mxr_config::read_private_file(path) {
        Ok(Some(text)) => text,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "could not read saved LLM background state; starting empty");
            return None;
        }
    };
    match serde_json::from_str::<BackgroundState>(&text) {
        Ok(state) if state.is_supported() => Some(state),
        Ok(_) => {
            tracing::warn!(path = %path.display(), "saved LLM background state is from another mxr version; starting empty");
            None
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "saved LLM background state is corrupt; starting empty");
            None
        }
    }
}

/// Write the breaker and ledger if they changed since the last save. A state
/// without a path (tests, the demo) is never written.
pub(crate) async fn save_if_changed(state: &AppState) {
    let Some(path) = state.llm_state_path.clone() else {
        return;
    };
    let Some(snapshot) = state.llm.take_unsaved_background_state() else {
        return;
    };
    let written = tokio::task::spawn_blocking(move || write(&path, &snapshot)).await;
    let error = match written {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error.to_string(),
        Err(error) => error.to_string(),
    };
    tracing::warn!(%error, "could not save LLM background state; will retry");
    state.llm.background_state_unsaved();
}

fn write(path: &Path, snapshot: &BackgroundState) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(snapshot).map_err(std::io::Error::other)?;
    mxr_config::write_private_file(path, &bytes)
}

/// Starts the loop that saves the state and re-runs put-off work.
pub(crate) fn spawn(state: Arc<AppState>) -> JoinHandle<()> {
    // Subscribe before spawning so a shutdown sent first is still seen.
    let shutdown_rx = state.shutdown_receiver();
    tokio::spawn(background_loop(state, shutdown_rx))
}

async fn background_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut recovered = state.llm.subscribe_background_recovery();
    let mut recovery_open = true;
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut rerun: Option<JoinHandle<()>> = None;
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow_and_update() {
                    break;
                }
                continue;
            }
            _ = ticker.tick() => save_if_changed(&state).await,
            changed = recovered.changed(), if recovery_open => {
                recovery_open = changed.is_ok();
            }
        }
        // One pass at a time: a pass still running holds this pass's inputs.
        if rerun.as_ref().is_some_and(|pass| !pass.is_finished()) {
            continue;
        }
        let due = state.llm.background_recovery_due(&RECOVERABLE).await;
        if !due.is_empty() {
            let pass_state = state.clone();
            rerun = Some(tokio::spawn(async move {
                rerun_deferred(&pass_state, &due).await;
            }));
        }
    }
}

/// Run each input put off for `features` once, through the path that first
/// ran it. An input the endpoint fails again is put off again by that path.
pub(crate) async fn rerun_deferred(state: &AppState, features: &[LlmFeature]) {
    let take = |feature| {
        if features.contains(&feature) {
            state.llm.take_deferred_background(feature)
        } else {
            Vec::new()
        }
    };
    let contacts: Vec<_> = take(LlmFeature::RelationshipSummary)
        .iter()
        .filter_map(|key| mxr_relationship::summary::parse_attempt_key(key))
        .collect();
    if !contacts.is_empty() {
        tracing::info!(
            contacts = contacts.len(),
            "re-running relationship summaries put off while the LLM endpoint was out"
        );
        if let Err(error) = state.relationship.enqueue_contacts(contacts).await {
            tracing::warn!(%error, "could not re-queue relationship summaries");
        }
    }

    let messages: Vec<MessageId> = take(LlmFeature::DeliveryExtraction)
        .iter()
        .filter_map(|key| MessageId::from_str(key).ok())
        .collect();
    if !messages.is_empty() {
        tracing::info!(
            messages = messages.len(),
            "re-scanning deliveries put off while the LLM endpoint was out"
        );
        crate::handler::deliveries::scan_messages(state, &messages).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_llm::{
        CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider, LlmRuntime,
        NoopProvider,
    };
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    #[tokio::test]
    async fn a_saved_state_reads_back_privately_and_a_corrupt_one_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        assert_eq!(load(&path), None, "no file yet");

        let runtime = Arc::new(LlmRuntime::new(Arc::new(NoopProvider)));
        runtime
            .for_feature(LlmFeature::DeliveryExtraction)
            .record_background_failure("message", "", &LlmError::Timeout(Duration::from_secs(45)));
        let saved = runtime.take_unsaved_background_state().unwrap();
        write(&path, &saved).unwrap();
        assert_eq!(load(&path), Some(saved));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        std::fs::write(&path, b"{\"version\":1,\"breaker\":").unwrap();
        assert_eq!(load(&path), None, "a cut-off file starts empty");
    }

    /// Confirms an order when the endpoint is up.
    #[derive(Default)]
    struct Shop {
        up: AtomicBool,
        calls: AtomicU32,
    }

    #[async_trait::async_trait]
    impl LlmProvider for Shop {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if !self.up.load(Ordering::SeqCst) {
                return Err(LlmError::Unreachable("connection refused".into()));
            }
            Ok(CompletionResponse {
                content: r#"{"is_shipment_related": true, "status": "ordered", "merchant": "Acme Store", "order_number": "AB-12345", "confidence": 0.9}"#.into(),
                model: "shop".into(),
                finish_reason: Some("stop".into()),
            })
        }

        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 8192,
                supports_streaming: false,
            }
        }

        fn model_name(&self) -> &str {
            "shop"
        }
    }

    /// The issue's open question: a delivery that fell back to the heuristic
    /// during an outage is scanned again, once, when the endpoint is back.
    #[tokio::test]
    async fn a_delivery_put_off_by_an_outage_is_enriched_once_the_endpoint_is_back() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.deliveries.enabled = true;
        state.set_config_for_test(config).await;
        let shop = Arc::new(Shop::default());
        state.llm.replace(shop.clone());

        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(state.default_account_id_opt().unwrap())
            .provider_id("acme-order")
            .subject("Order #AB-12345 confirmation — thanks for your order")
            .sender_address("Acme Store", "orders@mail.acme.com")
            .recipient_address(Some("User"), "user@example.com")
            .build();
        let id = envelope.id.clone();
        state.store.upsert_envelope(&envelope).await.unwrap();
        state
            .store
            .insert_body(&mxr_core::types::MessageBody {
                message_id: id.clone(),
                text_plain: Some("We received your order.".into()),
                text_html: None,
                attachments: Vec::new(),
                fetched_at: chrono::Utc::now(),
                metadata: mxr_core::types::MessageMetadata::default(),
            })
            .await
            .unwrap();

        let scanned =
            crate::handler::deliveries::scan_messages(&state, std::slice::from_ref(&id)).await;
        assert_eq!(scanned.shortlisted + scanned.created, 0, "{scanned:?}");
        assert_eq!(shop.calls.load(Ordering::SeqCst), 1, "the model was asked");
        let all = mxr_store::DeliveryListFilter::All;
        assert!(state.store.list_deliveries(all).await.unwrap().is_empty());

        shop.up.store(true, Ordering::SeqCst);
        rerun_deferred(&state, &RECOVERABLE).await;
        assert_eq!(shop.calls.load(Ordering::SeqCst), 2);
        assert_eq!(state.store.list_deliveries(all).await.unwrap().len(), 1);

        rerun_deferred(&state, &RECOVERABLE).await;
        assert_eq!(shop.calls.load(Ordering::SeqCst), 2, "run once, not again");
    }
}
