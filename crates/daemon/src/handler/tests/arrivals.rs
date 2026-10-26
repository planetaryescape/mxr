//! Sorting shows its work (D119): the arrivals line sums and each count
//! opens exactly its emails, the never-bury rule, corrections and their
//! precedence, undo, the Not-sure cap and the track record.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::arrivals::arrivals_at;
use chrono::{Duration, FixedOffset, TimeZone, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageDirection, MessageFlags};
use mxr_protocol::{
    ArrivalBucketData, ArrivalsData, DaemonEvent, KindRuleData, ModeKindData, MoveOutcomeData,
    ThreadModesData,
};

/// One message: `to` and `cc` as given, list headers when `list` is set.
async fn mail(
    fx: &Fixture,
    thread: &ThreadId,
    from: &str,
    to: &[&str],
    cc: &[&str],
    list: bool,
    age: Duration,
) -> Envelope {
    let mut envelope = fx
        .message(thread, from, to.first().copied().unwrap_or(ME), age, None)
        .await;
    let addr = |email: &&str| Address {
        name: None,
        email: (*email).to_string(),
    };
    envelope.to = to.iter().map(addr).collect();
    envelope.cc = cc.iter().map(addr).collect();
    envelope.subject = format!("Note from {from}");
    if list {
        envelope.unsubscribe = UnsubscribeMethod::OneClick {
            url: "https://lists.example/unsub".into(),
        };
    }
    let direction = if from == ME {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    cached_content(
        fx,
        &envelope,
        "",
        Some("Could you reply with your availability?"),
    )
    .await;
    envelope
}

async fn inbound(fx: &Fixture, from: &str, list: bool) -> Envelope {
    mail(
        fx,
        &ThreadId::new(),
        from,
        &[ME],
        &[],
        list,
        Duration::minutes(5),
    )
    .await
}

async fn cached_content(fx: &Fixture, mail: &Envelope, headers: &str, text: Option<&str>) {
    fx.state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: mail.id.clone(),
            text_plain: text.map(str::to_string),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: mxr_core::MessageMetadata {
                raw_headers: Some(headers.into()),
                ..Default::default()
            },
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn explicit_reply_later_preserves_copied_purpose_and_adds_messages_attention() {
    let fx = Fixture::new().await;
    let copied = mail(
        &fx,
        &ThreadId::new(),
        "sam@studio.example",
        &["ruth@studio.example"],
        &[ME],
        false,
        Duration::minutes(5),
    )
    .await;
    cached_content(&fx, &copied, "", Some("FYI: the report is ready.")).await;
    fx.state
        .store
        .defer_reply_later(
            &copied.id,
            Utc::now() - Duration::hours(2),
            Utc::now() - Duration::hours(1),
        )
        .await
        .unwrap();
    let held = membership(&fx, &copied.thread_id).await;
    assert_eq!(
        modes(&held),
        vec![ModeKindData::Messages, ModeKindData::Updates]
    );
    let ResponseData::Desk { owed, .. } = fx.desk(100).await else {
        panic!("desk");
    };
    assert_eq!(owed.rows.len(), 1);
    assert_eq!(owed.rows[0].thread_id, copied.thread_id);
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Updates);
}

#[tokio::test]
async fn current_arrival_projection_crosses_page_boundaries_without_rewriting_legacy_rows() {
    let fx = Fixture::new().await;
    for _ in 0..401 {
        fx.message(
            &ThreadId::new(),
            "no-reply@batch.example",
            ME,
            Duration::minutes(5),
            None,
        )
        .await;
    }
    sqlx::query("UPDATE arrivals SET mode='messages', rule='person', reason='legacy'")
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let arrivals = line(&fx).await;
    assert_eq!(arrivals.total, 401);
    assert_eq!(count(&arrivals, ArrivalBucketData::Updates), 401);
    let ids = list(&fx, &arrivals, ArrivalBucketData::Updates).await;
    assert_eq!(ids.len(), 401);
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        401
    );
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM arrivals WHERE mode='messages' AND reason='legacy' AND moved_at IS NULL")
        .fetch_one(fx.state.store.reader()).await.unwrap();
    assert_eq!(stored, 401);
}

#[tokio::test]
async fn bare_greeting_attention_agrees_across_shared_surfaces_without_guessing_names() {
    for (text, recipient, expected, action) in [
        ("Hi,\n\nCould you send the deck by Friday?", "owned", true, true),
        ("Hi, could you confirm the meeting time?", "owned", true, false),
        ("Hello,\nCould you review the plan?", "owned", true, true),
        ("Hey, could you reply today?", "owned", true, false),
        ("Good morning,\nCould you confirm?", "owned", true, false),
        ("Hope you're well,\nCould you send the deck?", "owned", true, true),
        ("Alice,\nCould you send the deck?", "owned", false, false),
        ("Hi Alice, could you confirm?", "owned", false, false),
        ("Hi Brian,\nCould you confirm?", "owned", false, false),
        ("Hi, could you confirm? No need to reply.", "owned", false, false),
        ("Hi,\nCould you confirm?", "copied", false, false),
        ("Hi, could you confirm?", "group", false, false),
        ("Hi, could you confirm?", "alias", false, false),
        ("Hi,\nUser, could you confirm?", "group", true, false),
        ("Hi,\nFYI, this was handled.\n\nFrom: Alice <alice@example.com>\nSent: Monday\nTo: User <user@example.com>\nSubject: Old ask\n\nCould you confirm?", "owned", false, false),
    ] {
        let fx = Fixture::new().await;
        let mut message = inbound(&fx, "maya@team.example", false).await;
        match recipient {
            "copied" => { message.cc = message.to.clone(); message.to = vec![Address { name: None, email: "alice@team.example".into() }]; },
            "group" => message.to.push(Address { name: None, email: "alice@team.example".into() }),
            "alias" => message.to = vec![Address { name: None, email: "unknown-alias@example.com".into() }],
            _ => {},
        }
        for address in &mut message.to {
            if address.email == ME { address.name = Some("User".into()); }
        }
        fx.store_envelope(&message, MessageDirection::Inbound).await;
        cached_content(&fx, &message, "", Some(text)).await;
        let ResponseData::Desk { owed, people_new, .. } = fx.desk(100).await else { panic!("desk"); };
        assert_eq!(owed.rows.iter().chain(&people_new.rows).any(|row| row.thread_id == message.thread_id), expected && !action, "{recipient}: {text}");
        let ResponseData::Messages { messages } = request(&fx, Request::ListMessages { account_id: None, turn: None, filter: Default::default(), limit: 100 }).await else { panic!("Messages"); };
        assert_eq!(messages.your_turn.iter().flat_map(|row| &row.topics).any(|topic| topic.thread_id == message.thread_id), expected && !action, "{recipient}: {text}");
        let now = crate::handler::now::now_at(&fx.state, None, Utc::now(), &Utc).await.unwrap();
        assert_eq!(now.people.total, u32::from(expected && !action), "{recipient}: {text}");
        let answers = fx.state.store.asks_for_messages(&fx.account, std::slice::from_ref(&message.id)).await.unwrap();
        let answer = &answers[&message.id];
        assert_eq!(answer.units.iter().any(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou), action, "{recipient}: {text}");
        for unit in answer.units.iter().filter(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou) {
            assert!(text.contains(answer.quote(unit).unwrap()), "exact authored action quote");
        }
        let tasks = fx.state.store.list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100).await.unwrap();
        assert_eq!(tasks.iter().filter(|row| row.source_message_id.as_ref() == Some(&message.id)).count(), usize::from(action));
        let arrivals = line(&fx).await;
        if expected {
            assert!(list(&fx, &arrivals, ArrivalBucketData::Messages).await.contains(&message.id));
            assert_eq!(chip(&fx, &message.id).await.0, ArrivalBucketData::Messages);
            assert!(modes(&membership(&fx, &message.thread_id).await).contains(&ModeKindData::Messages));
        }
    }
}

