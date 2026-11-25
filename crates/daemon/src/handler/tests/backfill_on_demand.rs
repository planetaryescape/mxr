//! Backfilled mail reaches the model only through commands the user runs,
//! and each makes exactly the calls its dry run predicted.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::test_fixtures::TestEnvelopeBuilder;
use mxr_core::id::MessageId;
use mxr_core::types::{MessageBody, MessageDirection, MessageFlags, MessageMetadata};
use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use mxr_protocol::{DeliveryScanSummary, RelationshipRebuildSummary};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default)]
struct CountingModel {
    calls: AtomicU32,
}

#[async_trait]
impl LlmProvider for CountingModel {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let prompt: String = req.messages.iter().map(|m| m.content.as_str()).collect();
        let content = if prompt.contains("relationship profile") {
            r#"{"text":"tends to reply within a day","known_topics":[]}"#
        } else {
            r#"{"is_shipment_related": true, "status": "ordered", "merchant": "Acme Store", "confidence": 0.9}"#
        };
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

/// A fixture whose mail all came from the backfill: none arrived live.
async fn backfilled() -> (Fixture, Arc<CountingModel>) {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.llm.enabled = true;
    config.deliveries.enabled = true;
    fx.state.set_config_for_test(config).await;
    let model = Arc::new(CountingModel::default());
    fx.state.llm.replace(model.clone());
    (fx, model)
}

async fn mail(fx: &Fixture, from: (&str, &str), to: &str, subject: &str, text: &str, days: i64) {
    let outbound = from.1 == ME;
    let envelope = TestEnvelopeBuilder::new()
        .account_id(fx.account.clone())
        .provider_id(format!("backfill-{}", MessageId::new()))
        .sender_address(from.0, from.1)
        .recipient_address(None, to)
        .subject(subject)
        .flags(if outbound {
            MessageFlags::READ | MessageFlags::SENT
        } else {
            MessageFlags::empty()
        })
        .date(chrono::Utc::now() - chrono::Duration::days(days))
        .build();
    let direction = if outbound {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(text.into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: envelope.date,
            metadata: MessageMetadata::default(),
        })
        .await
        .unwrap();
}

/// Enough history with `email` for a style profile, so a summary is due.
async fn correspondent(fx: &Fixture, email: &str, days: i64) {
    for i in 0..5 {
        let text = format!("Here is part {i} of the plan. Let me know what you think.");
        mail(fx, ("User", ME), email, "Plan", &text, days).await;
    }
    let reply = "Thanks, this looks right. I will send comments by Friday.";
    mail(fx, ("Them", email), ME, "Re: Plan", reply, days).await;
}

async fn rebuild(fx: &Fixture, dry_run: bool) -> RelationshipRebuildSummary {
    match request(
        fx,
        Request::RebuildRelationshipProfiles {
            account_id: None,
            since_days: 30,
            dry_run,
        },
    )
    .await
    {
        ResponseData::RelationshipRebuild { summary } => summary,
        other => panic!("expected a rebuild summary, got {other:?}"),
    }
}

#[tokio::test]
async fn relationship_rebuild_makes_exactly_the_calls_its_dry_run_predicted() {
    let (fx, model) = backfilled().await;
    correspondent(&fx, "bob@example.com", 3).await;
    correspondent(&fx, "carol@example.com", 10).await;
    mail(
        &fx,
        ("Dave", "dave@example.com"),
        ME,
        "Hi",
        "Quick hello.",
        5,
    )
    .await;
    correspondent(&fx, "erin@example.com", 90).await;

    let preview = rebuild(&fx, true).await;
    assert_eq!(
        (preview.contacts, preview.model_calls, preview.dry_run),
        (3, 2, true),
        "bob and carol are due; dave has too little mail; erin is outside the window"
    );
    assert_eq!(
        model.calls.load(Ordering::SeqCst),
        0,
        "a dry run calls nothing"
    );
    assert!(fx
        .state
        .store
        .get_contact_style(&fx.account, "bob@example.com")
        .await
        .unwrap()
        .is_none());

    let run = rebuild(&fx, false).await;
    assert_eq!((run.contacts, run.model_calls, run.errors), (3, 2, 0));
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);

    assert_eq!(
        rebuild(&fx, true).await.model_calls,
        0,
        "summaries are current"
    );
    rebuild(&fx, false).await;
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn delivery_scan_makes_exactly_the_calls_its_dry_run_predicted() {
    let (fx, model) = backfilled().await;
    for (number, days) in [("10001", 2), ("10002", 20)] {
        let subject = format!("Order #AB-{number} confirmation — thanks for your order");
        let from = ("Acme Store", "orders@mail.acme.com");
        mail(&fx, from, ME, &subject, "We received your order.", days).await;
    }

    let scan = |dry_run| Request::ScanDeliveries {
        account_id: None,
        since_days: Some(30),
        dry_run,
    };
    let summary = |data| match data {
        ResponseData::DeliveryScan { summary } => summary,
        other => panic!("expected a scan summary, got {other:?}"),
    };
    let preview: DeliveryScanSummary = summary(request(&fx, scan(true)).await);
    assert_eq!(preview.model_calls, 2);
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);

    let run: DeliveryScanSummary = summary(request(&fx, scan(false)).await);
    assert_eq!(run.model_calls, 2);
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
}
