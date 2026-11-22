use super::classify_eval_floor;
use super::*;
use crate::handler::classify_eval::classify_eval;
use chrono::Utc;
use mxr_core::id::AccountId;
use mxr_core::types::{Account, BackendRef, ProviderKind};
use mxr_protocol::{
    ClassifyEvalMismatchData, ClassifyEvalSetData, ClassifyEvalSummaryData, ResponseData,
};
use mxr_provider_fake::fixtures::scenarios::{self, SELF_EMAIL};
use std::sync::Arc;

async fn scenario_state() -> (Arc<AppState>, AccountId) {
    scenario_state_at(Utc::now()).await
}

async fn scenario_state_at(now: chrono::DateTime<Utc>) -> (Arc<AppState>, AccountId) {
    let account_id = AccountId::from_provider_id("fake", SELF_EMAIL);
    let account = Account {
        id: account_id.clone(),
        name: "Alex Demo".to_string(),
        email: SELF_EMAIL.to_string(),
        sync_backend: Some(BackendRef {
            provider_kind: ProviderKind::Fake,
            config_key: "fake".to_string(),
        }),
        send_backend: None,
        enabled: true,
    };
    let provider = Arc::new(
        mxr_provider_fake::FakeProvider::with_scenario_set(account_id.clone(), now, &[])
            .with_page_size(1000),
    );
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, provider.clone(), None)
            .await
            .unwrap(),
    );
    crate::state::load_account_address_registry(&state.store, &state.account_addresses)
        .await
        .unwrap();
    let engine = mxr_sync::SyncEngine::with_address_lookup(
        state.store.clone(),
        state.search.clone(),
        state.account_addresses.clone() as Arc<dyn mxr_core::types::AccountAddressLookup>,
    );
    loop {
        let outcome = engine
            .sync_account_with_outcome(provider.as_ref())
            .await
            .unwrap();
        crate::loops::post_sync_classification(
            &state,
            &account_id,
            provider.as_ref(),
            &outcome.upserted_message_ids,
            false,
            now,
        )
        .await;
        if !outcome.has_more {
            break;
        }
    }
    crate::loops::run_analytics_repair(&state, &account_id).await;
    state.store.refresh_contacts().await.unwrap();
    authenticate_demo_payment_alerts(&state, &account_id, now).await;
    (state, account_id)
}