#[tokio::test]
async fn unreferenced_quote_context_and_informational_placement_ignore_import_order() {
    let old_text = "User, could you confirm the launch date by Monday and send the signed copy?\nUser, could you also share the final reference with everyone before the Friday review meeting?";
    for reversed in [false, true] {
        let fx = Fixture::new().await;
        let template = inbound(&fx, "sam@team.example", false).await;
        let mut previous = template.clone();
        previous.id = MessageId::new();
        previous.provider_id = format!("previous-{}", previous.id);
        previous.message_id_header = Some(format!("<{}@team.example>", previous.id));
        previous.thread_id = ThreadId::new();
        previous.in_reply_to = None;
        previous.references.clear();
        previous.date = Utc::now() - Duration::minutes(30);
        // The source was copied context, not an unresolved ask to this owner.
        previous.to = vec![Address {
            name: None,
            email: "other@team.example".into(),
        }];
        previous.cc = vec![Address {
            name: None,
            email: ME.into(),
        }];
        let mut current = previous.clone();
        current.id = MessageId::new();
        current.provider_id = format!("current-{}", current.id);
        current.message_id_header = Some(format!("<{}@team.example>", current.id));
        current.date = previous.date + Duration::minutes(20);
        current.subject = "Weekly project report".into();
        current.to = vec![Address {
            name: Some("User".into()),
            email: ME.into(),
        }];
        current.to.extend((0..8).map(|i| Address {
            name: None,
            email: format!("colleague{i}@team.example"),
        }));
        current.cc.clear();
        let order = if reversed {
            [&current, &previous]
        } else {
            [&previous, &current]
        };
        for message in order {
            fx.store_envelope(message, MessageDirection::Inbound).await;
        }
        cached_content(&fx, &previous, "", Some(old_text)).await;
        cached_content(
            &fx,
            &current,
            "",
            Some(&format!(
                "FYI: this was handled. The report is attached.\n\n{old_text}"
            )),
        )
        .await;
        let desk = fx
            .state
            .store
            .desk_messages_in_threads(&fx.account, std::slice::from_ref(&current.thread_id))
            .await
            .unwrap();
        let observed = desk.iter().find(|m| m.id == current.id).unwrap();
        let place = fx
            .state
            .store
            .place_message(&current.id)
            .await
            .unwrap()
            .unwrap();
        assert!(!observed.evidence.is_reply && !observed.evidence.thread_correspondence);
        assert_eq!(observed.evidence.previous_text.as_deref(), Some(old_text));
        assert_eq!(observed.evidence, place.evidence);
        assert!(!crate::handler::mail_kind::needs_reply(
            observed,
            &|email| email == ME
        ));
        let ResponseData::MessageKind { mail_kind, .. } = request(
            &fx,
            Request::GetMessageKind {
                message_id: current.id.clone(),
            },
        )
        .await
        else {
            panic!("why");
        };
        assert_eq!(mail_kind.kind, mxr_protocol::SenderKindData::PaperTrail);
        assert!(mail_kind.reason.contains("informational update"));
        let line = line(&fx).await;
        assert!(list(&fx, &line, ArrivalBucketData::Updates)
            .await
            .contains(&current.id));
        assert_eq!(chip(&fx, &current.id).await.0, ArrivalBucketData::Updates);
        assert_eq!(
            modes(&membership(&fx, &current.thread_id).await),
            [ModeKindData::Updates]
        );
        let ResponseData::Desk {
            owed, people_new, ..
        } = fx.desk(100).await
        else {
            panic!("desk");
        };
        assert!(!owed
            .rows
            .iter()
            .chain(&people_new.rows)
            .any(|r| r.thread_id == current.thread_id));
        let ResponseData::Freshness { freshness } = request(
            &fx,
            Request::GetFreshness {
                account_id: None,
                limit: Some(20),
            },
        )
        .await
        else {
            panic!("freshness");
        };
        let arrival = freshness
            .arrivals
            .iter()
            .find(|a| a.message_id == current.id)
            .unwrap();
        assert_eq!(arrival.modes[0].mode, ModeKindData::Updates);
        assert!(arrival.modes[0].reason.contains("informational update"));
    }
}

#[tokio::test]
async fn message_purpose_and_attention_agree_without_rewriting_history() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "support@studio.example").await;
    wrote_to(&fx, "editor@weekly.example").await;
    wrote_to(&fx, "emea_invoicing@email.vendor.example").await;
    wrote_to(&fx, "maya@team.example").await;
    let newsletter = inbound(&fx, "editor@weekly.example", false).await;
    // Simulate an already-placed row from the old policy before headers arrive.
    line(&fx).await;
    let original = fx
        .state
        .store
        .arrivals_by_ids(std::slice::from_ref(&newsletter.id))
        .await
        .unwrap()
        .remove(0);
    cached_content(&fx, &newsletter, "LiSt-Id: Weekly\r\n <weekly.example.org>\r\nList-Unsubscribe: <https://weekly.example.org/unsub>\r\n", Some("This week's essays")).await;
    let receipt = inbound(&fx, "support@studio.example", false).await;
    cached_content(
        &fx,
        &receipt,
        "Auto-Submitted: auto-generated\r\n",
        Some("Invoice INV-2048 is available."),
    )
    .await;
    let discussion = inbound(&fx, "support@studio.example", false).await;
    cached_content(
        &fx,
        &discussion,
        "In-Reply-To: <question@example.org>\r\nX-SES-Outgoing: delivery\r\n",
        Some("Could you confirm the invoice reference?\nInvoice Number:AB12345\nDate of Issue:14.10.2026\n\nDear User,\nThank you for your recent purchase from Vendor.\nWeb order number: W12345678\nPlease find attached your billing document AB12345."),
    )
    .await;
    let mut invoice = inbound(&fx, "emea_invoicing@email.vendor.example", false).await;
    invoice.subject = "Your invoice Number AB12345".into();
    fx.store_envelope(&invoice, MessageDirection::Inbound).await;
    cached_content(&fx, &invoice, "", Some("Invoice Number:AB12345\nDate of Issue:14.10.2026\n\nDear User,\nThank you for your recent purchase from Vendor.\nWeb order number: W12345678\nPlease find attached your billing document AB12345.")).await;
    let mut report = inbound(&fx, "maya@team.example", false).await;
    report.subject = "Weekly project report".into();
    report.to.extend((0..8).map(|i| Address {
        name: None,
        email: format!("colleague{i}@team.example"),
    }));
    fx.store_envelope(&report, MessageDirection::Inbound).await;
    let mut decision = inbound(&fx, "maya@team.example", false).await;
    decision.subject = "Weekly project report".into();
    decision.to = report.to.clone();
    decision
        .to
        .iter_mut()
        .find(|address| address.email == ME)
        .unwrap()
        .name = Some("User".into());
    fx.store_envelope(&decision, MessageDirection::Inbound)
        .await;
    cached_content(
        &fx,
        &decision,
        "",
        Some("User, could you confirm the next step? The report is attached."),
    )
    .await;
    let recruiter = inbound(&fx, "recruiter@ats.example", false).await;
    cached_content(&fx, &recruiter,
        "In-Reply-To: <interview@example.org>\r\nList-Unsubscribe: <https://ats.example/optout>\r\nX-SES-Outgoing: delivery\r\n",
        Some("Could you confirm the interview time?")).await;
    cached_content(
        &fx,
        &report,
        "",
        Some("FYI: the weekly team report is attached."),
    )
    .await;
    let acknowledgement = inbound(&fx, "sam@team.example", false).await;
    cached_content(
        &fx,
        &acknowledgement,
        "",
        Some("Thanks, received. Reference INV-2048.\n\n> Could you reply?"),
    )
    .await;
    let unknown = mail(
        &fx,
        &ThreadId::new(),
        "new@studio.example",
        &["unknown-alias@example.com"],
        &[],
        false,
        Duration::minutes(2),
    )
    .await;
    cached_content(&fx, &unknown, "", Some("Could you confirm the meeting?")).await;
    let negative = inbound(&fx, "ruth@team.example", false).await;
    cached_content(
        &fx,
        &negative,
        "",
        Some("Please do not reply; no need to confirm."),
    )
    .await;

    for _ in 0..2 {
        let now = line(&fx).await;
        for (mail, bucket) in [
            (&newsletter, ArrivalBucketData::Reading),
            (&receipt, ArrivalBucketData::Updates),
            (&discussion, ArrivalBucketData::Messages),
            (&decision, ArrivalBucketData::Messages),
            (&recruiter, ArrivalBucketData::Messages),
            (&invoice, ArrivalBucketData::Updates),
            (&report, ArrivalBucketData::Updates),
            (&acknowledgement, ArrivalBucketData::Messages),
            (&unknown, ArrivalBucketData::Messages),
            (&negative, ArrivalBucketData::Messages),
        ] {
            assert_eq!(chip(&fx, &mail.id).await.0, bucket);
            assert!(list(&fx, &now, bucket).await.contains(&mail.id));
            let held = membership(&fx, &mail.thread_id).await;
            assert!(modes(&held).contains(&bucket.mode().unwrap()), "{held:?}");
        }
        let ResponseData::Desk {
            owed, people_new, ..
        } = fx.desk(100).await
        else {
            panic!("desk");
        };
        let turns: Vec<_> = owed
            .rows
            .iter()
            .chain(&people_new.rows)
            .map(|r| &r.thread_id)
            .collect();
        assert!(turns.contains(&&discussion.thread_id));
        assert!(turns.contains(&&decision.thread_id));
        assert!(turns.contains(&&recruiter.thread_id));
        for quiet in [
            &newsletter,
            &receipt,
            &report,
            &invoice,
            &acknowledgement,
            &unknown,
            &negative,
        ] {
            assert!(
                !turns.contains(&&quiet.thread_id),
                "false debt for {}",
                quiet.from.email
            );
        }
    }
    let stored = fx
        .state
        .store
        .arrivals_by_ids(std::slice::from_ref(&newsletter.id))
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        (stored.first_seen_at, stored.mode, stored.moved_at),
        (original.first_seen_at, original.mode, original.moved_at)
    );
    let corrections: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mode_corrections")
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap();
    assert_eq!(corrections, 0);
    fx.state
        .store
        .set_message_labels(&newsletter.id, &[], mxr_core::EventSource::User)
        .await
        .unwrap();
    let ResponseData::Messages { messages } = request(
        &fx,
        Request::ListMessages {
            account_id: None,
            turn: None,
            filter: mxr_protocol::MessagesFilterData {
                turn: Some(mxr_protocol::MessagesTurnData::All),
                ..Default::default()
            },
            limit: 100,
        },
    )
    .await
    else {
        panic!("messages");
    };
    assert_eq!(messages.your_turn_split.total(), 3);
    let topics: Vec<_> = messages
        .your_turn
        .iter()
        .chain(&messages.pinned)
        .chain(&messages.recent)
        .chain(&messages.quiet)
        .flat_map(|row| &row.topics)
        .map(|topic| &topic.thread_id)
        .collect();
    assert!(
        !topics.contains(&&invoice.thread_id),
        "a document does not linger as Quiet in Messages"
    );
    assert!(
        !topics.contains(&&report.thread_id),
        "a team FYI does not linger as Quiet in Messages"
    );
    for (message, reason) in [
        (&invoice, "structured billing document"),
        (&report, "informational update"),
    ] {
        let ResponseData::MessageKind { mail_kind, .. } = request(
            &fx,
            Request::GetMessageKind {
                message_id: message.id.clone(),
            },
        )
        .await
        else {
            panic!("message why");
        };
        assert_eq!(mail_kind.kind, mxr_protocol::SenderKindData::PaperTrail);
        assert!(mail_kind.reason.contains(reason), "{}", mail_kind.reason);
        assert!(chip(&fx, &message.id).await.1.contains(reason));
        let held = membership(&fx, &message.thread_id).await;
        assert!(held
            .modes
            .iter()
            .any(|mode| mode.mode == ModeKindData::Updates && mode.reason.contains(reason)));
    }
    let now = crate::handler::now::now_at(&fx.state, None, Utc::now(), &Utc)
        .await
        .unwrap();
    assert_eq!(now.people.total, 3);
    let ResponseData::Freshness { freshness } = request(
        &fx,
        Request::GetFreshness {
            account_id: None,
            limit: Some(20),
        },
    )
    .await
    else {
        panic!("freshness");
    };
    for (id, mode) in [
        (&newsletter.id, ModeKindData::Reading),
        (&receipt.id, ModeKindData::Updates),
    ] {
        let arrival = freshness
            .arrivals
            .iter()
            .find(|arrival| &arrival.message_id == id)
            .expect("known arrival in Freshness");
        assert_eq!(arrival.modes[0].mode, mode);
        if id == &newsletter.id {
            assert!(!arrival.in_inbox);
        }
    }
    assert!(fx.fake.sent_drafts().is_empty());
}

