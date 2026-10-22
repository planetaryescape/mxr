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
    MessagesBandData, MessagesData, MessagesFilterData, MessagesRowKindData, MessagesTurnData,
    PersonPageData, TopicStateData,
};

const SAMIR: &str = "samir@launchpad.example";
const RUTH: &str = "ruth@keystone.example";
const MAYA: &str = "maya@orbit.example";
const JON: &str = "jon@northstar.example";

async fn list(fx: &Fixture) -> MessagesData {
    match request(
        fx,
        Request::ListMessages {
            account_id: None,
            turn: None,
            filter: Default::default(),
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
            conversation: true,
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

    // A profile read (a hover card's) carries the same facts and no message text.
    let profile = match request(
        &fx,
        Request::GetPerson {
            account_id: None,
            person: SAMIR.into(),
            topic: None,
            conversation: false,
        },
    )
    .await
    {
        ResponseData::PersonPage { page } => page,
        other => panic!("expected a person page, got {other:?}"),
    };
    assert!(profile.conversation.is_none());
    assert!(profile.row.preview.is_none());
    assert_eq!(profile.header_line, page.header_line);
    assert_eq!(profile.topics.len(), 3);
    let wire = serde_json::to_string(&profile).unwrap();
    for message in &conversation.messages {
        assert!(
            message.text.is_empty() || !wire.contains(message.text.as_str()),
            "{wire}"
        );
    }
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
    assert_eq!(
        last.source,
        Some(mxr_protocol::MessageTextSourceData::StoredBody)
    );
    assert!(last.trimmed.quote);
    assert_eq!(last.trimmed_label.as_deref(), Some("trimmed: quote"));
    assert_eq!(last.layout, mxr_protocol::MessageLayoutData::Compact);
    let first = &conversation.messages[0];
    assert!(first.from_me);
    assert!(!first.trimmed.any());
    let messages = list(&fx).await;
    assert!(
        messages.your_turn.is_empty(),
        "an answer without a new ask is not reply debt"
    );
    assert_eq!(
        messages.recent.len(),
        1,
        "a fresh answer stays accessible in Recent"
    );
    let preview = messages.recent[0].preview.as_ref().unwrap();
    assert_eq!(preview.text, "Yes, keep it at 5%.");
}

#[tokio::test]
async fn conversation_marks_snippet_fallback_and_preserves_stored_empty_body() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let stored = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Status",
        Duration::days(1),
    )
    .await;
    body(&fx, &stored.id, Some("Stored body."), None).await;
    let empty = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Status",
        Duration::hours(2),
    )
    .await;
    body(&fx, &empty.id, Some("  "), None).await;
    let preview = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Status",
        Duration::hours(1),
    )
    .await;
    // This case deliberately represents a provider preview with no cache.
    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(preview.id.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let page = person(&fx, SAMIR, Some(&thread)).await;
    let messages = page.conversation.unwrap().messages;
    assert_eq!(
        messages[0].source,
        Some(mxr_protocol::MessageTextSourceData::StoredBody)
    );
    assert_eq!(
        messages[1].source,
        Some(mxr_protocol::MessageTextSourceData::StoredBody)
    );
    assert_eq!(messages[1].text, "");
    assert_eq!(messages[2].message_id, preview.id);
    assert_eq!(
        messages[2].source,
        Some(mxr_protocol::MessageTextSourceData::ProviderSnippet)
    );
    assert_eq!(messages[2].text, preview.snippet);
}

/// A thread where Samir answered your deck: Got it has someone to thank.
async fn deck_thread(fx: &Fixture) -> (ThreadId, Envelope) {
    let thread = ThreadId::new();
    let mine = mail(
        fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
    )
    .await;
    body(
        fx,
        &mine.id,
        Some("Hi Samir,\n\nDeck attached.\n\nCheers,\nAlex"),
        None,
    )
    .await;
    let theirs = mail(
        fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Deck",
        Duration::hours(2),
    )
    .await;
    body(fx, &theirs.id, Some("Thanks, reading it tonight."), None).await;
    (thread, theirs)
}

async fn ack_at(
    fx: &Fixture,
    thread: &ThreadId,
    dry_run: bool,
    proof: Option<(&str, &str)>,
    at: chrono::DateTime<Utc>,
) -> Result<mxr_protocol::AckPlanData, String> {
    crate::handler::messages_ack::ack_at(
        &fx.state,
        thread,
        dry_run,
        proof.map(|p| p.0),
        proof.map(|p| p.1),
        at,
    )
    .await
    .map(|data| match data {
        ResponseData::MessagesAck { ack } => ack,
        other => panic!("expected an ack, got {other:?}"),
    })
    .map_err(|e| e.to_string())
}

