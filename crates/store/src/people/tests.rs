use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::Store;
use chrono::Duration;
use mxr_core::types::{Address, MessageDirection};

async fn fixture() -> (Store, AccountId) {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    (store, account.id)
}

fn linked(links: &[PersonLink]) -> Vec<(String, String)> {
    links
        .iter()
        .map(|l| (l.email.clone(), l.person_email.clone()))
        .collect()
}

#[tokio::test]
async fn a_merge_links_addresses_and_carries_their_own_links() {
    let (store, account) = fixture().await;
    let now = Utc::now();
    store
        .merge_people(
            &account,
            &PersonMerge {
                person_email: "sam@work.example".into(),
                addresses: vec!["Sam@Home.example".into()],
            },
            now,
        )
        .await
        .unwrap();
    store
        .merge_people(
            &account,
            &PersonMerge {
                person_email: "sam@new.example".into(),
                addresses: vec!["sam@work.example".into()],
            },
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        linked(&store.person_links(&account).await.unwrap()),
        vec![
            ("sam@home.example".into(), "sam@new.example".into()),
            ("sam@work.example".into(), "sam@new.example".into()),
        ]
    );
}

#[tokio::test]
async fn splitting_the_primary_hands_the_person_to_the_oldest_link() {
    let (store, account) = fixture().await;
    let now = Utc::now();
    for (address, at) in [
        ("a@two.example", now),
        ("a@three.example", now + Duration::seconds(5)),
    ] {
        store
            .merge_people(
                &account,
                &PersonMerge {
                    person_email: "a@one.example".into(),
                    addresses: vec![address.into()],
                },
                at,
            )
            .await
            .unwrap();
    }
    assert!(store.split_person(&account, "a@one.example").await.unwrap());
    assert_eq!(
        linked(&store.person_links(&account).await.unwrap()),
        vec![("a@three.example".into(), "a@two.example".into())]
    );
    assert!(!store
        .split_person(&account, "nobody@x.example")
        .await
        .unwrap());
}

#[tokio::test]
async fn person_threads_cover_mail_from_them_and_mail_you_sent_them() {
    let (store, account) = fixture().await;
    let now = Utc::now();
    let mut ids = Vec::new();
    for (from, to, direction, hours) in [
        (
            "samir@launchpad.example",
            "me@example.com",
            MessageDirection::Inbound,
            5,
        ),
        (
            "me@example.com",
            "Samir@Launchpad.example",
            MessageDirection::Outbound,
            2,
        ),
        (
            "ruth@keystone.example",
            "me@example.com",
            MessageDirection::Inbound,
            1,
        ),
    ] {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        envelope.provider_id = format!("people-{hours}");
        envelope.thread_id = ThreadId::new();
        envelope.date = now - Duration::hours(hours);
        envelope.from = Address {
            name: None,
            email: from.into(),
        };
        envelope.to = vec![Address {
            name: None,
            email: to.into(),
        }];
        store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .unwrap();
        ids.push(envelope.thread_id);
    }
    let threads = store
        .person_thread_ids(
            &account,
            &["samir@launchpad.example".to_string()],
            now - Duration::days(30),
            10,
        )
        .await
        .unwrap();
    assert_eq!(threads, vec![ids[1].clone(), ids[0].clone()]);
}