/// You once wrote to `email`, so the contacts table knows it.
async fn wrote_to(fx: &Fixture, email: &str) {
    mail(
        fx,
        &ThreadId::new(),
        ME,
        &[email],
        &[],
        false,
        Duration::days(20),
    )
    .await;
    fx.state.store.refresh_contacts().await.unwrap();
}

/// The line now, with its window starting an hour ago.
async fn line(fx: &Fixture) -> ArrivalsData {
    let now = Utc::now();
    fx.state
        .store
        .set_mode_viewed("arrivals:since", now - Duration::hours(1))
        .await
        .unwrap();
    arrivals_at(&fx.state, None, false, None, now, &Utc)
        .await
        .unwrap()
}

fn count(line: &ArrivalsData, bucket: ArrivalBucketData) -> u32 {
    line.counts
        .iter()
        .find(|c| c.bucket == bucket)
        .map_or(0, |c| c.count)
}

async fn list(fx: &Fixture, line: &ArrivalsData, bucket: ArrivalBucketData) -> Vec<MessageId> {
    match request(
        fx,
        Request::ListArrivals {
            account_id: None,
            bucket: Some(bucket),
            since: Some(line.since),
            until: Some(line.until),
            limit: 1000,
        },
    )
    .await
    {
        ResponseData::ArrivalList { list } => {
            assert_eq!(list.total as usize, list.items.len());
            list.items.into_iter().map(|item| item.message_id).collect()
        }
        other => panic!("expected an arrival list, got {other:?}"),
    }
}

async fn move_to(
    fx: &Fixture,
    message: &MessageId,
    mode: ModeKindData,
    sender: bool,
) -> MoveOutcomeData {
    move_with(fx, message, mode, sender, None).await
}

async fn move_with(
    fx: &Fixture,
    message: &MessageId,
    mode: ModeKindData,
    sender: bool,
    source: Option<&str>,
) -> MoveOutcomeData {
    match request(
        fx,
        Request::MoveMessage {
            message_id: message.clone(),
            mode,
            sender,
            dry_run: false,
            source: source.map(str::to_string),
        },
    )
    .await
    {
        ResponseData::MessageMoved { outcome } => outcome,
        other => panic!("expected a move, got {other:?}"),
    }
}

async fn undo(fx: &Fixture, correction: i64) -> String {
    match request(
        fx,
        Request::UndoMove {
            correction_id: correction,
        },
    )
    .await
    {
        ResponseData::MoveUndone { copy, .. } => copy,
        other => panic!("expected an undo, got {other:?}"),
    }
}

async fn chip(fx: &Fixture, message: &MessageId) -> (ArrivalBucketData, String) {
    match request(
        fx,
        Request::GetArrivalModes {
            message_ids: vec![message.clone()],
        },
    )
    .await
    {
        ResponseData::ArrivalModes { mut items } => {
            let item = items.remove(0);
            (item.bucket, item.chip)
        }
        other => panic!("expected arrival modes, got {other:?}"),
    }
}

async fn membership(fx: &Fixture, thread: &ThreadId) -> ThreadModesData {
    match request(
        fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(thread.clone()),
            thread_ids: Vec::new(),
        },
    )
    .await
    {
        ResponseData::ModeMembership { mut threads } => threads.remove(0),
        other => panic!("expected membership, got {other:?}"),
    }
}

fn modes(data: &ThreadModesData) -> Vec<ModeKindData> {
    data.modes.iter().map(|entry| entry.mode).collect()
}

#[tokio::test]
async fn the_line_sums_with_spam_aspects_and_mail_archived_after_it_arrived() {
    let fx = Fixture::new().await;
    let person = inbound(&fx, "maya@orbit.example", false).await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    let mut spam = inbound(&fx, "prize@win.example", false).await;
    spam.flags = MessageFlags::SPAM;
    fx.store_envelope(&spam, MessageDirection::Inbound).await;
    // A to-do from Maya's email: also in To do, never added to the sum.
    request(
        &fx,
        Request::CreateTodo {
            message_id: person.id.clone(),
            title: "Send the deck".into(),
            kind: None,
            due: None,
            time_zone: None,
            dry_run: false,
        },
    )
    .await;

    let first = line(&fx).await;
    // Archived after it arrived: still counted where it went.
    fx.state
        .store
        .set_message_labels(&newsletter.id, &[], mxr_core::types::EventSource::User)
        .await
        .unwrap();
    let after = line(&fx).await;
    for line in [&first, &after] {
        assert_eq!(line.total, 4);
        assert_eq!(line.counts.iter().map(|c| c.count).sum::<u32>(), line.total);
        assert_eq!(count(line, ArrivalBucketData::Messages), 1);
        assert_eq!(count(line, ArrivalBucketData::Reading), 1);
        assert_eq!(count(line, ArrivalBucketData::Updates), 1);
        assert_eq!(count(line, ArrivalBucketData::Spam), 1);
        assert_eq!(count(line, ArrivalBucketData::Sorting), 0);
        assert_eq!(line.also.len(), 1, "{:?}", line.also);
        assert_eq!(line.also[0].bucket, ArrivalBucketData::Todo);
        assert!(
            line.counts.iter().all(|c| c.count > 0),
            "zero counts are left out"
        );
        for c in &line.counts {
            assert_eq!(list(&fx, line, c.bucket).await.len() as u32, c.count);
        }
        assert_eq!(
            list(&fx, line, ArrivalBucketData::Todo).await,
            vec![person.id.clone()]
        );
    }
    assert!(after
        .line
        .starts_with(&format!("Since {}: 4 arrived. ", after.since_label)));
    assert!(after
        .line
        .contains("1 Messages · 1 Updates · 1 Reading · 1 spam."));
    assert!(after.line.ends_with("Also 1 in To do."));
    assert_eq!(
        after.clear_line.as_deref(),
        Some(
            format!(
                "Clear. All 4 emails since {} are accounted for.",
                after.since_label
            )
            .as_str()
        )
    );
    assert_eq!(
        list(&fx, &after, ArrivalBucketData::Reading).await,
        vec![newsletter.id]
    );
    assert_eq!(
        list(&fx, &after, ArrivalBucketData::Updates).await,
        vec![receipt.id]
    );
}

#[tokio::test]
async fn nothing_new_says_when_mail_last_came() {
    let fx = Fixture::new().await;
    inbound(&fx, "maya@orbit.example", false).await;
    let now = Utc::now();
    // The window opens after the mail arrived.
    fx.state
        .store
        .set_mode_viewed("arrivals:since", now + Duration::seconds(2))
        .await
        .unwrap();
    let line = arrivals_at(
        &fx.state,
        None,
        false,
        None,
        now + Duration::seconds(5),
        &Utc,
    )
    .await
    .unwrap();
    assert_eq!(line.total, 0);
    assert!(line.line.starts_with("Nothing new since "), "{}", line.line);
    assert!(line.line.contains("Latest mail"), "{}", line.line);
    assert!(line.clear_line.is_none());
}

#[tokio::test]
async fn the_window_runs_from_the_visit_before_and_a_quick_return_keeps_it() {
    let fx = Fixture::new().await;
    let tz = Utc;
    let morning = Utc.with_ymd_and_hms(2026, 10, 7, 8, 12, 0).unwrap();
    // Never opened: the start of today.
    let first = arrivals_at(&fx.state, None, true, None, morning, &tz)
        .await
        .unwrap();
    assert_eq!(first.since_label, "00:00");
    // The next visit counts from 08:12.
    let noon = morning + Duration::hours(4);
    let second = arrivals_at(&fx.state, None, true, None, noon, &tz)
        .await
        .unwrap();
    assert_eq!(second.since_label, "08:12");
    // Back within a minute (a double mount, a quick look elsewhere): the
    // same visit.
    let again = arrivals_at(
        &fx.state,
        None,
        true,
        None,
        noon + Duration::seconds(20),
        &tz,
    )
    .await
    .unwrap();
    assert_eq!(again.since_label, "08:12");
    // Never more than 24 hours back.
    let days_later = noon + Duration::days(3);
    let late = arrivals_at(&fx.state, None, true, None, days_later, &tz)
        .await
        .unwrap();
    assert_eq!(late.since, days_later - Duration::hours(24));
    // Reading without marking (the CLI, MCP) starts no visit, and shows the
    // window an open Now would show then: within the visit's minute the
    // one it opened with, once it has lapsed the visit itself, so the
    // line never reads smaller after the click than before it.
    let within = arrivals_at(
        &fx.state,
        None,
        false,
        None,
        days_later + Duration::seconds(30),
        &tz,
    )
    .await
    .unwrap();
    // Still the visit's own window, clamped to 24 hours before now.
    assert_eq!(
        within.since,
        days_later + Duration::seconds(30) - Duration::hours(24)
    );
    let lapsed_at = days_later + Duration::hours(1);
    let peek = arrivals_at(&fx.state, None, false, None, lapsed_at, &tz)
        .await
        .unwrap();
    assert_eq!(peek.since, days_later);
    let opened = arrivals_at(&fx.state, None, true, None, lapsed_at, &tz)
        .await
        .unwrap();
    assert_eq!(opened.since, peek.since);
}

