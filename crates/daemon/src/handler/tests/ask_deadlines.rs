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