async fn outbound_count(fx: &Fixture, thread: &ThreadId) -> usize {
    fx.state
        .store
        .desk_messages_in_threads(&fx.account, std::slice::from_ref(thread))
        .await
        .unwrap()
        .iter()
        .filter(|m| m.direction == "outbound")
        .count()
}

#[tokio::test]
async fn got_it_sends_exactly_the_previewed_plan_once() {
    let fx = Fixture::new().await;
    let (thread, theirs) = deck_thread(&fx).await;
    let now = Utc::now();
    let preview = ack_at(&fx, &thread, true, None, now).await.unwrap();
    assert!(preview.dry_run);
    assert_eq!(preview.reply_to_message_id, theirs.id);
    assert_eq!(preview.subject, "Re: Deck");
    assert_eq!(preview.to.len(), 1, "never reply all");
    assert_eq!(preview.to[0].email, SAMIR);
    assert!(preview.text.to_lowercase().contains("got it"));
    let token = preview
        .preview_token
        .clone()
        .expect("a preview carries its token");
    assert_eq!(
        preview.countdown_seconds,
        mxr_protocol::messages_copy::ACK_COUNTDOWN_SECONDS
    );
    let got_it_hint_seen = || async {
        fx.state
            .store
            .hints_seen()
            .await
            .unwrap()
            .contains_key("messages.got_it")
    };
    assert!(!got_it_hint_seen().await, "a preview is not a use");

    let sent = ack_at(&fx, &thread, false, Some((&token, &preview.text)), now)
        .await
        .unwrap();
    assert!(
        got_it_hint_seen().await,
        "sending Got it dismisses its hint in every client"
    );
    let sent_id = sent.sent_message_id.expect("sent");
    let stored = fx.state.store.get_body(&sent_id).await.unwrap().unwrap();
    assert_eq!(
        stored.text_plain.as_deref().map(str::trim),
        Some(preview.text.as_str())
    );
    assert_eq!(outbound_count(&fx, &thread).await, 2);

    // The same token again: already acknowledged, nothing more goes out.
    let again = ack_at(&fx, &thread, false, Some((&token, &preview.text)), now).await;
    assert!(again.unwrap_err().contains("lready"), "a repeat is refused");
    // And a fresh preview has nobody left to acknowledge.
    assert!(ack_at(&fx, &thread, true, None, now).await.is_err());
    assert_eq!(outbound_count(&fx, &thread).await, 2);
    let messages = list(&fx).await;
    assert!(messages
        .your_turn
        .iter()
        .all(|r| r.id != format!("person:{SAMIR}")));
}

#[tokio::test]
async fn got_it_refuses_to_send_without_a_preview() {
    let fx = Fixture::new().await;
    let (thread, _) = deck_thread(&fx).await;
    let now = Utc::now();
    let preview = ack_at(&fx, &thread, true, None, now).await.unwrap();
    let token = preview.preview_token.clone().unwrap();
    for proof in [
        None,
        Some(("", preview.text.as_str())),
        Some((token.as_str(), "Thanks, got it.")),
    ] {
        let refused = ack_at(&fx, &thread, false, proof, now).await;
        assert!(refused.is_err(), "{proof:?} must not send");
    }
    // A token minted by hand, without the daemon's key, is refused too.
    let forged = format!("{}.{}", now.timestamp(), "0".repeat(64));
    assert!(
        ack_at(&fx, &thread, false, Some((&forged, &preview.text)), now)
            .await
            .is_err()
    );
    assert_eq!(outbound_count(&fx, &thread).await, 1, "nothing was sent");
}

#[tokio::test]
async fn a_preview_older_than_a_minute_sends_nothing() {
    let fx = Fixture::new().await;
    let (thread, _) = deck_thread(&fx).await;
    let then = Utc::now();
    let preview = ack_at(&fx, &thread, true, None, then).await.unwrap();
    let token = preview.preview_token.clone().unwrap();
    let late = ack_at(
        &fx,
        &thread,
        false,
        Some((&token, &preview.text)),
        then + Duration::seconds(61),
    )
    .await;
    assert!(late.unwrap_err().contains("review"), "preview again");
    assert_eq!(outbound_count(&fx, &thread).await, 1);
}

