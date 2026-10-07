//! Messages end to end on the handler: people as rows with their topics,
//! group threads as their own rows, copied threads in Updates, the shared
//! owed rule and its decay, manual merges with a suggestion, new text with
//! the trimmed marker, and Got it's preview equal to what it sends.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageDirection, MessageMetadata};
use mxr_protocol::{
    MessagesBandData, MessagesData, MessagesRowKindData, PersonPageData, TopicStateData,
};

const SAMIR: &str = "samir@launchpad.example";
const RUTH: &str = "ruth@keystone.example";

async fn list(fx: &Fixture) -> MessagesData {
    match request(
        fx,
        Request::ListMessages {
            account_id: None,
            turn: None,
            limit: 50,
        },
    )
    .await
    {
        ResponseData::Messages { messages } => messages,
        other => panic!("expected messages, got {other:?}"),
    }
}

async fn person(fx: &Fixture, who: &str, topic: Option<&ThreadId>) -> PersonPageData {
    match request(
        fx,
        Request::GetPerson {
            account_id: None,
            person: who.into(),
            topic: topic.cloned(),
        },
    )
    .await
    {
        ResponseData::PersonPage { page } => page,
        other => panic!("expected a person page, got {other:?}"),
    }
}

/// A message with a name on the sender and every recipient given.
async fn mail(
    fx: &Fixture,
    thread: &ThreadId,
    from: (&str, &str),
    to: &[&str],
    cc: &[&str],
    subject: &str,
    age: Duration,
) -> Envelope {
    let mut envelope = fx.message(thread, from.1, ME, age, None).await;
    envelope.from = Address {
        name: Some(from.0.to_string()),
        email: from.1.to_string(),
    };
    let address = |email: &&str| Address {
        name: None,
        email: (*email).to_string(),
    };
    envelope.to = to.iter().map(address).collect();
    envelope.cc = cc.iter().map(address).collect();
    envelope.subject = subject.to_string();
    let direction = if from.1 == ME {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    envelope
}

async fn body(fx: &Fixture, message: &MessageId, text: Option<&str>, html: Option<&str>) {
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: message.clone(),
            text_plain: text.map(str::to_string),
            text_html: html.map(str::to_string),
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        })
        .await
        .unwrap();
}

fn rows(messages: &MessagesData) -> Vec<&mxr_protocol::MessagesRowData> {
    messages
        .your_turn
        .iter()
        .chain(&messages.recent)
        .chain(&messages.quiet)
        .collect()
}

#[tokio::test]
async fn a_person_is_one_row_a_group_is_its_own_and_copied_mail_is_in_updates() {
    let fx = Fixture::new().await;
    let contract = ThreadId::new();
    mail(
        &fx,
        &contract,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Contract renewal",
        Duration::days(2),
    )
    .await;
    mail(
        &fx,
        &contract,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Contract renewal",
        Duration::hours(16),
    )
    .await;
    let checklist = ThreadId::new();
    mail(
        &fx,
        &checklist,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Launch checklist",
        Duration::days(1),
    )
    .await;
    let group = ThreadId::new();
    mail(
        &fx,
        &group,
        ("Samir Patel", SAMIR),
        &[ME, RUTH],
        &[],
        "Pricing copy",
        Duration::days(3),
    )
    .await;
    mail(
        &fx,
        &group,
        ("Ruth Vega", RUTH),
        &[SAMIR, ME],
        &[],
        "Re: Pricing copy",
        Duration::days(2),
    )
    .await;
    // You're only copied, and you never wrote in it.
    let copied = ThreadId::new();
    mail(
        &fx,
        &copied,
        ("Iris Chen", "iris@meridian.example"),
        &[RUTH],
        &[ME],
        "Offsite",
        Duration::hours(3),
    )
    .await;

    let messages = list(&fx).await;
    let all = rows(&messages);
    let samir: Vec<_> = all
        .iter()
        .filter(|r| r.id == format!("person:{SAMIR}"))
        .collect();
    assert_eq!(samir.len(), 1, "Samir is one row");
    let samir = samir[0];
    assert_eq!(samir.band, MessagesBandData::YourTurn);
    assert_eq!(samir.title, "Samir Patel");
    assert_eq!(samir.topics.len(), 2, "both one-to-one threads are topics");
    assert_eq!(samir.topics[0].thread_id, contract);
    assert_eq!(samir.topics[0].state, TopicStateData::YourTurn);
    assert_eq!(samir.topics[1].state, TopicStateData::Waiting);

    let group_row = all
        .iter()
        .find(|r| r.id == format!("group:{group}"))
        .expect("the group thread is its own row");
    assert_eq!(group_row.kind, MessagesRowKindData::Group);
    assert_eq!(group_row.title, "Samir, Ruth");

    assert!(
        all.iter()
            .all(|r| !r.topics.iter().any(|t| t.thread_id == copied)),
        "a copied thread is not in Messages"
    );
    match request(
        &fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(copied.clone()),
            thread_ids: vec![],
        },
    )
    .await
    {
        ResponseData::ModeMembership { threads } => {
            let modes: Vec<_> = threads[0].modes.iter().map(|m| m.mode).collect();
            assert_eq!(modes, vec![mxr_protocol::ModeKindData::Updates]);
        }
        other => panic!("expected membership, got {other:?}"),
    }

    // The person page lists the group under "with".
    let page = person(&fx, SAMIR, None).await;
    assert_eq!(page.topics.len(), 3);
    let with: Vec<_> = page.topics.iter().filter(|t| !t.with.is_empty()).collect();
    assert_eq!(with.len(), 1);
    assert_eq!(with[0].with, vec!["Ruth".to_string()]);
    let conversation = page.conversation.expect("the your-turn topic opens");
    assert_eq!(conversation.thread_id, contract);
    assert!(conversation.composer.label.starts_with("Reply to Samir · "));
    assert!(!conversation.composer.reply_all);
}

