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

async fn contact(store: &Store, account: &AccountId, email: &str, name: &str, outbound: i64) {
    sqlx::query(
        "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at,
           last_inbound_at, last_outbound_at, total_inbound, total_outbound, replied_count,
           cadence_days_p50, is_list_sender, list_id, refreshed_at)
         VALUES (?, ?, ?, 0, 0, NULL, NULL, 3, ?, 1, 9.5, 0, NULL, 0)",
    )
    .bind(account.as_str())
    .bind(email)
    .bind(name)
    .bind(outbound)
    .execute(store.writer())
    .await
    .unwrap();
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
    // Merging the person into a third address moves the old link with it.
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
    for (address, at) in [("a@two.example", now), ("a@three.example", now + Duration::seconds(5))] {
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
    assert!(!store.split_person(&account, "nobody@x.example").await.unwrap());
}

#[tokio::test]
async fn merge_candidates_need_the_same_name_and_mail_from_you_to_both() {
    let (store, account) = fixture().await;
    contact(&store, &account, "samir@launchpad.example", "Samir Patel", 12).await;
    contact(&store, &account, "samir.patel@gmail.example", "samir patel", 2).await;
    // Same name, never written to: not a candidate.
    contact(&store, &account, "samir@spam.example", "Samir Patel", 0).await;
    // Written to, unique name.
    contact(&store, &account, "ruth@keystone.example", "Ruth Vega", 4).await;
    let candidates = store.merge_suggestion_candidates(&account).await.unwrap();
    let emails: Vec<&str> = candidates.iter().map(|c| c.email.as_str()).collect();
    assert_eq!(
        emails,
        vec!["samir.patel@gmail.example", "samir@launchpad.example"]
    );
}

#[tokio::test]
async fn person_threads_cover_mail_from_them_and_mail_you_sent_them() {
    let (store, account) = fixture().await;
    let now = Utc::now();
    let mut ids = Vec::new();
    for (from, to, direction, hours) in [
        ("samir@launchpad.example", "me@example.com", MessageDirection::Inbound, 5),
        ("me@example.com", "Samir@Launchpad.example", MessageDirection::Outbound, 2),
        ("ruth@keystone.example", "me@example.com", MessageDirection::Inbound, 1),
    ] {
        let mut envelope = TestEnvelopeBuilder::new().account_id(account.clone()).build();
        envelope.provider_id = format!("people-{hours}");
        envelope.thread_id = ThreadId::new();
        envelope.date = now - Duration::hours(hours);
        envelope.from = Address { name: None, email: from.into() };
        envelope.to = vec![Address { name: None, email: to.into() }];
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

#[tokio::test]
async fn facts_read_the_contacts_row() {
    let (store, account) = fixture().await;
    contact(&store, &account, "maya@orbit.example", "Maya Ortiz", 48).await;
    let facts = store
        .people_facts(&account, &["MAYA@orbit.example".to_string()])
        .await
        .unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].total_outbound, 48);
    assert_eq!(facts[0].cadence_days_p50, Some(9.5));
}