#[tokio::test]
async fn a_conversation_that_changed_since_the_preview_sends_nothing() {
    let fx = Fixture::new().await;
    let (thread, _) = deck_thread(&fx).await;
    let now = Utc::now();
    let preview = ack_at(&fx, &thread, true, None, now).await.unwrap();
    let token = preview.preview_token.clone().unwrap();
    // Samir writes again before the countdown ends: a new target.
    let newer = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Deck",
        Duration::minutes(1),
    )
    .await;
    body(
        &fx,
        &newer.id,
        Some("Actually, one more question first?"),
        None,
    )
    .await;
    let changed = ack_at(&fx, &thread, false, Some((&token, &preview.text)), now).await;
    assert!(changed.unwrap_err().contains("changed"));
    assert_eq!(outbound_count(&fx, &thread).await, 1);
}

async fn reply_to_body(fx: &Fixture, message: &MessageId, reply_to: &str) {
    let metadata = MessageMetadata {
        reply_to: vec![Address {
            name: None,
            email: reply_to.to_string(),
        }],
        ..MessageMetadata::default()
    };
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: message.clone(),
            text_plain: Some("Thanks, reading it tonight.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn got_it_checks_where_the_reply_actually_goes() {
    let now = Utc::now();
    // A Reply-To on another domain you never wrote to: refused.
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
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
    reply_to_body(&fx, &theirs.id, "collect@harvester.example").await;
    let refused = ack_at(&fx, &thread, true, None, now).await;
    assert!(refused.unwrap_err().contains("collect@harvester.example"));

    // A no-reply Reply-To on the same domain: refused.
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
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
    reply_to_body(&fx, &theirs.id, "no-reply@launchpad.example").await;
    assert!(ack_at(&fx, &thread, true, None, now).await.is_err());

    // A Reply-To on Samir's own domain is his colleague: allowed.
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
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
    reply_to_body(&fx, &theirs.id, "ops@launchpad.example").await;
    let ok = ack_at(&fx, &thread, true, None, now).await.unwrap();
    assert_eq!(ok.to[0].email, "ops@launchpad.example");
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

/// Samir's latest message is only a quote of Alex's question.
async fn quote_only_thread(fx: &Fixture, earlier_words: Option<&str>) {
    let thread = ThreadId::new();
    let opener = mail(
        fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Rollout",
        Duration::days(2),
    )
    .await;
    if let Some(words) = earlier_words {
        body(fx, &opener.id, Some(words), None).await;
    }
    let asked = mail(
        fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Re: Rollout",
        Duration::days(1),
    )
    .await;
    body(
        fx,
        &asked.id,
        Some("Should we keep the canary at 5% until the dashboard is quiet?"),
        None,
    )
    .await;
    let forward = mail(
        fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Re: Rollout",
        Duration::hours(3),
    )
    .await;
    body(
        fx,
        &forward.id,
        Some(
            "On Mon, Alex wrote:\n> Should we keep the canary at 5% until the dashboard is quiet?",
        ),
        None,
    )
    .await;
}

#[tokio::test]
async fn a_quote_only_latest_message_previews_the_latest_earlier_one_with_words() {
    let fx = Fixture::new().await;
    quote_only_thread(&fx, Some("Rollout starts Monday, watching the dashboard.")).await;
    let messages = list(&fx).await;
    let previews: Vec<String> = rows(&messages)
        .iter()
        .filter_map(|row| row.preview.as_ref().map(|p| p.text.clone()))
        .collect();
    assert_eq!(previews.len(), 1, "{previews:?}");
    assert!(
        !previews[0].contains(mxr_reader::ONLY_QUOTED_TEXT),
        "{previews:?}"
    );
    // Alex's own question is the latest message with words of its own.
    assert_eq!(
        previews[0],
        "You: Should we keep the canary at 5% until the dashboard is quiet?"
    );
}

#[tokio::test]
async fn a_thread_of_only_quotes_has_no_preview_rather_than_the_placeholder() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let only = mail(
        &fx,
        &thread,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "FYI",
        Duration::hours(3),
    )
    .await;
    body(
        &fx,
        &only.id,
        Some(
            "On Mon, Alex wrote:\n> Should we keep the canary at 5% until the dashboard is quiet?",
        ),
        None,
    )
    .await;
    let messages = list(&fx).await;
    let found = rows(&messages);
    assert!(!found.is_empty());
    for row in found {
        assert!(
            row.preview
                .as_ref()
                .is_none_or(|p| !p.text.contains(mxr_reader::ONLY_QUOTED_TEXT)),
            "{:?}",
            row.preview
        );
    }
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

#[tokio::test]
async fn a_send_the_provider_took_but_we_lost_is_never_sent_again() {
    let fx = Fixture::new().await;
    let (thread, _) = deck_thread(&fx).await;
    let now = Utc::now();
    let preview = ack_at(&fx, &thread, true, None, now).await.unwrap();
    let token = preview.preview_token.clone().unwrap();
    // The provider accepts the mail, then the call fails (a timeout).
    fx.fake.fail_sends_after_accepting(true);
    let first = ack_at(&fx, &thread, false, Some((&token, &preview.text)), now).await;
    assert!(first.is_err());
    assert_eq!(fx.fake.sent_drafts().len(), 1);
    fx.fake.fail_sends_after_accepting(false);
    // A retry, with a fresh preview, must not send a second copy.
    let retry_preview = ack_at(&fx, &thread, true, None, now).await;
    let retry = match retry_preview {
        Ok(plan) => {
            let token = plan.preview_token.clone().unwrap();
            ack_at(&fx, &thread, false, Some((&token, &plan.text)), now).await
        }
        Err(e) => Err(e),
    };
    let error = retry.unwrap_err();
    assert!(
        error.contains("needs an outcome check"),
        "says why: {error}"
    );
    assert!(
        error.contains("won't be sent again"),
        "blocks retry: {error}"
    );
    assert!(
        error.contains("mxr send-outcome 'draft:"),
        "gives recovery: {error}"
    );
    assert!(error.contains("--account "), "scopes recovery: {error}");
    assert_eq!(fx.fake.sent_drafts().len(), 1, "never a duplicate");
}

#[tokio::test]
async fn a_list_reply_to_on_the_senders_own_domain_is_refused() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
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
    reply_to_body(&fx, &theirs.id, "partners@launchpad.example").await;
    sqlx::query(
        "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at,
           last_inbound_at, last_outbound_at, total_inbound, total_outbound, replied_count,
           cadence_days_p50, is_list_sender, list_id, refreshed_at)
         VALUES (?, 'partners@launchpad.example', NULL, 0, 0, NULL, NULL, 9, 0, 0, NULL, 1,
           'partners.launchpad.example', 0)",
    )
    .bind(fx.account.as_str())
    .execute(fx.state.store.writer())
    .await
    .unwrap();
    let refused = ack_at(&fx, &thread, true, None, Utc::now()).await;
    assert!(refused.unwrap_err().contains("mailing list"));
}