#[tokio::test]
async fn list_mail_keeps_its_purpose_even_from_someone_you_wrote_to() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    let known = inbound(&fx, "maya@orbit.example", true).await;
    let stranger = inbound(&fx, "editor@weekly.example", true).await;
    // Only copied: not covered, so Now asks about it.
    let copied_thread = ThreadId::new();
    let copied = mail(
        &fx,
        &copied_thread,
        "maya@orbit.example",
        &["ruth@keystone.example"],
        &[ME],
        false,
        Duration::minutes(3),
    )
    .await;

    let line = line(&fx).await;
    assert_eq!(
        list(&fx, &line, ArrivalBucketData::Messages).await,
        Vec::<MessageId>::new()
    );
    let reading = list(&fx, &line, ArrivalBucketData::Reading).await;
    assert!(reading.contains(&known.id) && reading.contains(&stranger.id));
    assert_eq!(
        list(&fx, &line, ArrivalBucketData::Updates).await,
        vec![copied.id.clone()]
    );
    let (_, known_chip) = chip(&fx, &known.id).await;
    assert_eq!(known_chip, "→ Reading · has List-Unsubscribe");

    // Live membership agrees: the known sender's list mail is in Messages.
    let held = membership(&fx, &known.thread_id).await;
    assert!(!modes(&held).contains(&ModeKindData::Messages), "{held:?}");
    assert!(modes(&held).contains(&ModeKindData::Reading), "{held:?}");

    assert_eq!(line.not_sure.len(), 1);
    assert_eq!(line.not_sure[0].message_id, copied.id);
    assert_eq!(line.not_sure[0].mode, ModeKindData::Updates);
    assert!(line.not_sure[0].line.contains("copied you on"));
    assert_eq!(
        line.not_sure_line.as_deref(),
        Some("1 email I wasn't sure about. Where should it go?")
    );
    assert!(line.not_sure_hint.is_some());

    // Placements record the rule.
    let rule: String = sqlx::query_scalar("SELECT rule FROM arrivals WHERE message_id = ?1")
        .bind(known.id.as_str())
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(KindRuleData::ListUnsubscribe).unwrap(),
        serde_json::Value::String(rule)
    );
}

#[tokio::test]
async fn a_move_takes_effect_everywhere_at_once_and_undo_puts_it_back() {
    let fx = Fixture::new().await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    let mut events = fx.state.event_tx.subscribe();
    let before = line(&fx).await;
    assert_eq!(count(&before, ArrivalBucketData::Reading), 1);

    let moved = move_to(&fx, &newsletter.id, ModeKindData::Messages, false).await;
    assert_eq!(moved.from, ArrivalBucketData::Reading);
    assert_eq!(moved.copy, "Moved to Messages.");
    assert_eq!(
        moved.ask_sender.as_deref(),
        Some("Always for this sender? (P)")
    );
    assert!(moved.hint.is_some());
    let correction = moved.correction_id.expect("a correction to undo");
    let mut changed = false;
    while let Ok(message) = events.try_recv() {
        changed |= matches!(
            message.payload,
            IpcPayload::Event(DaemonEvent::ModesChanged { .. })
        );
    }
    assert!(changed, "every client is told");

    let after = line(&fx).await;
    assert_eq!(count(&after, ArrivalBucketData::Messages), 1);
    assert_eq!(count(&after, ArrivalBucketData::Reading), 0);
    assert_eq!(after.total, before.total);
    let held = membership(&fx, &newsletter.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Messages), "{held:?}");
    assert!(!modes(&held).contains(&ModeKindData::Reading), "{held:?}");
    assert_eq!(
        chip(&fx, &newsletter.id).await,
        (
            ArrivalBucketData::Messages,
            "→ Messages · you moved this email".to_string()
        )
    );
    assert!(after
        .track_record
        .as_deref()
        .unwrap()
        .ends_with("you moved 1."));

    assert_eq!(undo(&fx, correction).await, "Moved back to Reading.");
    assert_eq!(undo(&fx, correction).await, "Already undone.");
    let back = line(&fx).await;
    assert_eq!(count(&back, ArrivalBucketData::Reading), 1);
    assert!(back.track_record.is_none(), "an undone move is no move");
    let held = membership(&fx, &newsletter.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Reading), "{held:?}");
}

#[tokio::test]
async fn a_sender_move_beats_earlier_email_moves_and_a_later_email_move_beats_it() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    let b = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;

    // X on A only.
    move_to(&fx, &a.id, ModeKindData::Messages, false).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);

    // K: the sender's mode wins over A's earlier move, in the same second.
    let sender = move_to(&fx, &b.id, ModeKindData::Updates, true).await;
    assert_eq!(
        sender.copy,
        "All mail from editor@weekly.example goes to Updates."
    );
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Updates);

    // X after K: this email only.
    let later = move_to(&fx, &b.id, ModeKindData::Reading, false).await;
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);

    // New mail from the sender follows K.
    let c = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    assert_eq!(chip(&fx, &c.id).await.0, ArrivalBucketData::Updates);

    // Undo K: A's move stands again; B's later move still stands.
    undo(&fx, sender.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);
    assert_eq!(chip(&fx, &c.id).await.0, ArrivalBucketData::Reading);
    undo(&fx, later.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);
}

#[tokio::test]
async fn a_sender_mode_set_elsewhere_is_logged_and_its_quick_reversal_is_an_undo() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let set = |kind| Request::SetSenderKind {
        account_id: fx.account.clone(),
        sender_email: "editor@weekly.example".into(),
        kind,
    };
    request(&fx, set(Some(mxr_protocol::SenderKindData::People))).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    let corrections = fx
        .state
        .store
        .list_corrections(std::slice::from_ref(&fx.account), 10)
        .await
        .unwrap();
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].fields.from_mode, "auto");
    assert_eq!(corrections[0].fields.to_mode, "messages");
    // The client's undo puts the old kind back: the move is undone, not a
    // second move.
    request(&fx, set(None)).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Reading);
    assert!(!fx
        .state
        .store
        .has_moves(std::slice::from_ref(&fx.account))
        .await
        .unwrap());
}

#[tokio::test]
async fn to_do_and_archive_add_the_email_there_and_undo_takes_it_back() {
    let fx = Fixture::new().await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    line(&fx).await;
    let todo = move_to(&fx, &receipt.id, ModeKindData::Todo, false).await;
    assert_eq!(todo.copy, "Added to To do.");
    assert!(todo.ask_sender.is_none());
    let todo_id = todo.aspect_id.clone().expect("the to-do made");
    let after = line(&fx).await;
    assert_eq!(
        count(&after, ArrivalBucketData::Updates),
        1,
        "To do is an aspect"
    );
    assert_eq!(after.also[0].bucket, ArrivalBucketData::Todo);
    undo(&fx, todo.correction_id.unwrap()).await;
    let row = fx.state.store.get_todo(&todo_id).await.unwrap().unwrap();
    assert_eq!(row.state, mxr_store::TodoState::Dismissed);
    assert!(line(&fx).await.also.is_empty());
}

#[tokio::test]
async fn a_dry_run_changes_nothing_and_a_sender_move_needs_a_kind_mode() {
    let fx = Fixture::new().await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let preview = match request(
        &fx,
        Request::MoveMessage {
            message_id: newsletter.id.clone(),
            mode: ModeKindData::Updates,
            sender: false,
            dry_run: true,
            source: None,
        },
    )
    .await
    {
        ResponseData::MessageMoved { outcome } => outcome,
        other => panic!("{other:?}"),
    };
    assert_eq!(preview.copy, "Would move to Updates.");
    assert!(preview.correction_id.is_none());
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Reading
    );

    let msg = IpcMessage {
        expected_workspace: None,
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::MoveMessage {
            message_id: newsletter.id.clone(),
            mode: ModeKindData::Todo,
            sender: true,
            dry_run: false,
            source: None,
        }),
    };
    assert!(matches!(
        handle_request(&fx.state, &msg).await.payload,
        IpcPayload::Response(Response::Error { .. })
    ));
    // Moving twice: the second move stands, and undoing it returns to the
    // first, not to where it arrived.
    let first = move_to(&fx, &newsletter.id, ModeKindData::Updates, false).await;
    let second = move_to(&fx, &newsletter.id, ModeKindData::Messages, false).await;
    assert_eq!(second.from, ArrivalBucketData::Updates);
    undo(&fx, second.correction_id.unwrap()).await;
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Updates
    );
    undo(&fx, first.correction_id.unwrap()).await;
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Reading
    );
}

