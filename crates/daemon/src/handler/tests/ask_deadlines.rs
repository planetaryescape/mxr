//! Deadline scope keeps a completion request and its evidence together.
use chrono::Utc;
use mxr_core::asks::AskAnswer;
use mxr_core::{AccountId, MailSyncProvider, OwnedAddresses};
use mxr_provider_fake::{fixtures::scenarios, FakeProvider};

#[tokio::test]
async fn dated_completion_continuations_keep_the_generated_source_deadline() {
    let now = Utc::now()
        .date_naive()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    let mut found = 0;
    let mut missing = Vec::new();
    for email in [scenarios::SELF_EMAIL, scenarios::WORK_EMAIL] {
        let account = AccountId::from_provider_id("fake", email);
        let provider = FakeProvider::with_demo_dataset_at(account.clone(), 50_000, now, &[]);
        let addresses = OwnedAddresses::new(Some(email), std::iter::empty::<&str>());
        let is_owner = |address: &str| addresses.contains(address);
        for provider_id in ["demo-msg-6368", "demo-msg-2108"] {
            let synced = provider
                .fetch_message(provider_id)
                .await
                .unwrap()
                .expect("one generated message");
            let mail = synced.envelope;
            let body = synced.body;
            if mail.subject != "Security training" {
                continue;
            }
            found += 1;
            let label =
                scenarios::full_label_for(&account, provider_id, &mail.thread_id, &mail.from.email)
                    .expect("actual account-scoped label");
            let answer = mxr_todo::asks::assess(
                &mxr_todo::asks::AskInput {
                    message_id: &mail.id,
                    account_id: &account,
                    subject: &mail.subject,
                    body: body.text_plain.as_deref(),
                    from: &mail.from,
                    to: &mail.to,
                    cc: &mail.cc,
                    bcc: &mail.bcc,
                    list_mail: false,
                    bulk: false,
                    owner_names: &["Alex".into(), "Alex Demo".into(), "Alex Work".into()],
                    owner_addresses: &[email.into()],
                    is_owner: &is_owner,
                    sent: mail.date,
                    predecessor: None,
                    owner_replied_afterwards: true,
                    reply_capable: true,
                },
                &Utc,
                "native-source-deadline".into(),
            );
            eprintln!(
                "deadline source {}",
                serde_json::json!({"account": account, "provider": provider_id, "message": mail.id, "sent": mail.date, "now": now, "body": body.text_plain, "dates": answer.facts.dates, "units": answer.units, "zones": answer.facts.zones, "label_items": format!("{:?}", label.items)})
            );
            assert!(!answer.facts.dates.is_empty());
            let actions: Vec<_> = answer
                .units
                .iter()
                .filter(|u| u.answer == AskAnswer::AsksYou)
                .collect();
            assert_eq!(
                actions.len(),
                1,
                "the generated completion is one actual action"
            );
            for unit in actions {
                assert_eq!(
                    unit.tags.as_ref().unwrap().action,
                    mxr_core::asks::ActionKind::Other
                );
                assert_eq!(
                    answer.quote(unit),
                    body.text_plain
                        .as_deref()
                        .and_then(|text| text.split("\n\n").next())
                );
                if unit.tags.as_ref().and_then(|t| t.due).is_none() {
                    missing.push((account.clone(), provider_id));
                }
            }
        }
    }
    assert_eq!(found, 2);
    assert!(
        missing.is_empty(),
        "completion deliverables lost their deadline: {missing:?}"
    );
}

#[tokio::test]
async fn completion_projection_uses_its_stored_date_and_expires_only_when_past() {
    use mxr_core::{Account, BackendRef, MessageDirection, ProviderKind};
    use std::sync::Arc;
    let now = Utc::now()
        .date_naive()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    for past in [false, true] {
        let email = scenarios::WORK_EMAIL;
        let account_id = AccountId::from_provider_id("fake", email);
        let provider = Arc::new(FakeProvider::with_demo_dataset_at(
            account_id.clone(),
            50_000,
            now,
            &[],
        ));
        let source = provider
            .fetch_message("demo-msg-6368")
            .await
            .unwrap()
            .unwrap();
        let mut mail = source.envelope;
        let mut body = source.body;
        let due = now + chrono::Duration::days(if past { -10 } else { 3 });
        if past {
            mail.date -= chrono::Duration::days(13);
        }
        let quote = format!("Hi Alex, the training must be completed by {}. Please finish it and send me the certificate.", due.format("%A %-d %B %Y"));
        body.text_plain = Some(quote.clone());
        let account = Account {
            id: account_id.clone(),
            name: "Alex Work".into(),
            email: email.into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".into(),
            }),
            send_backend: None,
            enabled: true,
        };
        let state = crate::state::AppState::in_memory_with_sync_provider(account, provider, None)
            .await
            .unwrap();
        crate::state::load_account_address_registry(&state.store, &state.account_addresses)
            .await
            .unwrap();
        state
            .store
            .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
            .await
            .unwrap();
        state.store.insert_body(&body).await.unwrap();
        super::super::todos::scan_messages_at(&state, std::slice::from_ref(&mail.id), now).await;
        let stored = state
            .store
            .asks_for_messages(&account_id, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        let answer = &stored[&mail.id];
        let task = answer
            .units
            .iter()
            .find(|u| u.answer == AskAnswer::AsksYou)
            .expect("completion action");
        assert_eq!(answer.quote(task), Some(quote.as_str()));
        assert_eq!(
            task.tags.as_ref().unwrap().action,
            mxr_core::asks::ActionKind::Other
        );
        let date = &answer.facts.dates[task.tags.as_ref().unwrap().due.unwrap()];
        let row: (String, String, i64, String, i64) = sqlx::query_as("SELECT kind, reason, due_at, state, expired_at_birth FROM todos WHERE source_message_id=?").bind(mail.id.as_str()).fetch_one(state.store.reader()).await.unwrap();
        assert_eq!(row.0, "other");
        assert_eq!(row.1, quote);
        assert_eq!(row.2, date.at.timestamp());
        assert_eq!(row.3, if past { "expired" } else { "open" });
        assert_eq!(row.4, i64::from(past));
        super::super::todos::scan_messages_at(&state, std::slice::from_ref(&mail.id), now).await;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM todos WHERE source_message_id=?")
            .bind(mail.id.as_str())
            .fetch_one(state.store.reader())
            .await
            .unwrap();
        assert_eq!(count, 1, "repeat projection retains one identity");
    }
}