#[tokio::test]
async fn your_turn_goes_quiet_after_a_week_without_a_cadence() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        ("Me", ME),
        &["leo@workbench.example"],
        &[],
        "Desk lamp",
        Duration::days(12),
    )
    .await;
    mail(
        &fx,
        &thread,
        ("Leo Park", "leo@workbench.example"),
        &[ME],
        &[],
        "Re: Desk lamp",
        Duration::days(9),
    )
    .await;
    let messages = list(&fx).await;
    assert!(messages.your_turn.is_empty());
    assert_eq!(
        messages.empty_state.as_deref(),
        Some(mxr_protocol::messages_copy::CLEAR)
    );
    let leo = messages
        .quiet
        .iter()
        .find(|r| r.id == "person:leo@workbench.example")
        .expect("Leo went quiet");
    assert_eq!(leo.topics[0].state, TopicStateData::Quiet);
}

#[tokio::test]
async fn closer_people_rank_first_in_your_turn() {
    let fx = Fixture::new().await;
    // Years of mail both ways with Maya.
    for days in [400, 300, 200, 100, 60, 40, 30, 20, 15, 12, 10, 8] {
        let t = ThreadId::new();
        mail(
            &fx,
            &t,
            ("Me", ME),
            &["maya@orbit.example"],
            &[],
            "Notes",
            Duration::days(days),
        )
        .await;
        mail(
            &fx,
            &t,
            ("Maya Ortiz", "maya@orbit.example"),
            &[ME],
            &[],
            "Re: Notes",
            Duration::days(days) - Duration::hours(2),
        )
        .await;
    }
    let newer = ThreadId::new();
    mail(
        &fx,
        &newer,
        ("Me", ME),
        &["theo@northstar.example"],
        &[],
        "Hello",
        Duration::days(2),
    )
    .await;
    mail(
        &fx,
        &newer,
        ("Theo Nash", "theo@northstar.example"),
        &[ME],
        &[],
        "Re: Hello",
        Duration::hours(30),
    )
    .await;
    let maya = ThreadId::new();
    mail(
        &fx,
        &maya,
        ("Me", ME),
        &["maya@orbit.example"],
        &[],
        "Launch",
        Duration::days(1),
    )
    .await;
    mail(
        &fx,
        &maya,
        ("Maya Ortiz", "maya@orbit.example"),
        &[ME],
        &[],
        "Re: Launch",
        Duration::hours(2),
    )
    .await;
    fx.state.store.refresh_contacts().await.unwrap();

    let messages = list(&fx).await;
    let order: Vec<&str> = messages.your_turn.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        order,
        vec!["person:maya@orbit.example", "person:theo@northstar.example"]
    );
    assert_eq!(
        messages.your_turn[0].closeness,
        mxr_protocol::ClosenessData::Close
    );
}