#[tokio::test]
async fn not_sure_asks_at_most_three_a_day_by_the_local_day() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    let mut ids = Vec::new();
    for _ in 0..5 {
        let copied = mail(
            &fx,
            &ThreadId::new(),
            "maya@orbit.example",
            &["ruth@keystone.example"],
            &[ME],
            false,
            Duration::minutes(3),
        )
        .await;
        ids.push(copied.id);
    }
    // In UTC+10, the first email arrived at 23:30 yesterday and the rest
    // after midnight today.
    let tz = FixedOffset::east_opt(10 * 3600).unwrap();
    let midnight = tz
        .with_ymd_and_hms(2026, 10, 8, 0, 0, 0)
        .unwrap()
        .with_timezone(&Utc);
    for (i, id) in ids.iter().enumerate() {
        let at = if i == 0 {
            midnight - Duration::minutes(30)
        } else {
            midnight + Duration::minutes(10 * i as i64)
        };
        sqlx::query("UPDATE arrivals SET first_seen_at = ?2 WHERE message_id = ?1")
            .bind(id.as_str())
            .bind(at.timestamp())
            .execute(fx.state.store.writer())
            .await
            .unwrap();
    }
    let now = midnight + Duration::hours(2);
    // Place them first.
    arrivals_at(&fx.state, None, false, None, now, &tz)
        .await
        .unwrap();
    let asked = arrivals_at(&fx.state, None, false, None, now, &tz)
        .await
        .unwrap();
    let shown: Vec<MessageId> = asked
        .not_sure
        .iter()
        .map(|q| q.message_id.clone())
        .collect();
    assert_eq!(
        shown,
        ids[1..4].to_vec(),
        "today's first three, in arrival order"
    );

    // Answering one never pulls the fourth in.
    let kept = move_with(&fx, &ids[1], ModeKindData::Updates, false, Some("not_sure")).await;
    assert_eq!(kept.copy, "Kept in Updates.");
    assert!(kept.correction_id.is_some(), "keeping it is an answer");
    let after = arrivals_at(&fx.state, None, false, None, now, &tz)
        .await
        .unwrap();
    let shown: Vec<MessageId> = after
        .not_sure
        .iter()
        .map(|q| q.message_id.clone())
        .collect();
    assert_eq!(shown, ids[2..4].to_vec());
    assert_eq!(
        after.not_sure_line.as_deref(),
        Some("2 emails I wasn't sure about. Where should these go?")
    );
    // Keeping an email where it was is not a move for the track record.
    assert!(after.track_record.is_none());
    // A day later UTC is still the 8th, but it's the 9th in UTC+10.
    let next_day = arrivals_at(&fx.state, None, false, None, now + Duration::days(1), &tz)
        .await
        .unwrap();
    assert!(next_day.not_sure.is_empty());
}

#[tokio::test]
async fn placement_does_not_depend_on_the_order_mail_was_stored() {
    let first = Fixture::new().await;
    let second = Fixture::new().await;
    let senders = [
        ("maya@orbit.example", false),
        ("editor@weekly.example", true),
        ("no-reply@shop.example", false),
    ];
    for (from, list) in senders {
        inbound(&first, from, list).await;
    }
    for (from, list) in senders.iter().rev() {
        inbound(&second, from, *list).await;
    }
    let a = line(&first).await;
    let b = line(&second).await;
    assert_eq!(a.counts, b.counts);
    assert_eq!(a.total, 3);
}

#[tokio::test]
async fn a_message_stored_mid_correction_is_placed_by_the_rules_and_the_move_stands() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let first = mail(
        &fx,
        &thread,
        "editor@weekly.example",
        &[ME],
        &[],
        true,
        Duration::minutes(10),
    )
    .await;
    line(&fx).await;
    move_to(&fx, &first.id, ModeKindData::Messages, false).await;
    // Sync stores the next issue in the same thread.
    let next = mail(
        &fx,
        &thread,
        "editor@weekly.example",
        &[ME],
        &[],
        true,
        Duration::minutes(1),
    )
    .await;
    let placed = line(&fx).await;
    assert_eq!(chip(&fx, &first.id).await.0, ArrivalBucketData::Messages);
    assert_eq!(chip(&fx, &next.id).await.0, ArrivalBucketData::Reading);
    assert_eq!(placed.total, 2);
}

#[tokio::test]
async fn each_account_counts_its_own_mail() {
    let fx = Fixture::new().await;
    inbound(&fx, "maya@orbit.example", false).await;
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Work".into(),
        email: "me@work.example".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&other).await.unwrap();
    let mut envelope = fx
        .message(
            &ThreadId::new(),
            "lead@work.example",
            ME,
            Duration::minutes(2),
            None,
        )
        .await;
    envelope.account_id = other.id.clone();
    envelope.id = MessageId::new();
    envelope.provider_id = "work-1".into();
    fx.state
        .store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    let all = line(&fx).await;
    assert_eq!(all.total, 3, "two in the default account, one at work");
    let now = Utc::now();
    fx.state
        .store
        .set_mode_viewed(
            &format!("arrivals:{}:since", other.id),
            now - Duration::hours(1),
        )
        .await
        .unwrap();
    let work = arrivals_at(&fx.state, Some(&other.id), false, None, now, &Utc)
        .await
        .unwrap();
    assert_eq!(work.total, 1);
}

/// The error text of a request the daemon refuses.
async fn refused(fx: &Fixture, request: Request) -> String {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_open_now_keeps_its_window_while_it_polls() {
    let fx = Fixture::new().await;
    let tz = Utc;
    let morning = Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap();
    arrivals_at(&fx.state, None, true, None, morning, &tz)
        .await
        .unwrap();
    let noon = morning + Duration::hours(4);
    let opened = arrivals_at(&fx.state, None, true, None, noon, &tz)
        .await
        .unwrap();
    assert_eq!(opened.since, morning);
    // A minute-plus later the open page refetches without marking and
    // names the window it holds: it must not jump to the visit itself.
    let later = noon + Duration::minutes(5);
    let polled = arrivals_at(&fx.state, None, false, Some(opened.since), later, &tz)
        .await
        .unwrap();
    assert_eq!(polled.since, opened.since);
    // A peek that holds no window (the CLI) still answers what Now would
    // open with.
    let peek = arrivals_at(&fx.state, None, false, None, later, &tz)
        .await
        .unwrap();
    assert_eq!(peek.since, noon);
}

#[tokio::test]
async fn undoing_an_older_move_is_refused_while_a_newer_one_stands() {
    let fx = Fixture::new().await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let first = move_to(&fx, &newsletter.id, ModeKindData::Updates, false).await;
    let second = move_to(&fx, &newsletter.id, ModeKindData::Messages, false).await;
    let message = refused(
        &fx,
        Request::UndoMove {
            correction_id: first.correction_id.unwrap(),
        },
    )
    .await;
    assert!(message.contains("newer move"), "{message}");
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Messages,
        "the newer move is untouched"
    );
    // Newest first works, and then the older one is free to undo.
    undo(&fx, second.correction_id.unwrap()).await;
    undo(&fx, first.correction_id.unwrap()).await;
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Reading
    );
}

#[tokio::test]
async fn undoing_a_sender_move_leaves_earlier_email_moves_standing() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    // The sender was sent to Reading some time ago; then A was moved to
    // Messages by hand.
    let long_ago = Utc::now() - Duration::hours(3);
    request(
        &fx,
        Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: "editor@weekly.example".into(),
            kind: Some(mxr_protocol::SenderKindData::Reading),
        },
    )
    .await;
    fx.state
        .store
        .set_screener_decided_at(&fx.account, "editor@weekly.example", long_ago)
        .await
        .unwrap();
    move_to(&fx, &a.id, ModeKindData::Messages, false).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);

    // K to Updates overrides A's move; undoing K restores Reading for the
    // sender with its old date, so A's move stands again.
    let sender = move_to(&fx, &a.id, ModeKindData::Updates, true).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    undo(&fx, sender.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    let decision = fx
        .state
        .store
        .get_screener_decision(&fx.account, "editor@weekly.example")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(decision.decided_at.timestamp(), long_ago.timestamp());
}

#[tokio::test]
async fn reversing_a_sender_mode_set_elsewhere_keeps_earlier_email_moves() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let set = |kind| Request::SetSenderKind {
        account_id: fx.account.clone(),
        sender_email: "editor@weekly.example".into(),
        kind,
    };
    request(&fx, set(Some(mxr_protocol::SenderKindData::Reading))).await;
    let long_ago = Utc::now() - Duration::hours(3);
    fx.state
        .store
        .set_screener_decided_at(&fx.account, "editor@weekly.example", long_ago)
        .await
        .unwrap();
    move_to(&fx, &a.id, ModeKindData::Messages, false).await;
    request(&fx, set(Some(mxr_protocol::SenderKindData::PaperTrail))).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    // The quick reversal is an undo: Reading is back, A's move stands.
    request(&fx, set(Some(mxr_protocol::SenderKindData::Reading))).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
}

#[tokio::test]
async fn moving_to_archive_when_already_filed_records_no_undo_and_keeps_the_record() {
    let fx = Fixture::new().await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    line(&fx).await;
    let first = move_to(&fx, &receipt.id, ModeKindData::Archive, false).await;
    let record = first.aspect_id.clone().expect("the record made");
    let again = move_to(&fx, &receipt.id, ModeKindData::Archive, false).await;
    assert_eq!(again.copy, "Already in Archive.");
    assert!(again.correction_id.is_none() && again.aspect_id.is_none());
    // Nothing the repeat logged can dismiss the record; undoing the first
    // filing still does.
    let still = fx
        .state
        .store
        .get_archive_record(&record)
        .await
        .unwrap()
        .unwrap();
    assert!(still.dismissed_at.is_none());
    let corrections = fx
        .state
        .store
        .list_corrections(std::slice::from_ref(&fx.account), 10)
        .await
        .unwrap();
    assert_eq!(corrections.len(), 1);
}

#[tokio::test]
async fn moving_to_to_do_twice_makes_one_task() {
    let fx = Fixture::new().await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    line(&fx).await;
    let first = move_to(&fx, &receipt.id, ModeKindData::Todo, false).await;
    let todo = first.aspect_id.clone().expect("the to-do made");
    let again = move_to(&fx, &receipt.id, ModeKindData::Todo, false).await;
    assert_eq!(again.copy, "Already in To do.");
    assert!(again.correction_id.is_none() && again.aspect_id.is_none());
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM todos WHERE source_message_id = ?1 AND state = 'open'",
    )
    .bind(receipt.id.as_str())
    .fetch_one(fx.state.store.reader())
    .await
    .unwrap();
    assert_eq!(open, 1);
    // Undo of the one move still dismisses that task.
    undo(&fx, first.correction_id.unwrap()).await;
    let row = fx.state.store.get_todo(&todo).await.unwrap().unwrap();
    assert_eq!(row.state, mxr_store::TodoState::Dismissed);
}

