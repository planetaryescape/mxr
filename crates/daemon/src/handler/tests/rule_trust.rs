use super::desk::{request, Fixture, ME};
use chrono::Duration;
use mxr_core::{AttachmentId, AttachmentMeta, MessageBody, MessageDirection, ThreadId};
use mxr_protocol::{Request, ResponseData};
use serde_json::{json, Value};

async fn save_rule(fx: &Fixture, id: &str, condition: Value, actions: Value, enabled: bool) {
    let now = chrono::Utc::now();
    request(
        fx,
        Request::UpsertRule {
            rule: json!({
                "id": id, "name": id, "account_id": fx.account, "enabled": enabled,
                "priority": 10, "conditions": condition, "actions": actions,
                "created_at": now, "updated_at": now
            }),
        },
    )
    .await;
}

async fn preview(fx: &Fixture, rule: Option<&str>, after: Option<&str>) -> Vec<Value> {
    match request(
        fx,
        Request::DryRunRules {
            rule: rule.map(str::to_string),
            all: rule.is_none(),
            after: after.map(str::to_string),
        },
    )
    .await
    {
        ResponseData::RuleDryRun { results } => results,
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn persisted_link_preview_matches_live_fake_effects() {
    let fx = Fixture::new().await;
    let mut messages = Vec::new();
    for (links, words) in [(0, 100), (1, 100), (10, 50)] {
        let mut envelope = fx
            .message(
                &ThreadId::new(),
                "editor@example.com",
                ME,
                Duration::minutes(1),
                None,
            )
            .await;
        envelope.link_count = links;
        envelope.body_word_count = words;
        fx.store_envelope(&envelope, MessageDirection::Inbound)
            .await;
        messages.push(envelope);
    }
    let cases = [
        (
            "any",
            json!({"type":"field","field":"link_density","match_kind":"any"}),
            vec![1, 2],
        ),
        (
            "heavy",
            json!({"type":"field","field":"link_density","match_kind":"heavy"}),
            vec![2],
        ),
        (
            "none",
            json!({"type":"field","field":"link_density","match_kind":"none"}),
            vec![0],
        ),
        (
            "not-any",
            json!({"type":"not","condition":{"type":"field","field":"link_density","match_kind":"any"}}),
            vec![0],
        ),
        (
            "nested",
            json!({"type":"and","conditions":[{"type":"field","field":"has_label","label":"INBOX"},{"type":"or","conditions":[{"type":"field","field":"link_density","match_kind":"heavy"},{"type":"not","condition":{"type":"field","field":"link_density","match_kind":"any"}}]}]}),
            vec![0, 2],
        ),
    ];
    for (id, condition, expected) in cases {
        save_rule(
            &fx,
            id,
            condition,
            json!([{"type":"add_label","label":id}]),
            true,
        )
        .await;
        let result = preview(&fx, Some(id), None).await;
        let mut ids: Vec<_> = result[0]["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["message_id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        let mut expected_ids: Vec<_> = expected
            .iter()
            .map(|&i| messages[i].id.to_string())
            .collect();
        expected_ids.sort();
        assert_eq!(ids, expected_ids, "preview {id}");
    }
    assert!(
        fx.fake.mutations().is_empty(),
        "preview must not mutate the provider"
    );
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        &messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    for (index, message) in messages.iter().enumerate() {
        let expected = match index {
            0 => vec!["none", "not-any", "nested"],
            1 => vec!["any"],
            _ => vec!["any", "heavy", "nested"],
        };
        let mut logged: Vec<_> = fx
            .state
            .store
            .list_rule_logs(None, 100)
            .await
            .unwrap()
            .iter()
            .map(mxr_store::row_to_rule_log_json)
            .filter(|log| log["message_id"] == message.id.to_string())
            .map(|log| log["rule_id"].as_str().unwrap().to_string())
            .collect();
        logged.sort();
        let mut expected: Vec<_> = expected.into_iter().map(str::to_string).collect();
        expected.sort();
        assert_eq!(logged, expected, "live message {index}");
    }
    assert_eq!(fx.fake.mutations().len(), 7);
}

#[tokio::test]
async fn live_action_order_stops_on_hook_failure() {
    let fx = Fixture::new().await;
    let message = fx
        .message(
            &ThreadId::new(),
            "sender@example.com",
            ME,
            Duration::minutes(1),
            None,
        )
        .await;
    save_rule(
        &fx,
        "ordered",
        json!({"type":"field","field":"is_unread"}),
        json!([
            {"type":"mark_read"}, {"type":"shell_hook","command":"exit 7"}, {"type":"mark_unread"}
        ]),
        true,
    )
    .await;
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        std::slice::from_ref(&message.id),
    )
    .await
    .unwrap();
    assert!(fx
        .state
        .store
        .get_envelope(&message.id)
        .await
        .unwrap()
        .unwrap()
        .flags
        .contains(mxr_core::MessageFlags::READ));
    assert_eq!(
        fx.fake.mutations().len(),
        1,
        "actions after the failure must not execute"
    );
    let logs = fx
        .state
        .store
        .list_rule_logs(Some("ordered"), 10)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    let log = mxr_store::row_to_rule_log_json(&logs[0]);
    assert_eq!(log["success"], false);
    assert!(log["error"].as_str().unwrap().contains("code Some(7)"));
}

#[tokio::test]
async fn ordinary_preview_preserves_account_date_and_disabled_selection() {
    let fx = Fixture::new().await;
    let mut first = fx
        .message(
            &ThreadId::new(),
            "editor@example.com",
            ME,
            Duration::minutes(1),
            None,
        )
        .await;
    first.date = "2026-03-17T00:00:00Z".parse().unwrap();
    fx.store_envelope(&first, MessageDirection::Inbound).await;
    let mut old = fx
        .message(
            &ThreadId::new(),
            "editor@example.com",
            ME,
            Duration::days(2),
            None,
        )
        .await;
    old.date = "2026-03-16T23:59:59Z".parse().unwrap();
    fx.store_envelope(&old, MessageDirection::Inbound).await;
    let mut other_account = fx
        .state
        .store
        .get_account(&fx.account)
        .await
        .unwrap()
        .unwrap();
    other_account.id = mxr_core::AccountId::new();
    other_account.name = "other-fake".into();
    fx.state.store.insert_account(&other_account).await.unwrap();
    let mut foreign = first.clone();
    foreign.id = mxr_core::MessageId::new();
    foreign.thread_id = ThreadId::new();
    foreign.provider_id = "foreign-fake".into();
    foreign.account_id = other_account.id;
    fx.state.store.upsert_envelope(&foreign).await.unwrap();
    save_rule(&fx, "disabled", json!({"type":"field","field":"from","pattern":{"type":"exact","value":"editor@example.com"}}), json!([{"type":"mark_read"}]), false).await;
    let results = preview(&fx, Some("disabled"), Some("2026-03-17")).await;
    assert_eq!(
        results[0]["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["message_id"].clone())
            .collect::<Vec<_>>(),
        vec![json!(first.id.to_string())]
    );
    assert!(preview(&fx, None, None).await.is_empty());
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        &[first.id.clone(), old.id],
    )
    .await
    .unwrap();
    assert!(fx.fake.mutations().is_empty());
    save_rule(&fx, "disabled", json!({"type":"field","field":"from","pattern":{"type":"exact","value":"editor@example.com"}}), json!([{"type":"mark_read"}]), true).await;
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &foreign.account_id,
        fx.fake.as_ref(),
        &[foreign.id],
    )
    .await
    .unwrap();
    assert!(
        fx.fake.mutations().is_empty(),
        "account-scoped live rule affected other account"
    );
}

#[tokio::test]
async fn live_hook_receives_stored_mail_and_preview_never_executes_it() {
    let fx = Fixture::new().await;
    let dir = tempfile::tempdir().unwrap();
    let capture = dir.path().join("capture.jsonl");
    let marker = dir.path().join("must-not-execute");
    let cached = dir.path().join("cached.bin");
    std::fs::write(&cached, b"cached").unwrap();
    let missing = dir.path().join("missing.bin");
    let malicious = format!(
        "$(touch {}) `touch {}` ; ' \\",
        marker.display(),
        marker.display()
    );
    let mut messages = Vec::new();
    for kind in ["plain", "missing", "html", "empty"] {
        let mut envelope = fx
            .message(
                &ThreadId::new(),
                "sender@example.com",
                ME,
                Duration::days(2),
                None,
            )
            .await;
        envelope.from.name = Some(malicious.clone());
        envelope.subject = format!("{kind} {malicious}");
        envelope.date = "2026-03-17T10:30:00Z".parse().unwrap();
        envelope.has_attachments = kind == "plain";
        fx.store_envelope(&envelope, MessageDirection::Inbound)
            .await;
        if kind != "missing" {
            let attachments = if kind == "plain" {
                [
                    Some(cached.clone()),
                    Some(missing.clone()),
                    Some(dir.path().to_path_buf()),
                    None,
                ]
                .into_iter()
                .enumerate()
                .map(|(i, local_path)| AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: envelope.id.clone(),
                    filename: format!("file-{i}-{malicious}"),
                    mime_type: "application/octet-stream".into(),
                    disposition: Default::default(),
                    content_id: None,
                    content_location: None,
                    size_bytes: 6 + i as u64,
                    local_path,
                    provider_id: format!("attachment-{i}"),
                })
                .collect()
            } else {
                vec![]
            };
            fx.state
                .store
                .insert_body(&MessageBody {
                    message_id: envelope.id.clone(),
                    text_plain: match kind {
                        "plain" => Some(malicious.clone()),
                        "empty" => Some(String::new()),
                        _ => None,
                    },
                    text_html: Some(format!("<p>{malicious}</p>")),
                    attachments,
                    fetched_at: chrono::Utc::now(),
                    metadata: Default::default(),
                })
                .await
                .unwrap();
        }
        messages.push(envelope);
    }
    save_rule(&fx, "capture", json!({"type":"field","field":"from","pattern":{"type":"exact","value":"sender@example.com"}}), json!([{"type":"shell_hook","command":format!("cat >> '{}' && printf '\\n' >> '{}'", capture.display(), capture.display())}]), true).await;
    assert_eq!(
        preview(&fx, Some("capture"), None).await[0]["matches"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(!capture.exists(), "preview ran the hook");
    crate::loops::apply_rules_to_messages(
        &fx.state,
        &fx.account,
        fx.fake.as_ref(),
        &messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    let captured: Vec<Value> = std::fs::read_to_string(&capture)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(captured.len(), 4);
    for (envelope, payload) in messages.iter().zip(&captured) {
        assert_eq!(payload["id"], envelope.id.to_string());
        assert_eq!(
            payload["from"],
            json!({"name": malicious, "email":"sender@example.com"})
        );
        assert_eq!(payload["subject"], envelope.subject);
        assert_eq!(payload["date"], envelope.date.to_rfc3339());
    }
    assert_eq!(captured[0]["body_text"], malicious);
    assert_eq!(captured[1]["body_text"], Value::Null);
    assert_eq!(
        captured[2]["body_text"],
        Value::Null,
        "HTML must not be mislabeled as plain text"
    );
    assert_eq!(captured[3]["body_text"], "");
    let attachments = captured[0]["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 4);
    for i in 0..4 {
        let attachment = attachments
            .iter()
            .find(|a| a["filename"] == format!("file-{i}-{malicious}"))
            .unwrap();
        assert_eq!(attachment["size_bytes"], 6 + i as u64);
        assert_eq!(
            attachment["local_path"],
            if i == 0 {
                json!(cached.display().to_string())
            } else {
                Value::Null
            }
        );
    }
    assert!(
        !marker.exists(),
        "mail text escaped JSON into the shell command"
    );
    assert_eq!(fx.fake.attachment_fetch_count(), 0);
}