#[tokio::test]
async fn got_it_only_works_on_a_conversation_in_messages() {
    let fx = Fixture::new().await;
    let copied = ThreadId::new();
    let theirs = mail(
        &fx,
        &copied,
        ("Iris Chen", "iris@meridian.example"),
        &[RUTH],
        &[ME],
        "Offsite",
        Duration::hours(3),
    )
    .await;
    body(
        &fx,
        &theirs.id,
        Some("The offsite is booked for the 20th."),
        None,
    )
    .await;
    let refused = ack_at(&fx, &copied, true, None, Utc::now()).await;
    assert!(refused.unwrap_err().contains("isn't in Messages"));
}

#[tokio::test]
async fn a_name_that_looks_like_markup_stays_text_in_both_parts() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let tricky = "[Alex](https://phish.example) <b>Bold</b> & *Co*";
    mail(
        &fx,
        &thread,
        ("Me", ME),
        &[SAMIR],
        &[],
        "Deck",
        Duration::days(2),
    )
    .await;
    let theirs = mail(
        &fx,
        &thread,
        (tricky, SAMIR),
        &[ME],
        &[],
        "Re: Deck",
        Duration::hours(2),
    )
    .await;
    body(&fx, &theirs.id, Some("Thanks, reading it tonight."), None).await;
    let now = Utc::now();
    let preview = ack_at(&fx, &thread, true, None, now).await.unwrap();
    assert!(
        preview.text.contains("[Alex](https://phish.example)"),
        "{}",
        preview.text
    );
    assert!(!preview.html.contains("<a"), "{}", preview.html);
    assert!(preview.html.contains("[Alex](https://phish.example)"));
    let token = preview.preview_token.clone().unwrap();
    ack_at(&fx, &thread, false, Some((&token, &preview.text)), now)
        .await
        .unwrap();
    let sent = fx.fake.sent_drafts().pop().unwrap();
    match sent.content {
        mxr_core::types::DraftContent::Html { html, text } => {
            assert_eq!(html, preview.html, "the send is the previewed HTML");
            assert_eq!(text.as_deref(), Some(preview.text.as_str()));
        }
        other => panic!("expected the previewed HTML and text, got {other:?}"),
    }
}