#[tokio::test]
async fn known_senders_list_purpose_survives_copy_newsletter_address_and_crowd() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    wrote_to(&fx, "newsletter@orbit.example").await;
    // Newsletter-address rule, addressed to you.
    let by_address = inbound(&fx, "newsletter@orbit.example", true).await;
    // Crowd: more than ten recipients, addressed to you.
    let crowd: Vec<String> = (0..12).map(|i| format!("p{i}@team.example")).collect();
    let mut to: Vec<&str> = crowd.iter().map(String::as_str).collect();
    to.push(ME);
    let crowded = mail(
        &fx,
        &ThreadId::new(),
        "maya@orbit.example",
        &to,
        &[],
        true,
        Duration::minutes(4),
    )
    .await;
    // List mail with you only in Cc: not buried in Reading; Now asks.
    let cc_only = mail(
        &fx,
        &ThreadId::new(),
        "maya@orbit.example",
        &["ruth@keystone.example"],
        &[ME],
        true,
        Duration::minutes(3),
    )
    .await;
    let line = line(&fx).await;
    let messages = list(&fx, &line, ArrivalBucketData::Messages).await;
    assert!(messages.is_empty());
    let reading = list(&fx, &line, ArrivalBucketData::Reading).await;
    assert_eq!(reading.len(), 3);
    for message in [by_address, crowded, cc_only] {
        assert!(reading.contains(&message.id));
    }
    assert!(list(&fx, &line, ArrivalBucketData::Updates)
        .await
        .is_empty());
    assert!(line.not_sure.is_empty());
}

#[tokio::test]
async fn always_messages_for_a_sender_keeps_a_copied_email_in_messages() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    let copied = mail(
        &fx,
        &ThreadId::new(),
        "maya@orbit.example",
        &["ruth@keystone.example"],
        &[ME],
        false,
        Duration::minutes(3),
    )
    .await;
    line(&fx).await;
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Updates);
    move_to(&fx, &copied.id, ModeKindData::Messages, false).await;
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Messages);
    // K supersedes the per-email move; the sender's Messages setting must
    // hold the email there, as the preview and the toast promise.
    let preview = match request(
        &fx,
        Request::MoveMessage {
            message_id: copied.id.clone(),
            mode: ModeKindData::Messages,
            sender: true,
            dry_run: true,
            source: None,
        },
    )
    .await
    {
        ResponseData::MessageMoved { outcome } => outcome,
        other => panic!("expected a preview, got {other:?}"),
    };
    assert!(preview.copy.contains("Messages"));
    move_to(&fx, &copied.id, ModeKindData::Messages, true).await;
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Messages);
    let held = membership(&fx, &copied.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Messages), "{held:?}");
    assert!(!modes(&held).contains(&ModeKindData::Updates), "{held:?}");
}

#[tokio::test]
async fn undoing_an_older_sender_move_is_refused_while_a_newer_one_stands() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let to_updates = move_to(&fx, &a.id, ModeKindData::Updates, true).await;
    let to_messages = move_to(&fx, &a.id, ModeKindData::Messages, true).await;
    let message = refused(
        &fx,
        Request::UndoMove {
            correction_id: to_updates.correction_id.unwrap(),
        },
    )
    .await;
    assert!(message.contains("newer move"), "{message}");
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    undo(&fx, to_messages.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    undo(&fx, to_updates.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Reading);
}

#[tokio::test]
async fn assessment_body_replacement_changes_stored_home_and_every_projection() {
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "sam@studio.example", false).await;
    cached_content(&fx, &mail, "", Some("Thanks Sam. That works for me.")).await;
    membership(&fx, &mail.thread_id).await;
    let before = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .remove(&mail.id)
        .unwrap();
    assert_eq!(before.home, "messages");
    cached_content(
        &fx,
        &mail,
        "",
        Some("FYI: the report is ready. No action is required."),
    )
    .await;
    let held = membership(&fx, &mail.thread_id).await;
    let after = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .remove(&mail.id)
        .unwrap();
    assert_eq!(after.home, "updates");
    assert_ne!(after.input_fingerprint, before.input_fingerprint);
    assert_eq!(
        held.assessments.iter().find(|a| a.message_id == mail.id),
        Some(&after)
    );
    assert_eq!(chip(&fx, &mail.id).await.0, ArrivalBucketData::Updates);
    assert!(held
        .modes
        .iter()
        .any(|m| m.mode == ModeKindData::Updates && m.reason.contains(&after.deciding.reason)));
}

#[tokio::test]
async fn stale_assessment_cannot_overwrite_a_newer_body_decision() {
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "sam@studio.example", false).await;
    membership(&fx, &mail.thread_id).await;
    let revision = fx.state.store.assessment_input_revision().await.unwrap();
    let stale = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .remove(&mail.id)
        .unwrap();
    cached_content(
        &fx,
        &mail,
        "Auto-Submitted: auto-generated\r\n",
        Some("Your receipt is available."),
    )
    .await;
    membership(&fx, &mail.thread_id).await;
    assert!(!fx
        .state
        .store
        .save_assessment(&stale, revision)
        .await
        .unwrap());
    let current = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .remove(&mail.id)
        .unwrap();
    assert_eq!(current.home, "updates");
    assert_ne!(current.input_fingerprint, stale.input_fingerprint);
}