pub(super) async fn authenticate_demo_payment_alerts(
    state: &AppState,
    account_id: &AccountId,
    now: chrono::DateTime<Utc>,
) {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT id, from_email, date FROM messages
         WHERE account_id = ? AND direction = 'inbound'
           AND (from_email LIKE '%@spotify.com' OR from_email LIKE '%@stripe.com' OR from_email LIKE '%@hollisgym.example' OR from_email LIKE '%@pulsefit.example')
           AND (subject LIKE '%payment%' OR subject LIKE '%Payout%' OR subject LIKE '%declined%')"
    )
    .bind(account_id.as_str())
    .fetch_all(state.store.reader())
    .await
    .unwrap();

    let mut hosts = std::collections::HashSet::new();
    for (_, from_email, _) in &rows {
        if let Some((_, host)) = from_email.rsplit_once('@') {
            hosts.insert(host.to_string());
        }
    }

    for host in hosts {
        for days in [60, 40, 20] {
            let at = now - chrono::Duration::days(days);
            let prior_id = mxr_core::MessageId::new();
            let envelope = mxr_core::types::Envelope {
                id: prior_id.clone(),
                account_id: account_id.clone(),
                thread_id: mxr_core::ThreadId::new(),
                provider_id: format!("prior-{host}-{days}"),
                message_id_header: None,
                in_reply_to: None,
                references: Vec::new(),
                from: mxr_core::types::Address {
                    name: None,
                    email: format!("service@{host}"),
                },
                to: vec![mxr_core::types::Address {
                    name: None,
                    email: account_id.to_string(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: "Your account summary".into(),
                date: at,
                flags: mxr_core::MessageFlags::READ,
                snippet: String::new(),
                has_attachments: false,
                size_bytes: 10,
                unsubscribe: mxr_core::types::UnsubscribeMethod::None,
                link_count: 0,
                body_word_count: 0,
                label_provider_ids: vec![],
                keywords: std::collections::BTreeSet::new(),
            };
            let stored_id = state
                .store
                .upsert_envelope_with_direction(
                    &envelope,
                    mxr_core::types::MessageDirection::Inbound,
                )
                .await
                .unwrap();
            state
                .store
                .insert_body(&mxr_core::MessageBody {
                    message_id: stored_id.clone(),
                    text_plain: Some("Your account summary".into()),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: at,
                    metadata: Default::default(),
                })
                .await
                .unwrap();
            state
                .store
                .record_message_arrival(
                    &stored_id,
                    account_id,
                    &envelope.from.email,
                    &mxr_core::arrival::ArrivalEvidence {
                        received_at: Some(at),
                        sender: mxr_core::arrival::SenderCheck::Verified { host: host.clone() },
                        message_fingerprint: Some(stored_id.to_string()),
                        is_live: false,
                    },
                )
                .await
                .unwrap();
            sqlx::query("UPDATE message_arrivals SET observed_at=? WHERE message_id=?")
                .bind(at.timestamp())
                .bind(stored_id.as_str())
                .execute(state.store.writer())
                .await
                .unwrap();
        }
    }

    for (raw_id, from_email, msg_date) in rows {
        let msg_id = mxr_core::MessageId(uuid::Uuid::parse_str(&raw_id).unwrap());
        let host = from_email
            .rsplit_once('@')
            .expect("from_email host")
            .1
            .to_string();
        let sent = chrono::DateTime::from_timestamp(msg_date, 0).unwrap_or(now);
        let fresh_date = sent.clamp(
            now - chrono::Duration::hours(23),
            now - chrono::Duration::minutes(5),
        );
        state
            .store
            .record_message_arrival(
                &msg_id,
                account_id,
                &from_email,
                &mxr_core::arrival::ArrivalEvidence {
                    received_at: Some(fresh_date),
                    sender: mxr_core::arrival::SenderCheck::Verified { host: host.clone() },
                    message_fingerprint: Some(msg_id.to_string()),
                    is_live: true,
                },
            )
            .await
            .unwrap();
        sqlx::query("UPDATE message_arrivals SET observed_at = ?, is_live = 1, received_at = ?, verified_host = ? WHERE message_id = ?")
            .bind(fresh_date.timestamp())
            .bind(fresh_date.timestamp())
            .bind(&host)
            .bind(msg_id.as_str())
            .execute(state.store.writer())
            .await
            .unwrap();
        crate::handler::alert_todos::project_payment_ask(state, &msg_id, now)
            .await
            .unwrap();
    }
}

async fn eval(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> (ClassifyEvalSummaryData, Vec<ClassifyEvalMismatchData>) {
    eval_at(state, account_id, Utc::now()).await
}

async fn eval_at(
    state: &AppState,
    account_id: Option<&AccountId>,
    now: chrono::DateTime<Utc>,
) -> (ClassifyEvalSummaryData, Vec<ClassifyEvalMismatchData>) {
    for account in state.store.list_accounts().await.unwrap() {
        if account_id.is_none_or(|id| id == &account.id) {
            crate::handler::assessments::reconcile_account_at(state, &account.id, now)
                .await
                .unwrap();
        }
    }

    match classify_eval(state, account_id, ClassifyEvalSetData::Quick)
        .await
        .unwrap()
    {
        ResponseData::ClassifyEval {
            summary,
            mismatches,
        } => (summary, mismatches),
        other => panic!("{other:?}"),
    }
}

fn report(summary: &ClassifyEvalSummaryData, mismatches: &[ClassifyEvalMismatchData]) -> String {
    let mut out = format!(
        "label schema v{}; {} labelled in the set, {} in the mailbox, {} not placed\n",
        summary.label_schema_version, summary.labelled_in_set, summary.present, summary.not_placed
    );
    for q in &summary.questions {
        out.push_str(&format!(
            "{:<7} {:>3}/{:<3} {:>5.1}%\n",
            q.question, q.correct, q.evaluated, q.accuracy_percent
        ));
    }
    out.push('\n');
    out.push_str(&classify_eval_floor::report(
        summary.asks.as_ref().expect("the asks were graded"),
    ));
    out.push_str(&format!("\n{} mismatches\n", mismatches.len()));
    for m in mismatches {
        out.push_str(&format!(
            "{} {} {}: expected {}, got {}{}\n",
            m.scenario,
            m.provider_id,
            m.question,
            m.expected,
            m.actual,
            if m.label_review.is_some() {
                " [label to check]"
            } else {
                ""
            }
        ));
    }
    out
}

#[tokio::test]
async fn the_quick_set_is_graded_and_every_mismatch_is_listed() {
    let (state, _) = scenario_state().await;
    let (summary, mismatches) = eval(&state, None).await;
    assert_eq!(summary.present, summary.labelled_in_set);
    assert_eq!(summary.not_placed, 0);
    assert_eq!(summary.message, None);
    assert!(
        mismatches
            .iter()
            .all(|m| !(m.scenario == "no-reply-vendor-question" && m.question == "reply")),
        "{mismatches:?}"
    );
    if std::env::var_os("MXR_CLASSIFY_TRACE").is_some() {
        use sqlx::Row;
        eprintln!("{}", report(&summary, &mismatches));
        for row in sqlx::query("SELECT m.provider_id, m.subject, a.assessment_json, (SELECT json_group_array(json_object('reason', t.reason, 'state', t.state, 'expired_at_birth', t.expired_at_birth, 'field_sources', json(t.field_sources))) FROM todos t WHERE t.source_message_id = m.id) AS todo_json FROM messages m JOIN mail_assessments a ON a.message_id = m.id WHERE a.policy_version = ?1 ORDER BY m.provider_id")
            .bind(mxr_core::asks::ASKS_POLICY_VERSION)
            .fetch_all(state.store.reader()).await.unwrap()
        {
            eprintln!("ASK_TRACE {}", serde_json::json!({
                "provider_id": row.get::<String, _>("provider_id"),
                "subject": row.get::<String, _>("subject"),
                    "todos": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("todo_json")).expect("synthetic projection JSON"),
                "asks": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("assessment_json")).unwrap(),
            }));
        }
    }
    classify_eval_floor::check("quick", summary.asks.as_ref().expect("asks"));
    insta::assert_snapshot!(report(&summary, &mismatches));
}

#[tokio::test]
async fn a_missing_assessment_is_pending_without_being_repaired_or_graded_as_wrong() {
    let (state, account_id) = scenario_state().await;
    let (before, _) = eval(&state, Some(&account_id)).await;
    let victim = scenarios::quick_set()
        .find(|label| !label.outbound)
        .unwrap();
    let removed = sqlx::query(
        "DELETE FROM mail_assessments WHERE policy_version = ?4 AND message_id IN
         (SELECT id FROM messages WHERE account_id = ?1 AND provider_id = ?2 AND from_email = ?3)",
    )
    .bind(account_id.as_str())
    .bind(&victim.provider_id)
    .bind(&victim.from_email)
    .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
    .execute(state.store.writer())
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(removed, 1);
    let stored_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_assessments")
        .fetch_one(state.store.reader())
        .await
        .unwrap();

    let ResponseData::ClassifyEval {
        summary,
        mismatches,
    } = classify_eval(&state, Some(&account_id), ClassifyEvalSetData::Quick)
        .await
        .unwrap()
    else {
        panic!("expected evaluation");
    };
    assert_eq!(summary.not_placed, before.not_placed + 1);
    for question in ["who", "home"] {
        let old = before
            .questions
            .iter()
            .find(|q| q.question == question)
            .unwrap();
        let new = summary
            .questions
            .iter()
            .find(|q| q.question == question)
            .unwrap();
        assert_eq!(new.evaluated + 1, old.evaluated);
        assert!(!mismatches
            .iter()
            .any(|m| { m.provider_id == victim.provider_id && m.question == question }));
    }
    let stored_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_assessments")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    assert_eq!(
        stored_after, stored_before,
        "evaluation must not repair the missing answer"
    );
}
