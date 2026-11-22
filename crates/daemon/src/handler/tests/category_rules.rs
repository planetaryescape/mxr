use super::desk::{Fixture, ME};
use crate::handler::category_rules;
use chrono::Duration;
use mxr_core::{CategoryId, Envelope, ThreadId};
use mxr_protocol::{ResponseData, RuleCategoriesPreviewData, RuleFormData};
use mxr_store::CategoryMembershipChange;

async fn mail(fx: &Fixture, from: &str) -> Envelope {
    fx.message(&ThreadId::new(), from, ME, Duration::minutes(2), None)
        .await
}
fn form(fx: &Fixture, category: &CategoryId, name: &str, add: bool, priority: i32) -> RuleFormData {
    RuleFormData {
        id: None,
        account_id: Some(fx.account.clone()),
        name: name.into(),
        condition: "from:airline@example.com".into(),
        action: format!("{}-category:{category}", if add { "add" } else { "remove" }),
        priority,
        enabled: true,
    }
}
async fn preview(
    fx: &Fixture,
    form: &RuleFormData,
    token: Option<&str>,
    delete: bool,
) -> RuleCategoriesPreviewData {
    match category_rules::run(&fx.state, form, token, delete)
        .await
        .unwrap()
    {
        ResponseData::RuleCategoriesResult { preview } => preview,
        other => panic!("unexpected {other:?}"),
    }
}
async fn save(fx: &Fixture, form: &mut RuleFormData) -> RuleCategoriesPreviewData {
    let before = preview(fx, form, None, false).await;
    let after = preview(fx, form, before.token.as_deref(), false).await;
    assert_eq!(after.matches, before.matches);
    form.id = Some(after.rule_id.clone());
    after
}
async fn assigned(fx: &Fixture, message: &Envelope, category: &CategoryId) -> bool {
    fx.state
        .store
        .category_membership_versions(
            None,
            std::slice::from_ref(&message.id),
            std::slice::from_ref(category),
        )
        .await
        .unwrap()[0]
        .assigned
}
async fn manual(fx: &Fixture, message: &Envelope, category: &CategoryId, assigned: bool) -> String {
    let before = fx
        .state
        .store
        .category_membership_versions(
            None,
            std::slice::from_ref(&message.id),
            std::slice::from_ref(category),
        )
        .await
        .unwrap()
        .remove(0);
    fx.state
        .store
        .apply_category_membership_changes(&[CategoryMembershipChange { before, assigned }])
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn category_rules_overlap_manual_replay_lifecycle_and_undo() {
    let fx = Fixture::new().await;
    let category = fx
        .state
        .store
        .create_personal_category("Travel")
        .await
        .unwrap();
    let a = mail(&fx, "airline@example.com").await;
    let b = mail(&fx, "ordinary@example.com").await;
    let labels = fx.state.store.get_message_label_ids(&a.id).await.unwrap();
    let flags = a.flags;
    let mut first = form(&fx, &category.id, "Airline", true, 10);
    let mut second = form(&fx, &category.id, "Remove airline", false, 20);
    save(&fx, &mut first).await;
    save(&fx, &mut second).await;
    assert!(assigned(&fx, &a, &category.id).await);
    let undo = manual(&fx, &a, &category.id, false).await;
    manual(&fx, &b, &category.id, true).await;
    for _ in 0..2 {
        crate::loops::apply_rules_to_messages(
            &fx.state,
            &fx.account,
            fx.fake.as_ref(),
            &[a.id.clone(), b.id.clone()],
        )
        .await
        .unwrap();
    }
    assert!(!assigned(&fx, &a, &category.id).await);
    assert!(assigned(&fx, &b, &category.id).await);
    assert_eq!(
        fx.state.store.category_rule_sources().await.unwrap().len(),
        2
    );
    first.enabled = false;
    let disabled = save(&fx, &mut first).await;
    assert_eq!(disabled.matches[0].manual_decision, Some(false));
    assert_eq!(disabled.matches[0].winner, second.id);
    fx.state
        .store
        .undo_category_membership(&undo)
        .await
        .unwrap();
    assert!(!assigned(&fx, &a, &category.id).await);
    second.enabled = false;
    save(&fx, &mut second).await;
    assert!(!assigned(&fx, &a, &category.id).await);
    assert!(assigned(&fx, &b, &category.id).await);
    first.enabled = true;
    first.condition = "from:ordinary@example.com".into();
    save(&fx, &mut first).await;
    assert_eq!(
        fx.state.store.category_rule_sources().await.unwrap().len(),
        1
    );
    assert!(assigned(&fx, &b, &category.id).await);
    let deletion = preview(&fx, &first, None, true).await;
    preview(&fx, &first, deletion.token.as_deref(), true).await;
    assert!(fx
        .state
        .store
        .category_rule_sources()
        .await
        .unwrap()
        .is_empty());
    assert!(assigned(&fx, &b, &category.id).await);
    assert_eq!(
        fx.state.store.get_message_label_ids(&a.id).await.unwrap(),
        labels
    );
    assert_eq!(
        fx.state
            .store
            .get_envelope(&a.id)
            .await
            .unwrap()
            .unwrap()
            .flags,
        flags
    );
}

#[tokio::test]
async fn category_sync_reads_after_gate_and_rejects_stale_transaction_eligibility() {
    let fx = Fixture::new().await;
    let category = fx
        .state
        .store
        .create_personal_category("Travel")
        .await
        .unwrap();
    let mut message = mail(&fx, "airline@example.com").await;
    let mut rule = form(&fx, &category.id, "Airline", true, 10);
    save(&fx, &mut rule).await;
    let stale = fx
        .state
        .store
        .category_rule_message_snapshot(&message.id)
        .await
        .unwrap();
    let sources = fx.state.store.category_rule_sources().await.unwrap();
    let gate = fx.state.rule_mutation_gate.lock().await;
    let state = fx.state.clone();
    let id = message.id.clone();
    let waiting = tokio::spawn(async move {
        let _gate = state.rule_mutation_gate.lock().await;
        category_rules::classify_message(&state, &id).await
    });
    message.from.email = "ordinary@example.com".into();
    fx.state.store.upsert_envelope(&message).await.unwrap();
    assert!(fx
        .state
        .store
        .reconcile_message_category_sources(&message.id, &sources, &stale)
        .await
        .is_err());
    drop(gate);
    waiting.await.unwrap().unwrap();
    assert!(fx
        .state
        .store
        .category_rule_sources()
        .await
        .unwrap()
        .is_empty());
    assert!(!assigned(&fx, &message, &category.id).await);
}
