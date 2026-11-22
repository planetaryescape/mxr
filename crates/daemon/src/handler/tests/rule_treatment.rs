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
