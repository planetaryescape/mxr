use super::desk::{request, Fixture, ME};
use crate::handler::rule_treatment;
use chrono::Duration;
use mxr_core::ThreadId;
use mxr_protocol::{ModeKindData, Request, ResponseData, RuleFormData};

fn form(fx: &Fixture) -> RuleFormData {
    RuleFormData {
        id: None,
        account_id: Some(fx.account.clone()),
        name: "Reading notes".into(),
        condition: "from:editor@example.com".into(),
        action: "treatment:reading".into(),
        priority: 10,
        enabled: true,
    }
}
async fn mail(fx: &Fixture) -> mxr_core::Envelope {
    fx.message(
        &ThreadId::new(),
        "editor@example.com",
        ME,
        Duration::minutes(2),
        None,
    )
    .await
}
fn preview(data: ResponseData) -> serde_json::Value {
    match data {
        ResponseData::RuleTreatmentResult { preview } => serde_json::to_value(preview).unwrap(),
        other => panic!("Unexpected {other:?}"),
    }
}

#[tokio::test]
async fn preview_apply_sync_and_manual_correction_use_one_classification() {
    let fx = Fixture::new().await;
    let message = mail(&fx).await;
    let mut form = form(&fx);
    let before = preview(rule_treatment::run(&fx.state, &form, None).await.unwrap());
    assert_eq!(before["result"]["matches"].as_array().unwrap().len(), 1);
    assert_eq!(before["result"]["matches"][0]["before"], "messages");
    assert_eq!(before["result"]["matches"][0]["after"], "reading");
    assert!(fx.state.store.list_rules().await.unwrap().is_empty());
    assert!(fx
        .state
        .store
        .rule_treatments(&fx.account)
        .await
        .unwrap()
        .is_empty());
    let applied = preview(
        rule_treatment::run(&fx.state, &form, before["token"].as_str())
            .await
            .unwrap(),
    );
    assert_eq!(applied["result"], before["result"]);
    form.id = applied["rule_id"].as_str().map(str::to_string);
    assert_eq!(
        fx.state.store.rule_treatments(&fx.account).await.unwrap()[&message.id].0,
        "reading"
    );
    let moved = request(
        &fx,
        Request::MoveMessage {
            message_id: message.id.clone(),
            mode: ModeKindData::Messages,
            sender: false,
            dry_run: false,
            source: None,
        },
    )
    .await;
    assert!(matches!(moved, ResponseData::MessageMoved { .. }));
    let blocked = preview(rule_treatment::run(&fx.state, &form, None).await.unwrap());
    assert_eq!(blocked["result"]["matches"][0]["after"], "messages");
    assert_eq!(blocked["result"]["matches"][0]["blocked"], true);
    let next = mail(&fx).await;
    for _ in 0..2 {
        crate::loops::apply_rules_to_messages(
            &fx.state,
            &fx.account,
            fx.fake.as_ref(),
            &[message.id.clone(), next.id.clone()],
        )
        .await
        .unwrap();
    }
    let replay = preview(rule_treatment::run(&fx.state, &form, None).await.unwrap());
    let rows = replay["result"]["matches"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .find(|r| r["message_id"] == message.id.to_string())
            .unwrap()["after"],
        "messages"
    );
    assert_eq!(
        rows.iter()
            .find(|r| r["message_id"] == next.id.to_string())
            .unwrap()["after"],
        "reading"
    );
    assert!(fx
        .state
        .store
        .list_rule_logs(None, 100)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn changed_preview_and_reused_token_cannot_apply() {
    let fx = Fixture::new().await;
    mail(&fx).await;
    let form = form(&fx);
    let before = preview(rule_treatment::run(&fx.state, &form, None).await.unwrap());
    mail(&fx).await;
    assert!(
        rule_treatment::run(&fx.state, &form, before["token"].as_str())
            .await
            .is_err()
    );
    assert!(fx.state.store.list_rules().await.unwrap().is_empty());
    let before = preview(rule_treatment::run(&fx.state, &form, None).await.unwrap());
    rule_treatment::run(&fx.state, &form, before["token"].as_str())
        .await
        .unwrap();
    assert!(
        rule_treatment::run(&fx.state, &form, before["token"].as_str())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn json_edit_stamps_revision_and_preserves_historical_sorting() {
    let fx = Fixture::new().await;
    let message = mail(&fx).await;
    let form = form(&fx);
    let before = preview(rule_treatment::run(&fx.state, &form, None).await.unwrap());
    let applied = preview(
        rule_treatment::run(&fx.state, &form, before["token"].as_str())
            .await
            .unwrap(),
    );
    let id = applied["rule_id"].as_str().unwrap().to_string();
    let old = request(&fx, Request::GetRule { rule: id }).await;
    let mut value = match old {
        ResponseData::RuleData { rule } => rule,
        other => panic!("{other:?}"),
    };
    let old_revision = value["updated_at"].clone();
    value["actions"] = serde_json::json!([{"type":"set_treatment","treatment":"updates"}]);
    let saved = request(&fx, Request::UpsertRule { rule: value }).await;
    let value = match saved {
        ResponseData::RuleData { rule } => rule,
        other => panic!("{other:?}"),
    };
    assert_ne!(value["updated_at"], old_revision);
    assert_eq!(
        fx.state.store.rule_treatments(&fx.account).await.unwrap()[&message.id].0,
        "reading"
    );
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        std::slice::from_ref(&message.id),
    )
    .await
    .unwrap();
    assert_eq!(
        fx.state.store.rule_treatments(&fx.account).await.unwrap()[&message.id].0,
        "reading"
    );
}

#[tokio::test]
async fn treatment_requires_one_existing_account_and_valid_action() {
    let fx = Fixture::new().await;
    let mut draft = form(&fx);
    draft.account_id = None;
    assert!(rule_treatment::run(&fx.state, &draft, None).await.is_err());
    draft.account_id = Some(mxr_core::AccountId::new());
    assert!(rule_treatment::run(&fx.state, &draft, None).await.is_err());
    draft = form(&fx);
    draft.action = "treatment:todo".into();
    assert!(rule_treatment::run(&fx.state, &draft, None).await.is_err());
}

#[tokio::test]
async fn simultaneous_previews_cannot_both_commit() {
    let fx = Fixture::new().await;
    mail(&fx).await;
    let draft = form(&fx);
    let first = preview(rule_treatment::run(&fx.state, &draft, None).await.unwrap());
    let second = preview(rule_treatment::run(&fx.state, &draft, None).await.unwrap());
    let (a, b) = tokio::join!(
        rule_treatment::run(&fx.state, &draft, first["token"].as_str()),
        rule_treatment::run(&fx.state, &draft, second["token"].as_str()),
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(fx.state.store.list_rules().await.unwrap().len(), 1);
}

#[tokio::test]
async fn copied_messages_stay_in_messages_and_deleted_sources_keep_history() {
    use mxr_core::{Address, MessageDirection};
    use mxr_protocol::{ArrivalBucketData, KindRuleData, SenderKindData};
    let fx = Fixture::new().await;
    let mut message = mail(&fx).await;
    message.to = vec![Address {
        name: None,
        email: "colleague@example.com".into(),
    }];
    message.cc = vec![Address {
        name: None,
        email: ME.into(),
    }];
    fx.store_envelope(&message, MessageDirection::Inbound).await;
    let mut draft = form(&fx);
    draft.action = "treatment:messages".into();
    let p = preview(rule_treatment::run(&fx.state, &draft, None).await.unwrap());
    assert_eq!(p["result"]["matches"][0]["after"], "messages");
    let applied = preview(
        rule_treatment::run(&fx.state, &draft, p["token"].as_str())
            .await
            .unwrap(),
    );
    let id = applied["rule_id"].as_str().unwrap().to_string();
    let kind = request(
        &fx,
        Request::GetMessageKind {
            message_id: message.id.clone(),
        },
    )
    .await;
    match kind {
        ResponseData::MessageKind { mail_kind, .. } => {
            assert_eq!(mail_kind.kind, SenderKindData::People);
            assert_eq!(mail_kind.rule, KindRuleData::CustomRule);
        }
        other => panic!("{other:?}"),
    }
    match request(
        &fx,
        Request::GetArrivalModes {
            message_ids: vec![message.id.clone()],
        },
    )
    .await
    {
        ResponseData::ArrivalModes { items } => {
            assert_eq!(items[0].bucket, ArrivalBucketData::Messages)
        }
        other => panic!("{other:?}"),
    }
    request(&fx, Request::DeleteRule { rule: id }).await;
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        std::slice::from_ref(&message.id),
    )
    .await
    .unwrap();
    let stored = fx.state.store.rule_treatments(&fx.account).await.unwrap();
    assert_eq!(stored[&message.id].0, "messages");
    assert!(stored[&message.id].1.contains("deleted rule"));
}

#[tokio::test]
async fn body_predicates_are_rejected_recursively_and_archive_is_excluded() {
    let fx = Fixture::new().await;
    let message = mail(&fx).await;
    for condition in [
        "body:hello",
        "NOT body:hello",
        "from:editor@example.com OR has:link",
        "from:editor@example.com AND has:link-none",
    ] {
        let mut draft = form(&fx);
        draft.condition = condition.into();
        let error = rule_treatment::run(&fx.state, &draft, None)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("header conditions"),
            "{condition}: {error}"
        );
    }
    fx.state
        .store
        .set_message_labels(&message.id, &[], mxr_core::EventSource::User)
        .await
        .unwrap();
    let p = preview(
        rule_treatment::run(&fx.state, &form(&fx), None)
            .await
            .unwrap(),
    );
    assert!(p["result"]["matches"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn now_read_before_sync_still_applies_rule_before_first_placement() {
    let fx = Fixture::new().await;
    let draft = form(&fx);
    request(
        &fx,
        Request::UpsertRuleForm {
            existing_rule: None,
            account_id: draft.account_id.clone(),
            name: draft.name.clone(),
            condition: draft.condition.clone(),
            action: draft.action.clone(),
            priority: draft.priority,
            enabled: true,
        },
    )
    .await;
    let message = mail(&fx).await;
    request(
        &fx,
        Request::GetArrivals {
            account_id: Some(fx.account.clone()),
            mark_seen: false,
            since: None,
        },
    )
    .await;
    let stored = fx.state.store.rule_treatments(&fx.account).await.unwrap();
    assert_eq!(stored[&message.id].0, "reading");
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        std::slice::from_ref(&message.id),
    )
    .await
    .unwrap();
    assert_eq!(
        fx.state.store.rule_treatments(&fx.account).await.unwrap()[&message.id].0,
        "reading"
    );
    match request(
        &fx,
        Request::GetArrivalModes {
            message_ids: vec![message.id],
        },
    )
    .await
    {
        ResponseData::ArrivalModes { items } => {
            assert_eq!(items[0].bucket, mxr_protocol::ArrivalBucketData::Reading)
        }
        other => panic!("{other:?}"),
    }
}