#[tokio::test]
async fn classifier_report_and_eval_do_not_materialize_missing_assessments() {
    let fx = Fixture::new().await;
    inbound(&fx, "stranger@studio.example", false).await;
    let revision = fx.state.store.change_feed().seq();
    let response = request(
        &fx,
        Request::ClassifyReport {
            account_id: Some(fx.account.clone()),
            limit: 20,
        },
    )
    .await;
    let ResponseData::ClassifyReport { report } = response else {
        panic!("classifier report");
    };
    assert_eq!(report.missing, 1);
    crate::handler::classify_eval::classify_eval(
        &fx.state,
        Some(&fx.account),
        mxr_protocol::ClassifyEvalSetData::Quick,
    )
    .await
    .unwrap();
    assert_eq!(fx.state.store.change_feed().seq(), revision);
    assert!(fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn read_only_reports_mark_changed_inputs_pending_without_invalidating_other_accounts() {
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "sam@studio.example", false).await;
    membership(&fx, &mail.thread_id).await;
    let mut other = fx
        .state
        .store
        .get_account(&fx.account)
        .await
        .unwrap()
        .unwrap();
    other.id = mxr_core::AccountId::new();
    other.email = "other@example.org".into();
    fx.state.store.insert_account(&other).await.unwrap();
    let read_report =
        || crate::handler::classifier_report::report(&fx.state, Some(&fx.account), 20);
    let ResponseData::ClassifyReport { report } = read_report().await.unwrap() else {
        panic!("report");
    };
    assert_eq!((report.missing, report.stale), (0, 0));
    request(
        &fx,
        Request::SetScreenerDecision {
            account_id: fx.account.clone(),
            sender_email: mail.from.email.clone(),
            disposition: mxr_protocol::ScreenerDispositionData::Feed,
            route_label: None,
        },
    )
    .await;
    let before = fx.state.store.change_feed().seq();
    let ResponseData::ClassifyReport { report } = read_report().await.unwrap() else {
        panic!("report");
    };
    assert_eq!((report.missing, report.stale), (0, 1));
    assert_eq!(fx.state.store.change_feed().seq(), before);
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    let ResponseData::ClassifyReport { report } = read_report().await.unwrap() else {
        panic!("report");
    };
    assert_eq!((report.missing, report.stale), (0, 0));
    let stored = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap();
    assert_eq!(stored.get(&mail.id).unwrap().home, "reading");
}

#[tokio::test]
async fn delivery_and_invite_share_the_updates_home_without_duplicate_parcel_rows() {
    let fx = Fixture::new().await;
    let delivery = inbound(&fx, "tracking@shop.example", false).await;
    let invite = inbound(&fx, "sam@studio.example", false).await;
    let delivery_id = mxr_core::DeliveryId::new();
    sqlx::query("INSERT INTO deliveries (id, account_id, dedup_key, status, source, thread_id, last_event_at, created_at, updated_at) VALUES (?, ?, 'synthetic-parcel', 'in_transit', 'heuristic', ?, ?, ?, ?)")
        .bind(delivery_id.as_str()).bind(fx.account.as_str()).bind(delivery.thread_id.as_str())
        .bind(Utc::now().timestamp()).bind(Utc::now().timestamp()).bind(Utc::now().timestamp())
        .execute(fx.state.store.writer()).await.unwrap();
    fx.state
        .store
        .link_delivery_message(
            &delivery_id,
            &delivery.id,
            Some(&delivery.thread_id),
            Some("shipped"),
            Utc::now(),
        )
        .await
        .unwrap();
    fx.state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: invite.id.clone(),
            text_plain: Some("Calendar invitation for tomorrow".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: mxr_core::MessageMetadata {
                calendar: Some(mxr_core::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning".into()),
                    uid: Some("synthetic-planning".into()),
                    starts_at: Some((Utc::now() + Duration::days(1)).to_rfc3339()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    for message in [&delivery, &invite] {
        let held = membership(&fx, &message.thread_id).await;
        assert!(held
            .modes
            .iter()
            .any(|mode| mode.mode == ModeKindData::Updates));
        assert_eq!(
            held.assessments
                .iter()
                .find(|a| a.message_id == message.id)
                .unwrap()
                .home,
            "updates"
        );
        assert_eq!(chip(&fx, &message.id).await.0, ArrivalBucketData::Updates);
    }
    let ResponseData::UpdatesDigest { digest } = request(
        &fx,
        Request::GetUpdatesDigest {
            account_id: Some(fx.account.clone()),
            cut: None,
            mark_seen: false,
            expired: false,
        },
    )
    .await
    else {
        panic!("updates digest");
    };
    let lines: Vec<_> = digest
        .needs_a_look
        .iter()
        .chain(&digest.changed)
        .chain(&digest.routine)
        .chain(&digest.since.lines)
        .collect();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.message_ids.contains(&delivery.id))
            .count(),
        1
    );
    assert!(lines
        .iter()
        .any(|line| line.message_ids.contains(&invite.id)));
    let before = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap();
    sqlx::query("UPDATE deliveries SET delivered_at = ?, status = 'delivered' WHERE id = ?")
        .bind((Utc::now() - Duration::days(2)).timestamp())
        .bind(delivery_id.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    assert!(!membership(&fx, &delivery.thread_id)
        .await
        .modes
        .iter()
        .any(|mode| mode.mode == ModeKindData::Updates));
    let after = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap();
    assert_eq!(before[&delivery.id].home, after[&delivery.id].home);
}

#[tokio::test]
async fn unknown_person_and_machine_get_named_fallbacks_and_observed_fact_clusters() {
    let fx = Fixture::new().await;
    let person = inbound(&fx, "stranger@studio.example", false).await;
    let machine = inbound(&fx, "no-reply@service.example", false).await;
    for message in [&person, &machine] {
        cached_content(&fx, message, "", None).await;
        membership(&fx, &message.thread_id).await;
    }
    let stored = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap();
    assert_eq!(stored[&person.id].home, "messages");
    assert_eq!(stored[&machine.id].home, "updates");
    for message in [&person, &machine] {
        let assessment = &stored[&message.id];
        assert!(assessment.fallback && assessment.pending_body);
        assert_eq!(assessment.not_sure.as_deref(), Some("fallback"));
        assert_eq!(assessment.deciding.rule, "fallback");
    }
    let ResponseData::ClassifyReport { report } = request(
        &fx,
        Request::ClassifyReport {
            account_id: Some(fx.account.clone()),
            limit: 20,
        },
    )
    .await
    else {
        panic!("report");
    };
    assert_eq!(report.fallback, 2);
    assert_eq!(report.clusters.len(), 2);
    assert_ne!(
        report.clusters[0].facts.sender_shape,
        report.clusters[1].facts.sender_shape
    );
}

#[tokio::test]
async fn policy_comparison_only_uses_retained_answers_and_never_invents_a_baseline() {
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "sam@studio.example", false).await;
    membership(&fx, &mail.thread_id).await;
    let request_for = |policy: &str| Request::ClassifyCompare {
        account_id: Some(fx.account.clone()),
        policy_version: policy.into(),
        limit: 20,
    };
    let ResponseData::ClassifyCompare { comparison } = request(&fx, request_for("pre-r2")).await
    else {
        panic!("comparison");
    };
    assert!(!comparison.available);
    assert!(comparison.changes.is_empty());
    let mut retained = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .remove(&mail.id)
        .unwrap();
    retained.policy_version = "retained-test-policy".into();
    retained.home = "updates".into();
    retained.input_fingerprint = "retained-runtime-answer".into();
    let revision = fx.state.store.assessment_input_revision().await.unwrap();
    assert!(fx
        .state
        .store
        .save_assessment(&retained, revision)
        .await
        .unwrap());
    let new_mail = inbound(&fx, "new@studio.example", false).await;
    membership(&fx, &new_mail.thread_id).await;
    let before = fx.state.store.change_feed().seq();
    let ResponseData::ClassifyCompare { comparison } =
        request(&fx, request_for("retained-test-policy")).await
    else {
        panic!("comparison");
    };
    assert!(comparison.available);
    assert_eq!(comparison.compared, 1);
    assert_eq!(comparison.missing_previous_assessments, 1);
    assert_eq!(comparison.changes.len(), 1);
    assert_eq!(comparison.changes[0].previous_home, "updates");
    assert_eq!(comparison.changes[0].current_home, "messages");
    assert_eq!(fx.state.store.change_feed().seq(), before);
}

#[tokio::test]
async fn copied_mail_projects_its_stored_home_and_reason_without_judging_again() {
    let fx = Fixture::new().await;
    let mail = mail(
        &fx,
        &ThreadId::new(),
        "sam@studio.example",
        &["ruth@studio.example"],
        &[ME],
        false,
        Duration::minutes(5),
    )
    .await;
    cached_content(&fx, &mail, "", None).await;
    membership(&fx, &mail.thread_id).await;
    let mut assessment = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .remove(&mail.id)
        .unwrap();
    assert_eq!(assessment.home, "updates");
    assert_eq!(assessment.deciding.rule, "copied");
    assert!(
        !assessment.fallback,
        "being copied is affirmative evidence, even without a body"
    );
    assessment.deciding.reason = "stored copied placement from the resolver".into();
    sqlx::query("UPDATE mail_assessments SET assessment_json = ? WHERE message_id = ? AND policy_version = ?")
        .bind(serde_json::to_string(&assessment).unwrap()).bind(mail.id.as_str())
        .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION).execute(fx.state.store.writer()).await.unwrap();
    let held = membership(&fx, &mail.thread_id).await;
    assert_eq!(modes(&held), vec![ModeKindData::Updates]);
    assert!(held.modes[0].reason.contains(&assessment.deciding.reason));
    let placed =
        crate::handler::places::placed_inbox(&fx.state, std::slice::from_ref(&fx.account), None)
            .await
            .unwrap();
    assert_eq!(
        placed
            .iter()
            .find(|row| row.message.id == mail.id)
            .unwrap()
            .kind
            .reason,
        assessment.deciding.reason
    );
}

#[tokio::test]
async fn top_level_why_returns_stored_evidence_for_historical_archived_mail() {
    let fx = Fixture::new().await;
    let mail = mail(
        &fx,
        &ThreadId::new(),
        "sam@studio.example",
        &[ME],
        &[],
        false,
        Duration::days(45),
    )
    .await;
    fx.state
        .store
        .remove_message_label(&mail.id, &fx.inbox, mxr_core::EventSource::User)
        .await
        .unwrap();
    let ResponseData::MessageKind {
        assessment: Some(assessment),
        mail_kind,
        ..
    } = request(
        &fx,
        Request::GetMessageKind {
            message_id: mail.id.clone(),
        },
    )
    .await
    else {
        panic!("why includes assessment");
    };
    let stored = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap();
    assert_eq!(stored.get(&mail.id), Some(&assessment));
    assert_eq!(mail_kind.reason, assessment.deciding.reason);
    assert!(!assessment.deciding.evidence.is_empty());
    assert!(
        fx.state
            .store
            .arrivals_by_ids(&[mail.id])
            .await
            .unwrap()
            .is_empty(),
        "historical mail needs no arrival to have an assessment"
    );
}

#[tokio::test]
async fn assessment_cas_ignores_unrelated_threads_and_live_bookkeeping() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let fx = Fixture::new().await;
    let wanted = inbound(&fx, "sam@studio.example", false).await;
    let other = inbound(&fx, "other@studio.example", false).await;
    let attempts = AtomicUsize::new(0);
    let answers = crate::handler::assessments::materialize_threads(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&wanted.thread_id),
        || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async {
                cached_content(&fx, &other, "", Some("An unrelated new body")).await;
                sqlx::query("UPDATE messages SET flags = flags | 1 WHERE id = ?")
                    .bind(wanted.id.as_str())
                    .execute(fx.state.store.writer())
                    .await?;
                sqlx::query("UPDATE accounts SET sync_cursor = 'next-page'")
                    .execute(fx.state.store.writer())
                    .await?;
                sqlx::query("UPDATE arrivals SET mode = 'updates', placed_at = 1")
                    .execute(fx.state.store.writer())
                    .await?;
                Ok(())
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    let persisted = fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap();
    assert_eq!(answers[&wanted.id], persisted[&wanted.id]);
}

#[tokio::test]
async fn assessment_race_returns_only_committed_answers_or_bounded_pending() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let fx = Fixture::new().await;
    let wanted = inbound(&fx, "sam@studio.example", false).await;
    cached_content(&fx, &wanted, "", Some("Thanks. That works for me.")).await;
    membership(&fx, &wanted.thread_id).await;
    let policy = mxr_core::assessment::CLASSIFIER_POLICY_VERSION;
    let previous = fx
        .state
        .store
        .assessments(&fx.account, policy)
        .await
        .unwrap();
    let attempts = AtomicUsize::new(0);
    let pending = crate::handler::assessments::materialize_threads(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&wanted.thread_id),
        || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async {
                sqlx::query(
                    "UPDATE bodies SET text_plain = text_plain || ' changed' WHERE message_id = ?",
                )
                .bind(wanted.id.as_str())
                .execute(fx.state.store.writer())
                .await?;
                Ok(())
            }
        },
    )
    .await
    .unwrap();
    assert!(pending.is_empty());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(
        fx.state
            .store
            .assessments(&fx.account, policy)
            .await
            .unwrap(),
        previous
    );
    assert!(
        crate::handler::assessments::fresh_stored(&fx.state, &fx.account, previous)
            .await
            .unwrap()
            .is_empty()
    );
    let attempts = AtomicUsize::new(0);
    let answers = crate::handler::assessments::materialize_threads(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&wanted.thread_id),
        || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            let fx = &fx;
            let wanted = &wanted;
            async move {
                if attempt == 0 {
                    cached_content(
                        fx,
                        wanted,
                        "Auto-Submitted: auto-generated\r\n",
                        Some("Your receipt is available."),
                    )
                    .await;
                }
                Ok(())
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(answers[&wanted.id].home, "updates");
    assert_eq!(
        answers,
        fx.state
            .store
            .assessments(&fx.account, policy)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn assessment_dirty_queue_covers_sender_history_aliases_and_rethreading() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "sam@studio.example", false).await;
    let b = inbound(&fx, "SAM@studio.example", false).await;
    let c = inbound(&fx, "other@studio.example", false).await;
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .is_empty());
    cached_content(&fx, &a, "X-SG-EID: bulk\r\n", Some("body")).await;
    let dirty = fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap();
    assert!(dirty.contains(&a.thread_id) && dirty.contains(&b.thread_id));
    assert!(!dirty.contains(&c.thread_id));
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    sqlx::query("INSERT INTO account_addresses(account_id, email, is_primary) VALUES (?, 'alias@example.com', 0)")
        .bind(fx.account.as_str()).execute(fx.state.store.writer()).await.unwrap();
    assert_eq!(
        fx.state
            .store
            .dirty_assessment_threads(&fx.account, None)
            .await
            .unwrap()
            .len(),
        3
    );
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    sqlx::query("UPDATE messages SET thread_id = ? WHERE id = ?")
        .bind(c.thread_id.as_str())
        .bind(a.id.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let dirty = fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap();
    assert!(dirty.contains(&b.thread_id) && dirty.contains(&c.thread_id));
}

