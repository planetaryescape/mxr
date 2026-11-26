//! Background model work runs only on mail that arrives live: a whole
//! initial backfill sends nothing to the model, and mail that arrives after
//! it still gets its delivery and relationship checks
//! (docs/issues/backfill-mail-triggers-background-inference.md).

use super::*;
use crate::test_fixtures::{make_empty_body, test_account_with_id, TestEnvelopeBuilder};
use mxr_core::types::{MessageFlags, SyncedMessage};
use mxr_core::{Label, SyncBatch, SyncCapabilities};
use mxr_llm::{
    BackfillHeldJob, CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
};
use std::sync::Mutex;

const ME: &str = "user@example.com";
const ALICE: &str = "alice@example.com";

/// Two backfill pages, then one page of mail that arrived live.
struct BackfillThenLive {
    account_id: AccountId,
}

impl BackfillThenLive {
    fn order(&self, key: &str) -> SyncedMessage {
        self.synced(
            TestEnvelopeBuilder::new()
                .provider_id(format!("order-{key}"))
                .subject(format!(
                    "Order #AB-{key} confirmation — thanks for your order"
                ))
                .sender_address("Acme Store", "orders@mail.acme.com")
                .recipient_address(Some("User"), ME),
            "We received your order.",
        )
    }

    fn with_alice(&self, key: &str, outbound: bool) -> SyncedMessage {
        let envelope = if outbound {
            TestEnvelopeBuilder::new()
                .sender_address("User", ME)
                .recipient_address(Some("Alice"), ALICE)
                .flags(MessageFlags::READ | MessageFlags::SENT)
        } else {
            TestEnvelopeBuilder::new()
                .sender_address("Alice", ALICE)
                .recipient_address(Some("User"), ME)
        };
        self.synced(
            envelope
                .provider_id(format!("alice-{key}"))
                .subject(format!("Planning {key}")),
            &format!("Thanks for the notes on {key}. I will send the next draft on Friday."),
        )
    }

    fn synced(&self, envelope: TestEnvelopeBuilder, text: &str) -> SyncedMessage {
        let envelope = envelope.account_id(self.account_id.clone()).build();
        let mut body = make_empty_body(&envelope.id);
        body.text_plain = Some(text.to_string());
        SyncedMessage {
            envelope,
            body,
            arrival: Default::default(),
        }
    }
}

#[async_trait::async_trait]
impl MailSyncProvider for BackfillThenLive {
    fn name(&self) -> &str {
        "backfill-then-live"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities::default()
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(Vec::new())
    }

    fn is_backfill_cursor(&self, cursor: &SyncCursor) -> bool {
        cursor.as_bytes().starts_with(b"page:")
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        let (upserted, next): (Vec<SyncedMessage>, &[u8]) = match cursor.as_bytes() {
            b"page:1" => (
                (0..3)
                    .map(|i| self.with_alice(&format!("old-out-{i}"), true))
                    .chain([self.with_alice("old-in", false), self.order("10001")])
                    .collect(),
                b"live:0",
            ),
            b"live:0" => (
                vec![self.with_alice("new-in", false), self.order("20002")],
                b"live:1",
            ),
            b"live:1" => (Vec::new(), b"live:1"),
            _ => (
                (0..3)
                    .map(|i| self.with_alice(&format!("older-out-{i}"), true))
                    .chain([self.order("10000")])
                    .collect(),
                b"page:1",
            ),
        };
        Ok(SyncBatch {
            upserted,
            deleted_provider_ids: Vec::new(),
            label_changes: Vec::new(),
            next_cursor: SyncCursor::from_bytes(next.to_vec()),
            has_more: next == b"page:1",
            threads_changed: Vec::new(),
            remaining_estimate: None,
        })
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, MxrError> {
        Err(MxrError::NotFound("no attachment".into()))
    }

    async fn apply_mutation(
        &self,
        _mutation_id: &str,
        _mutation: &mxr_core::Mutation,
    ) -> Result<(), MxrError> {
        Ok(())
    }
}

/// A model endpoint that records which job each call came from.
#[derive(Default)]
struct CountingModel {
    calls: Mutex<Vec<&'static str>>,
}

impl CountingModel {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LlmProvider for CountingModel {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let prompt: String = req.messages.iter().map(|m| m.content.as_str()).collect();
        let (job, content) = if prompt.contains("relationship profile") {
            (
                "relationship",
                r#"{"text":"tends to send drafts on Friday","known_topics":["planning"]}"#,
            )
        } else if prompt.contains("Order #AB-") {
            (
                "delivery",
                r#"{"is_shipment_related": true, "status": "ordered", "merchant": "Acme Store", "confidence": 0.9}"#,
            )
        } else {
            ("other", "{}")
        };
        self.calls.lock().unwrap().push(job);
        Ok(CompletionResponse {
            content: content.into(),
            model: "counting".into(),
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
        "counting"
    }
}

async fn sync_once(state: &Arc<AppState>, provider: Arc<dyn MailSyncProvider>) {
    let pass = begin_sync_pass(state, provider, None, SyncStarter::AccountLoop).await;
    let SyncWait::Finished { pass, result } = run_sync_pass(pass, Duration::from_secs(10)).await
    else {
        panic!("a fast sync must not detach");
    };
    finalize_sync_pass(pass, result).await.unwrap();
}

/// Polls until `done` holds, so the test waits for detached post-sync work
/// and the relationship worker rather than for a fixed time.
async fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..400 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn held(state: &AppState, job: BackfillHeldJob) -> u64 {
    state
        .llm
        .backfill_holds()
        .into_iter()
        .find_map(|(held, inputs)| (held == job).then_some(inputs))
        .unwrap_or(0)
}

#[tokio::test]
async fn a_full_backfill_makes_no_model_calls_and_live_mail_still_gets_its_checks() {
    let account_id = AccountId::new();
    let provider = Arc::new(BackfillThenLive {
        account_id: account_id.clone(),
    });
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(
            test_account_with_id(account_id.clone()),
            provider.clone(),
            None,
        )
        .await
        .unwrap(),
    );
    let mut config = state.config_snapshot();
    config.llm.enabled = true;
    config.deliveries.enabled = true;
    state.set_config_for_test(config).await;
    let model = Arc::new(CountingModel::default());
    state.llm.replace(model.clone());
    let provider = provider as Arc<dyn MailSyncProvider>;

    sync_once(&state, provider.clone()).await;
    sync_once(&state, provider.clone()).await;
    eventually("the backfill's delivery and relationship jobs", || {
        held(&state, BackfillHeldJob::DeliveryExtraction) == 2
            && held(&state, BackfillHeldJob::RelationshipSummary) == 1
    })
    .await;
    assert!(
        state
            .store
            .get_contact_style(&account_id, ALICE)
            .await
            .unwrap()
            .is_some(),
        "the heuristic style profile still builds from backfilled mail"
    );
    assert_eq!(
        model.calls(),
        Vec::<&str>::new(),
        "backfill reached the model"
    );

    sync_once(&state, provider).await;
    eventually("the live mail's delivery and relationship calls", || {
        let calls = model.calls();
        calls.contains(&"delivery") && calls.contains(&"relationship")
    })
    .await;
    let calls = model.calls();
    assert_eq!(
        calls.iter().filter(|job| **job == "delivery").count(),
        1,
        "only the live order goes to the model: {calls:?}"
    );
    assert!(state
        .store
        .get_contact_relationship_summary(&account_id, ALICE)
        .await
        .unwrap()
        .is_some());
}