#[tokio::test]
async fn merging_is_manual_with_a_suggestion_and_makes_one_row() {
    let fx = Fixture::new().await;
    let work = ThreadId::new();
    mail(
        &fx,
        &work,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Contract",
        Duration::days(3),
    )
    .await;
    mail(
        &fx,
        &work,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Contract",
        Duration::days(2),
    )
    .await;
    let home = ThreadId::new();
    mail(
        &fx,
        &home,
        ("Me", ME),
        &["samir.p@home.example"],
        &[],
        "Dinner",
        Duration::days(4),
    )
    .await;
    mail(
        &fx,
        &home,
        ("Samir Patel", "samir.p@home.example"),
        &[ME],
        &[],
        "Re: Dinner",
        Duration::days(1),
    )
    .await;
    fx.state.store.refresh_contacts().await.unwrap();

    let before = list(&fx).await;
    assert_eq!(before.merge_suggestion_count, 1);
    let samir_rows = rows(&before)
        .into_iter()
        .filter(|r| r.title == "Samir Patel")
        .count();
    assert_eq!(samir_rows, 2, "never merged on its own");

    let account = fx.account.clone();
    let preview = match request(
        &fx,
        Request::MergePeople {
            account_id: account.clone(),
            into: SAMIR.into(),
            addresses: vec!["samir.p@home.example".into()],
            dry_run: true,
        },
    )
    .await
    {
        ResponseData::PersonMerge { merge } => merge,
        other => panic!("expected a merge, got {other:?}"),
    };
    assert!(preview.dry_run);
    assert_eq!(preview.person.addresses.len(), 2);
    assert_eq!(preview.thread_count, 2);
    assert_eq!(
        rows(&list(&fx).await)
            .into_iter()
            .filter(|r| r.title == "Samir Patel")
            .count(),
        2,
        "a dry run changes nothing"
    );

    request(
        &fx,
        Request::MergePeople {
            account_id: account.clone(),
            into: SAMIR.into(),
            addresses: vec!["samir.p@home.example".into()],
            dry_run: false,
        },
    )
    .await;
    let after = list(&fx).await;
    let samir: Vec<_> = rows(&after)
        .into_iter()
        .filter(|r| r.title == "Samir Patel")
        .collect();
    assert_eq!(samir.len(), 1);
    assert_eq!(samir[0].topics.len(), 2);
    assert_eq!(
        after.merge_suggestion_count, 0,
        "a done merge is not suggested"
    );

    request(
        &fx,
        Request::SplitPerson {
            account_id: account,
            address: "samir.p@home.example".into(),
            dry_run: false,
        },
    )
    .await;
    assert_eq!(
        rows(&list(&fx).await)
            .into_iter()
            .filter(|r| r.title == "Samir Patel")
            .count(),
        2
    );
}

#[tokio::test]
async fn a_conversation_shows_new_text_with_the_trimmed_marker() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let asked = mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Rollout",
        Duration::days(1),
    )
    .await;
    body(
        &fx,
        &asked.id,
        Some("Hi Samir,\n\nShould we keep the canary at 5% until the dashboard is quiet?\n\nAlex"),
        None,
    )
    .await;
    let reply = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Rollout",
        Duration::hours(3),
    )
    .await;
    body(
        &fx,
        &reply.id,
        None,
        Some(r#"<div dir="ltr">Yes, keep it at 5%.</div><div class="gmail_quote"><div class="gmail_attr">On Mon, Alex wrote:</div><blockquote class="gmail_quote">Should we keep the canary at 5% until the dashboard is quiet?</blockquote></div>"#),
    )
    .await;

    let page = person(&fx, SAMIR, None).await;
    let conversation = page.conversation.unwrap();
    let last = conversation.messages.last().unwrap();
    assert_eq!(last.text, "Yes, keep it at 5%.");
    assert!(last.trimmed.quote);
    assert_eq!(last.trimmed_label.as_deref(), Some("trimmed: quote"));
    assert_eq!(last.layout, mxr_protocol::MessageLayoutData::Compact);
    let first = &conversation.messages[0];
    assert!(first.from_me);
    assert!(!first.trimmed.any());
    let messages = list(&fx).await;
    let preview = messages.your_turn[0].preview.as_ref().unwrap();
    assert_eq!(preview.text, "Yes, keep it at 5%.");
}