/// Maya: your turn, unread, past her usual pace. Samir: your turn, unread,
/// no pace yet. Jon: waiting on him, nothing unread.
async fn seed_filter_rows(fx: &Fixture) {
    // Pace: you answered Maya within an hour, twice, long ago. Pace counts
    // replies linked by In-Reply-To, so the reply names its parent.
    for days in [40, 45] {
        let old = ThreadId::new();
        let asked = fx.message(&old, MAYA, ME, Duration::days(days), None).await;
        fx.message(
            &old,
            ME,
            MAYA,
            Duration::days(days) - Duration::hours(1),
            asked.message_id_header.as_deref(),
        )
        .await;
    }
    let owed = ThreadId::new();
    mail(
        fx,
        &owed,
        ("Maya Ortiz", MAYA),
        &[ME],
        &[],
        "Offsite",
        Duration::days(2),
    )
    .await;
    let fresh = ThreadId::new();
    mail(
        fx,
        &fresh,
        ("Samir Patel", SAMIR),
        &[ME],
        &[],
        "Contract",
        Duration::hours(3),
    )
    .await;
    let waiting = ThreadId::new();
    mail(
        fx,
        &waiting,
        ("Me", ME),
        &[JON],
        &[],
        "Lunch",
        Duration::days(2),
    )
    .await;
}

async fn filtered(
    fx: &Fixture,
    turn: Option<MessagesTurnData>,
    filter: MessagesFilterData,
) -> MessagesData {
    match request(
        fx,
        Request::ListMessages {
            account_id: None,
            turn,
            filter,
            limit: 50,
        },
    )
    .await
    {
        ResponseData::Messages { messages } => messages,
        other => panic!("expected messages, got {other:?}"),
    }
}

