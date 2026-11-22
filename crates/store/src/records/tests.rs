use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::Store;
use chrono::{Duration, TimeZone};
use mxr_core::types::{Address, MessageDirection};

fn at(days: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 3, 3, 12, 0, 0)
        .single()
        .expect("valid time")
        + Duration::days(days)
}

struct Fx {
    store: Store,
    account: AccountId,
}

impl Fx {
    async fn new() -> Self {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        Self {
            store,
            account: account.id,
        }
    }

    async fn message(&self, provider_id: &str, date: DateTime<Utc>) -> (MessageId, ThreadId) {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(self.account.clone())
            .build();
        envelope.provider_id = provider_id.to_string();
        envelope.thread_id = ThreadId::new();
        envelope.date = date;
        envelope.subject = format!("Your Dell order {provider_id}");
        envelope.from = Address {
            name: Some("Dell".to_string()),
            email: "orders@dell.com".to_string(),
        };
        self.store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        (envelope.id, envelope.thread_id)
    }

    fn value(
        field: &str,
        source: &(MessageId, ThreadId),
        kind: &str,
        text: Option<&str>,
        int: Option<i64>,
        observed: DateTime<Utc>,
    ) -> RecordFieldValue {
        let (rank, checked) = match kind {
            "schema" => (3, true),
            "user" => (4, true),
            _ => (1, false),
        };
        RecordFieldValue {
            field: field.to_string(),
            source_key: source.0.as_str(),
            message_id: Some(source.0.clone()),
            source: kind.to_string(),
            rank,
            value_text: text.map(str::to_string),
            value_int: int,
            checked,
            evidence: text.map(str::to_string),
            observed_at: observed,
        }
    }

    fn filing(
        &self,
        id: &str,
        source: &(MessageId, ThreadId),
        stage: &str,
        message_at: DateTime<Utc>,
        fields: Vec<RecordFieldValue>,
    ) -> RecordFiling {
        RecordFiling {
            id: id.to_string(),
            account_id: self.account.clone(),
            dedup_key: "order|dell|402-118".to_string(),
            kind: "order".to_string(),
            origin: "rule".to_string(),
            reason: "order confirmation".to_string(),
            rules_version: 1,
            links: vec![RecordLink {
                message_id: source.0.clone(),
                thread_id: Some(source.1.clone()),
                stage: stage.to_string(),
                message_at,
                filed_by: "detector".to_string(),
            }],
            fields,
            now: message_at,
        }
    }
}