#[tokio::test]
async fn got_it_previews_the_text_it_sends() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mine = mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
    )
    .await;
    body(
        &fx,
        &mine.id,
        Some("Hi Samir,\n\nDeck attached.\n\nCheers,\nAlex"),
        None,
    )
    .await;
    let theirs = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Deck",
        Duration::hours(2),
    )
    .await;
    body(&fx, &theirs.id, Some("Thanks, reading it tonight."), None).await;

    let preview = match request(
        &fx,
        Request::AckMessage {
            thread_id: thread.clone(),
            dry_run: true,
            expect_text: None,
        },
    )
    .await
    {
        ResponseData::MessagesAck { ack } => ack,
        other => panic!("expected an ack, got {other:?}"),
    };
    assert!(preview.dry_run);
    assert_eq!(preview.reply_to_message_id, theirs.id);
    assert_eq!(preview.subject, "Re: Deck");
    assert_eq!(preview.to[0].email, SAMIR);
    assert!(preview.text.contains("got it") || preview.text.contains("Got it"));
    assert_eq!(
        preview.countdown_seconds,
        mxr_protocol::messages_copy::ACK_COUNTDOWN_SECONDS
    );

    // Anything but the previewed text is refused.
    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::AckMessage {
            thread_id: thread.clone(),
            dry_run: false,
            expect_text: Some("something else".into()),
        }),
    };
    assert!(matches!(
        handle_request(&fx.state, &msg).await.payload,
        IpcPayload::Response(Response::Error { .. })
    ));

    let sent = match request(
        &fx,
        Request::AckMessage {
            thread_id: thread.clone(),
            dry_run: false,
            expect_text: Some(preview.text.clone()),
        },
    )
    .await
    {
        ResponseData::MessagesAck { ack } => ack,
        other => panic!("expected an ack, got {other:?}"),
    };
    let sent_id = sent.sent_message_id.expect("sent");
    let stored = fx.state.store.get_body(&sent_id).await.unwrap().unwrap();
    assert_eq!(
        stored.text_plain.as_deref().map(str::trim),
        Some(preview.text.as_str())
    );
    // Your turn passed to them.
    let messages = list(&fx).await;
    assert!(messages
        .your_turn
        .iter()
        .all(|r| r.id != format!("person:{SAMIR}")));
}

#[tokio::test]
async fn messages_is_built_on_the_rail() {
    let fx = Fixture::new().await;
    match request(&fx, Request::GetRail { account_id: None }).await {
        ResponseData::Rail { rail } => {
            let messages = rail.entries.iter().find(|e| e.id == "messages").unwrap();
            assert_eq!(messages.status, mxr_protocol::RailStatusData::Built);
            assert!(messages.early_note.is_none());
        }
        other => panic!("expected the rail, got {other:?}"),
    }
}

#[tokio::test]
async fn a_quote_only_reply_shows_a_placeholder_not_the_quoted_words() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let asked = mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Rollout",
        Duration::days(1),
    )
    .await;
    body(
        &fx,
        &asked.id,
        Some("Should we keep the canary at 5% until the dashboard is quiet?"),
        None,
    )
    .await;
    let forward = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Rollout",
        Duration::hours(3),
    )
    .await;
    body(
        &fx,
        &forward.id,
        Some(
            "On Mon, Alex wrote:\n> Should we keep the canary at 5% until the dashboard is quiet?",
        ),
        None,
    )
    .await;
    let page = person(&fx, SAMIR, None).await;
    let last = page.conversation.unwrap().messages.pop().unwrap();
    assert_eq!(last.text, mxr_reader::ONLY_QUOTED_TEXT);
    assert!(last.only_quoted);
    assert_eq!(last.trimmed_label.as_deref(), Some("trimmed: quote"));
}

async fn updates(fx: &Fixture) -> Vec<mxr_protocol::PlaceBundleData> {
    match request(
        fx,
        Request::ListPlace {
            place: mxr_protocol::MailPlaceData::PaperTrail,
            account_id: None,
            sender_email: None,
            limit: 50,
            offset: 0,
            messages_per_bundle: 5,
            message_offset: 0,
        },
    )
    .await
    {
        ResponseData::Place { bundles, .. } => bundles,
        other => panic!("expected a place, got {other:?}"),
    }
}

#[tokio::test]
async fn a_copied_thread_is_listed_in_updates_and_leaves_when_done_there() {
    let fx = Fixture::new().await;
    let copied = ThreadId::new();
    mail(
        &fx,
        &copied,
        ("Iris Chen", "iris@meridian.example"),
        &[RUTH],
        &[ME],
        "Offsite",
        Duration::hours(3),
    )
    .await;
    let bundles = updates(&fx).await;
    let iris = bundles
        .iter()
        .find(|b| b.sender_email == "iris@meridian.example")
        .expect("the copied thread is in Updates' list, as its label says");
    assert_eq!(iris.kind.rule, mxr_protocol::KindRuleData::Copied);
    assert!(iris.kind.reason.contains("copied"), "{}", iris.kind.reason);
    assert_eq!(iris.messages[0].thread_id, copied);

    request(
        &fx,
        Request::SetModeDone {
            thread_ids: vec![copied.clone()],
            mode: mxr_protocol::ModeKindData::Updates,
            dry_run: false,
            todo_ids: vec![],
            sender: None,
        },
    )
    .await;
    assert!(updates(&fx)
        .await
        .iter()
        .all(|b| b.sender_email != "iris@meridian.example"));
}