/// Every row shown in any band, once each, sorted by id.
fn shown_ids(messages: &MessagesData) -> Vec<String> {
    let mut ids: Vec<String> = messages
        .your_turn
        .iter()
        .chain(&messages.pinned)
        .chain(&messages.recent)
        .chain(&messages.quiet)
        .map(|row| row.id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn person_ids(people: &[&str]) -> Vec<String> {
    let mut ids: Vec<String> = people.iter().map(|who| format!("person:{who}")).collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn each_filter_alone_and_combined_keeps_its_rows_and_not_the_counts() {
    let fx = Fixture::new().await;
    seed_filter_rows(&fx).await;
    let everyone = person_ids(&[MAYA, SAMIR, JON]);
    let unread = MessagesFilterData {
        unread: true,
        ..Default::default()
    };
    let past_pace = MessagesFilterData {
        past_pace: true,
        ..Default::default()
    };
    let both = MessagesFilterData {
        unread: true,
        past_pace: true,
        turn: None,
    };

    let all = filtered(&fx, None, MessagesFilterData::default()).await;
    assert_eq!(shown_ids(&all), everyone);
    assert_eq!(all.row_count, 3);
    assert_eq!(all.matched, 3, "no filter matches what row_count counts");
    assert_eq!(all.filter, MessagesFilterData::default());

    // Each filter below is read after the plain list, so a cache hit on
    // the plain key would show up as unfiltered rows here.
    let only_unread = filtered(&fx, None, unread).await;
    assert_eq!(shown_ids(&only_unread), person_ids(&[MAYA, SAMIR]));
    assert_eq!(only_unread.matched, 2);
    assert_eq!(only_unread.filter, unread);

    let only_past = filtered(&fx, None, past_pace).await;
    assert_eq!(shown_ids(&only_past), person_ids(&[MAYA]));
    assert_eq!(only_past.matched, 1);

    let combined = filtered(&fx, None, both).await;
    assert_eq!(shown_ids(&combined), person_ids(&[MAYA]));
    assert_eq!(combined.matched, 1);

    let mine_unread = filtered(
        &fx,
        None,
        MessagesFilterData {
            turn: Some(MessagesTurnData::Mine),
            unread: true,
            past_pace: false,
        },
    )
    .await;
    assert_eq!(shown_ids(&mine_unread), person_ids(&[MAYA, SAMIR]));

    let theirs = filtered(
        &fx,
        None,
        MessagesFilterData {
            turn: Some(MessagesTurnData::Theirs),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(shown_ids(&theirs), person_ids(&[JON]));
    assert!(theirs.empty_state.is_none());

    // Waiting on them has nothing unread: the filtered empty copy, with
    // the turn named and what is still in Your turn.
    let theirs_unread = filtered(
        &fx,
        None,
        MessagesFilterData {
            turn: Some(MessagesTurnData::Theirs),
            unread: true,
            past_pace: false,
        },
    )
    .await;
    assert!(shown_ids(&theirs_unread).is_empty());
    assert_eq!(theirs_unread.matched, 0);
    assert_eq!(
        theirs_unread.empty_state.as_deref(),
        Some("In Waiting on them, no unread messages from people.\nYour turn still has 2 people.")
    );

    // The header counts ignore every filter.
    for filtered in [
        &only_unread,
        &only_past,
        &combined,
        &mine_unread,
        &theirs_unread,
    ] {
        assert_eq!(filtered.row_count, all.row_count);
        assert_eq!(filtered.thread_count, all.thread_count);
        assert_eq!(filtered.your_turn_split, all.your_turn_split);
    }
    assert!(combined.empty_state.is_none());
}

#[tokio::test]
async fn an_empty_filtered_view_says_what_was_filtered_and_what_is_left() {
    let fx = Fixture::new().await;
    seed_filter_rows(&fx).await;
    // Maya is past pace, but she is your turn, so no one waiting on you is.
    let past_waiting = filtered(
        &fx,
        None,
        MessagesFilterData {
            turn: Some(MessagesTurnData::Theirs),
            past_pace: true,
            unread: false,
        },
    )
    .await;
    assert_eq!(
        past_waiting.empty_state.as_deref(),
        Some("In Waiting on them, nobody is past your usual pace.\nYour turn still has 2 people.")
    );

    let unfiltered_clear = filtered(&fx, Some(MessagesTurnData::Theirs), Default::default()).await;
    assert!(unfiltered_clear.empty_state.is_none());
    assert_eq!(shown_ids(&unfiltered_clear), person_ids(&[JON]));
}

#[tokio::test]
async fn the_filter_wins_over_a_disagreeing_top_level_turn_and_the_top_level_turn_still_works() {
    let fx = Fixture::new().await;
    seed_filter_rows(&fx).await;
    let old_style = match request(
        &fx,
        Request::ListMessages {
            account_id: None,
            turn: Some(MessagesTurnData::Theirs),
            filter: Default::default(),
            limit: 50,
        },
    )
    .await
    {
        ResponseData::Messages { messages } => messages,
        other => panic!("expected messages, got {other:?}"),
    };
    assert_eq!(shown_ids(&old_style), person_ids(&[JON]));
    assert_eq!(old_style.filter.turn, Some(MessagesTurnData::Theirs));

    let disagree = match request(
        &fx,
        Request::ListMessages {
            account_id: None,
            turn: Some(MessagesTurnData::Theirs),
            filter: MessagesFilterData {
                turn: Some(MessagesTurnData::Mine),
                ..Default::default()
            },
            limit: 50,
        },
    )
    .await
    {
        ResponseData::Messages { messages } => messages,
        other => panic!("expected messages, got {other:?}"),
    };
    assert_eq!(shown_ids(&disagree), person_ids(&[MAYA, SAMIR]));
    assert_eq!(disagree.filter.turn, Some(MessagesTurnData::Mine));
}

#[test]
fn each_filter_is_its_own_mode_cache_entry() {
    use crate::handler::mode_cache::ModeView;
    let view = |filter: MessagesFilterData, turn: Option<MessagesTurnData>| {
        ModeView::of(&Request::ListMessages {
            account_id: None,
            turn,
            filter,
            limit: 50,
        })
    };
    let plain = view(Default::default(), None);
    assert_ne!(
        plain,
        view(
            MessagesFilterData {
                unread: true,
                ..Default::default()
            },
            None
        )
    );
    assert_ne!(
        plain,
        view(
            MessagesFilterData {
                past_pace: true,
                ..Default::default()
            },
            None
        )
    );
    assert_ne!(
        plain,
        view(Default::default(), Some(MessagesTurnData::Mine))
    );
    assert_eq!(
        view(
            MessagesFilterData {
                turn: Some(MessagesTurnData::Mine),
                ..Default::default()
            },
            None
        ),
        view(Default::default(), Some(MessagesTurnData::Mine)),
        "the legacy top-level turn and the filter's turn are the same view"
    );
}

/// `who` wrote, then you replied, so they wait on you not and the row is
/// waiting on them. The message they wrote is unread unless `read`.
async fn waiting_on(fx: &Fixture, who: &str, unread: bool, reply_age: Duration) -> Envelope {
    let thread = ThreadId::new();
    let mut asked = fx
        .message(&thread, who, ME, reply_age + Duration::hours(2), None)
        .await;
    if !unread {
        asked.flags = mxr_core::MessageFlags::READ;
        fx.store_envelope(&asked, MessageDirection::Inbound).await;
    }
    fx.message(
        &thread,
        ME,
        who,
        reply_age,
        asked.message_id_header.as_deref(),
    )
    .await;
    asked
}

/// Pinning a person in Messages is adding them to the cadence watchlist.
async fn pin(fx: &Fixture, email: &str) {
    match request(
        fx,
        Request::WatchCadence {
            account_id: fx.account.clone(),
            email: email.to_string(),
            expected_days: None,
            note: None,
            allow_list_sender: false,
        },
    )
    .await
    {
        ResponseData::Ack => {}
        other => panic!("expected Ack, got {other:?}"),
    }
}

fn pinned_ids(messages: &MessagesData) -> Vec<String> {
    let mut ids: Vec<String> = messages.pinned.iter().map(|row| row.id.clone()).collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_pinned_unread_person_past_the_cap_shows_under_unread() {
    let fx = Fixture::new().await;
    // Ten pinned people, newest first: the tenth is past Pinned's nine, and
    // the only one with unread mail from them.
    let mut ids = Vec::new();
    for n in 0..10 {
        let who = format!("pinned{n}@cap.example");
        waiting_on(&fx, &who, n == 9, Duration::hours(n + 1)).await;
        pin(&fx, &who).await;
        ids.push(format!("person:{who}"));
    }
    ids.sort();

    let all = filtered(&fx, None, MessagesFilterData::default()).await;
    assert_eq!(
        pinned_ids(&all),
        ids[..9].to_vec(),
        "unfiltered Pinned keeps nine"
    );
    assert_eq!(all.row_count, 9);

    let unread = filtered(
        &fx,
        None,
        MessagesFilterData {
            unread: true,
            ..Default::default()
        },
    )
    .await;
    let tenth = format!("person:pinned9@cap.example");
    assert!(!all.pinned.iter().any(|row| row.id == tenth));
    assert_eq!(
        pinned_ids(&unread),
        vec![tenth],
        "the filter sees every pinned person"
    );
    assert_eq!(unread.matched, 1);
    assert!(
        unread.empty_state.is_none(),
        "a matching row is not an empty view"
    );
    assert_eq!(unread.row_count, 9, "header counts ignore the filter");
}

#[tokio::test]
async fn one_correspondent_in_two_accounts_is_two_rows_and_matched_counts_both() {
    let fx = Fixture::new().await;
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Other".into(),
        email: "other@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state
        .store
        .insert_account(&other)
        .await
        .expect("account");
    let maya = "maya@orbit.example";
    fx.message(&ThreadId::new(), ME, maya, Duration::hours(2), None)
        .await;
    let mut there = fx
        .message(&ThreadId::new(), ME, maya, Duration::hours(3), None)
        .await;
    there.id = MessageId::new();
    there.account_id = other.id.clone();
    there.provider_id = format!("other-{}", there.id);
    there.message_id_header = Some(format!("<{}@other>", there.id));
    fx.store_envelope(&there, MessageDirection::Outbound).await;

    let all = filtered(&fx, None, MessagesFilterData::default()).await;
    let maya_rows = all
        .recent
        .iter()
        .filter(|row| row.id == format!("person:{maya}"))
        .count();
    assert_eq!(maya_rows, 2, "one row per account");
    assert_eq!(all.row_count, 2);
    assert_eq!(
        all.matched, 2,
        "no filter matches every row, once per account"
    );
}