#[tokio::test]
async fn concurrent_classification_keeps_pending_mail_visible_then_recovers_stored_agreement() {
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "sam@studio.example", false).await;
    cached_content(&fx, &mail, "", Some("Could you confirm the date?")).await;
    membership(&fx, &mail.thread_id).await;
    let policy = mxr_core::assessment::CLASSIFIER_POLICY_VERSION;
    let before = fx
        .state
        .store
        .assessments(&fx.account, policy)
        .await
        .unwrap();
    let state = fx.state.clone();
    let id = mail.id.clone();
    let hook = std::sync::Arc::new(move || {
        let state = state.clone();
        let id = id.clone();
        Box::pin(async move {
            // A separate writer lands after all resolver reads, before CAS.
            tokio::spawn(async move {
                sqlx::query(
                    "UPDATE bodies SET text_plain = text_plain || ' changed' WHERE message_id = ?",
                )
                .bind(id.as_str())
                .execute(state.store.writer())
                .await
            })
            .await
            .unwrap()?;
            Ok(())
        })
            as std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = Result<(), crate::handler::HandlerError>>
                        + Send,
                >,
            >
    });
    crate::handler::assessments::BEFORE_COMMIT
        .scope(hook, async {
            let ResponseData::ArrivalModes { items } = request(
                &fx,
                Request::GetArrivalModes {
                    message_ids: vec![mail.id.clone()],
                },
            )
            .await
            else {
                panic!("chips");
            };
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].bucket, ArrivalBucketData::Sorting);
            assert!(items[0].chip.contains("sorting"));
            let ResponseData::ArrivalList { list } = request(
                &fx,
                Request::ListArrivals {
                    account_id: Some(fx.account.clone()),
                    bucket: None,
                    since: Some(Utc::now() - Duration::days(1)),
                    until: None,
                    limit: 20,
                },
            )
            .await
            else {
                panic!("arrivals");
            };
            assert!(list.items.iter().any(
                |item| item.message_id == mail.id && item.bucket == ArrivalBucketData::Sorting
            ));
            let held = membership(&fx, &mail.thread_id).await;
            assert!(held.assessments.is_empty());
            assert!(
                held.modes.is_empty(),
                "pending cannot invent an independent home"
            );
            assert!(held.in_inbox);
            let why = crate::handler::places::get_message_kind(&fx.state, &mail.id).await.unwrap_err().into_response();
            assert!(matches!(why, Response::Error { code, retryable: true, .. } if code == "classification_pending"));
            assert_eq!(
                fx.state
                    .store
                    .assessments(&fx.account, policy)
                    .await
                    .unwrap(),
                before
            );
        })
        .await;
    let held = membership(&fx, &mail.thread_id).await;
    let persisted = fx
        .state
        .store
        .assessments(&fx.account, policy)
        .await
        .unwrap();
    assert_eq!(held.assessments, vec![persisted[&mail.id].clone()]);
    assert_eq!(chip(&fx, &mail.id).await.0, ArrivalBucketData::Messages);
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn sync_page_materializes_only_its_threads_and_dirty_backfill_is_bounded() {
    let fx = Fixture::new().await;
    let mut historical = Vec::new();
    for index in 0..105 {
        historical.push(
            mail(
                &fx,
                &ThreadId::new(),
                &format!("person{index}@studio.example"),
                &[ME],
                &[],
                false,
                Duration::days(100),
            )
            .await,
        );
    }
    let last = historical.last().unwrap();
    crate::handler::assessments::reconcile_messages(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&last.id),
    )
    .await
    .unwrap();
    let policy = mxr_core::assessment::CLASSIFIER_POLICY_VERSION;
    assert_eq!(
        fx.state
            .store
            .assessments(&fx.account, policy)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fx.state
            .store
            .dirty_assessment_threads(&fx.account, None)
            .await
            .unwrap()
            .len(),
        100
    );
    let cursor = crate::handler::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .assessments(&fx.account, policy)
            .await
            .unwrap()
            .len(),
        101
    );
    crate::handler::assessments::reconcile_dirty_page(&fx.state, &fx.account, Some(&cursor))
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .assessments(&fx.account, policy)
            .await
            .unwrap()
            .len(),
        105
    );
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .is_empty());
    let revision = fx.state.store.assessment_input_revision().await.unwrap();
    sqlx::query("UPDATE messages SET flags = flags | 1")
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET sync_cursor = 'page-2', updated_at = updated_at + 1")
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    fx.state.store.refresh_contacts().await.unwrap();
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    let revision_after_contacts = fx.state.store.assessment_input_revision().await.unwrap();
    assert!(
        revision_after_contacts >= revision,
        "new contact existence is relevant"
    );
    fx.state.store.refresh_contacts().await.unwrap();
    assert_eq!(
        fx.state.store.assessment_input_revision().await.unwrap(),
        revision_after_contacts
    );
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn assessment_dependencies_cover_sent_recipients_and_account_deletion_cascades() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "sam@studio.example", false).await;
    let b = inbound(&fx, "other@studio.example", false).await;
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    let sent = mail(
        &fx,
        &ThreadId::new(),
        ME,
        &["sam@studio.example"],
        &[],
        false,
        Duration::minutes(1),
    )
    .await;
    let dirty = fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap();
    assert!(dirty.contains(&a.thread_id) && dirty.contains(&sent.thread_id));
    assert!(!dirty.contains(&b.thread_id));
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(fx.account.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let generations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM assessment_thread_inputs WHERE account_id = ?")
            .bind(fx.account.as_str())
            .fetch_one(fx.state.store.reader())
            .await
            .unwrap();
    assert_eq!(generations, 0);
    assert!(fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn deleting_the_last_thread_message_during_classification_is_pending_not_a_foreign_key_error()
{
    use std::sync::atomic::{AtomicBool, Ordering};
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "sam@studio.example", false).await;
    let deleted = AtomicBool::new(false);
    let answers = crate::handler::assessments::materialize_threads(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&mail.thread_id),
        || async {
            if !deleted.swap(true, Ordering::SeqCst) {
                sqlx::query("DELETE FROM messages WHERE id = ?")
                    .bind(mail.id.as_str())
                    .execute(fx.state.store.writer())
                    .await?;
            }
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(answers.is_empty());
    assert!(fx
        .state
        .store
        .assessments(&fx.account, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_recipient_decision_invalidates_other_senders_thread_geometry() {
    let fx = Fixture::new().await;
    let mail = mail(
        &fx,
        &ThreadId::new(),
        "sam@studio.example",
        &[ME, "participant@studio.example"],
        &[],
        false,
        Duration::minutes(5),
    )
    .await;
    crate::handler::assessments::reconcile_account(&fx.state, &fx.account)
        .await
        .unwrap();
    let old = fx
        .state
        .store
        .assessment_thread_revisions(&fx.account, std::slice::from_ref(&mail.thread_id))
        .await
        .unwrap();
    sqlx::query("INSERT INTO screener_decisions(account_id, sender_email, disposition, decided_at) VALUES (?, 'participant@studio.example', 'paper_trail', 1)")
        .bind(fx.account.as_str()).execute(fx.state.store.writer()).await.unwrap();
    assert_ne!(
        old,
        fx.state
            .store
            .assessment_thread_revisions(&fx.account, std::slice::from_ref(&mail.thread_id))
            .await
            .unwrap()
    );
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .contains(&mail.thread_id));
}

#[tokio::test]
async fn idle_assessment_projection_completes_while_a_writer_is_held() {
    let mut fx = Fixture::new().await;
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(
        mxr_store::Store::new(&dir.path().join("idle-projection.db"))
            .await
            .unwrap(),
    );
    let account = fx
        .state
        .store
        .get_account(&fx.account)
        .await
        .unwrap()
        .unwrap();
    let inbox = fx
        .state
        .store
        .list_labels_by_account(&fx.account)
        .await
        .unwrap()
        .into_iter()
        .find(|label| label.id == fx.inbox)
        .unwrap();
    store.insert_account(&account).await.unwrap();
    store.upsert_label(&inbox).await.unwrap();
    Arc::get_mut(&mut fx.state).unwrap().store = store.clone();
    let mail = inbound(&fx, "sam@studio.example", false).await;
    let expected = membership(&fx, &mail.thread_id).await;
    let tx = store.writer().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let actual = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        membership(&fx, &mail.thread_id),
    )
    .await
    .expect("settled mode membership must not wait for the SQLite writer");
    assert_eq!(actual, expected);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn moving_an_existing_action_to_do_does_not_create_an_undo_that_dismisses_it() {
    let fx = Fixture::new().await;
    let mail = inbound(&fx, "reviewer@example.com", false).await;
    cached_content(&fx, &mail, "", Some("Could you review the launch plan?")).await;
    line(&fx).await;
    let rows = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let before = rows[0].clone();
    for _ in 0..2 {
        let outcome = move_to(&fx, &mail.id, ModeKindData::Todo, false).await;
        assert_eq!(outcome.copy, "Already in To do.");
        assert!(outcome.correction_id.is_none() && outcome.aspect_id.is_none());
    }
    let filed = move_to(&fx, &mail.id, ModeKindData::Archive, false).await;
    undo(&fx, filed.correction_id.unwrap()).await;
    assert_eq!(
        fx.state.store.get_todo(&before.id).await.unwrap(),
        Some(before)
    );
}