#[tokio::test]
async fn three_emails_about_one_order_are_one_record_and_schema_beats_rule() {
    let fx = Fx::new().await;
    let confirmation = fx.message("c", at(0)).await;
    let shipped = fx.message("s", at(2)).await;
    let delivered = fx.message("d", at(4)).await;

    let first = fx
        .store
        .file_record(&fx.filing(
            "r1",
            &confirmation,
            "confirmation",
            at(0),
            vec![
                Fx::value("issuer", &confirmation, "schema", Some("Dell"), None, at(0)),
                Fx::value(
                    "amount",
                    &confirmation,
                    "schema",
                    Some("GBP"),
                    Some(124_900),
                    at(0),
                ),
                Fx::value(
                    "issued_at",
                    &confirmation,
                    "schema",
                    None,
                    Some(at(0).timestamp()),
                    at(0),
                ),
                Fx::value(
                    "reference",
                    &confirmation,
                    "schema",
                    Some("402-118"),
                    None,
                    at(0),
                ),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(first, RecordFiled::Inserted { id: "r1".into() });

    let second = fx
        .store
        .file_record(&fx.filing(
            "r-ignored",
            &shipped,
            "shipped",
            at(2),
            vec![Fx::value(
                "amount",
                &shipped,
                "rule",
                Some("GBP"),
                Some(999),
                at(2),
            )],
        ))
        .await
        .unwrap();
    assert_eq!(second, RecordFiled::Updated { id: "r1".into() });
    fx.store
        .file_record(&fx.filing(
            "r-ignored",
            &delivered,
            "delivered",
            at(4),
            vec![Fx::value(
                "delivered_at",
                &delivered,
                "rule",
                None,
                Some(at(4).timestamp()),
                at(4),
            )],
        ))
        .await
        .unwrap();

    let record = fx.store.get_archive_record("r1").await.unwrap().unwrap();
    assert_eq!(record.amount_minor, Some(124_900));
    assert_eq!(record.currency.as_deref(), Some("GBP"));
    assert_eq!(record.issuer_key.as_deref(), Some("dell"));
    assert_eq!(record.delivered_at, Some(at(4)));
    assert!(!record.checked);
    assert_eq!(record.last_message_at, Some(at(4)));
    let sources = fx
        .store
        .archive_record_sources(&["r1".to_string()])
        .await
        .unwrap();
    assert_eq!(sources.len(), 3);
    let all = fx
        .store
        .list_archive_records(&RecordQuery::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn the_user_wins_forever_and_confirming_checks_the_record() {
    let fx = Fx::new().await;
    let source = fx.message("c", at(0)).await;
    fx.store
        .file_record(&fx.filing(
            "r1",
            &source,
            "receipt",
            at(0),
            vec![Fx::value(
                "amount",
                &source,
                "rule",
                Some("GBP"),
                Some(500),
                at(0),
            )],
        ))
        .await
        .unwrap();
    assert!(
        !fx.store
            .get_archive_record("r1")
            .await
            .unwrap()
            .unwrap()
            .checked
    );

    let confirm = RecordFieldEdit::Confirm {
        field: "amount".into(),
    };
    let (preview, _) = fx
        .store
        .edit_archive_record("r1", &confirm, at(1), false)
        .await
        .unwrap()
        .unwrap();
    assert!(preview.checked);
    assert!(
        !fx.store
            .get_archive_record("r1")
            .await
            .unwrap()
            .unwrap()
            .checked
    );
    fx.store
        .edit_archive_record("r1", &confirm, at(1), true)
        .await
        .unwrap()
        .unwrap();
    let record = fx.store.get_archive_record("r1").await.unwrap().unwrap();
    assert!(record.checked);
    assert_eq!(record.amount_minor, Some(500));

    fx.store
        .edit_archive_record(
            "r1",
            &RecordFieldEdit::Set {
                field: "amount".into(),
                value_text: Some("GBP".into()),
                value_int: Some(700),
            },
            at(2),
            true,
        )
        .await
        .unwrap();
    fx.store
        .file_record(&fx.filing(
            "r1",
            &source,
            "receipt",
            at(9),
            vec![Fx::value(
                "amount",
                &source,
                "rule",
                Some("GBP"),
                Some(900),
                at(9),
            )],
        ))
        .await
        .unwrap();
    let record = fx.store.get_archive_record("r1").await.unwrap().unwrap();
    assert_eq!(record.amount_minor, Some(700));
    let fields = fx
        .store
        .archive_record_fields(&["r1".to_string()])
        .await
        .unwrap();
    let user = fields
        .iter()
        .find(|(_, field)| field.source == "user")
        .expect("user field");
    assert_eq!(user.1.evidence.as_deref(), Some("you"));
}

#[tokio::test]
async fn deleting_a_source_recomputes_fields_and_the_last_source_deletes_the_record() {
    let fx = Fx::new().await;
    let confirmation = fx.message("c", at(0)).await;
    let shipped = fx.message("s", at(2)).await;
    fx.store
        .file_record(&fx.filing(
            "r1",
            &confirmation,
            "confirmation",
            at(0),
            vec![Fx::value(
                "amount",
                &confirmation,
                "schema",
                Some("GBP"),
                Some(124_900),
                at(0),
            )],
        ))
        .await
        .unwrap();
    fx.store
        .file_record(&fx.filing(
            "r1",
            &shipped,
            "shipped",
            at(2),
            vec![Fx::value(
                "amount",
                &shipped,
                "rule",
                Some("GBP"),
                Some(999),
                at(2),
            )],
        ))
        .await
        .unwrap();
    fx.store
        .replace_record_groups(
            &fx.account,
            &[(
                RecordGroup {
                    id: "g1".into(),
                    account_id: fx.account.clone(),
                    kind: "series".into(),
                    group_key: "series|dell|".into(),
                    title: "Dell".into(),
                    span_start: None,
                    span_end: None,
                },
                vec!["r1".to_string()],
            )],
            at(3),
        )
        .await
        .unwrap();

    fx.store
        .delete_messages_and_derived(&fx.account, &["c".to_string()])
        .await
        .unwrap();
    let record = fx.store.get_archive_record("r1").await.unwrap().unwrap();
    assert_eq!(
        record.amount_minor,
        Some(999),
        "recomputed from what is left"
    );
    assert!(!record.checked);
    assert_eq!(record.last_message_at, Some(at(2)));

    fx.store
        .delete_messages_and_derived(&fx.account, &["s".to_string()])
        .await
        .unwrap();
    assert!(fx.store.get_archive_record("r1").await.unwrap().is_none());
    assert!(fx.store.list_record_groups(None).await.unwrap().is_empty());
}
