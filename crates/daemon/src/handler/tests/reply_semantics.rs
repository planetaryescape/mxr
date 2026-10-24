//! Reply speech acts are graded from canonical stored units, independently of live visibility.
use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, TimeZone, Utc};
use mxr_core::asks::{AskAnswer, AskAssessment, ASKS_POLICY_VERSION};
use mxr_core::{Envelope, MessageBody, MessageDirection, MessageFlags, ThreadId};
use mxr_provider_fake::fixtures::scenarios::{Expect, Home, Labelled, SenderKind, Who};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap()
}

async fn fixture() -> Fixture {
    let mut config = mxr_config::MxrConfig::default();
    config.llm.enabled = false;
    config.search.semantic.enabled = false;
    config.search.semantic.auto_download_models = false;
    config.activity.enabled = false;
    Fixture::with_config(config).await
}

async fn source(fx: &Fixture, text: &str) -> Envelope {
    let mut mail = fx
        .message(
            &ThreadId::new(),
            "riley@example.com",
            ME,
            Duration::minutes(5),
            None,
        )
        .await;
    mail.subject = "Next steps".into();
    mail.date = now() - Duration::minutes(5);
    fx.store_envelope(&mail, MessageDirection::Inbound).await;
    body(fx, &mail, text, None).await;
    mail
}

async fn body(fx: &Fixture, mail: &Envelope, text: &str, headers: Option<&str>) {
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: mail.id.clone(),
            text_plain: Some(text.into()),
            text_html: None,
            attachments: vec![],
            fetched_at: now(),
            metadata: mxr_core::MessageMetadata {
                raw_headers: headers.map(str::to_string),
                ..Default::default()
            },
        })
        .await
        .unwrap();
}

async fn settle(fx: &Fixture, mail: &Envelope) {
    crate::handler::assessments::materialize_threads_at(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&mail.thread_id),
        || async { Ok(()) },
        now(),
    )
    .await
    .unwrap();
    assert!(fx
        .state
        .store
        .pending_asks_projections(&fx.account)
        .await
        .unwrap()
        .is_empty());
}

async fn observe(
    fx: &Fixture,
    mail: &Envelope,
    expected_reply: bool,
) -> (AskAssessment, bool, u32) {
    settle(fx, mail).await;
    let mut packet = fx
        .state
        .store
        .classify_eval_packet(std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    let entry = packet.remove(0);
    assert!(!entry.projection_pending);
    let answer = entry.asks.unwrap();
    assert_eq!(answer.policy_version, ASKS_POLICY_VERSION);
    assert_eq!(
        answer.context_token,
        mxr_store::asks_context_token().unwrap()
    );
    assert!(!answer.input_fingerprint.is_empty());
    assert!(!answer.pending_body);
    if answer.facts.owner_authored {
        assert!(
            entry.home.is_none(),
            "sent mail has no delivered inbound home"
        );
    } else {
        assert!(entry.home.is_some());
    }
    // Feed the packet's semantic reply answer through the existing grader;
    // a synthetic oracle avoids resolving any mixed-generator scenario.
    let reply = !entry.envelope.flags.contains(MessageFlags::SPAM)
        && answer
            .units
            .iter()
            .any(|u| u.answer == AskAnswer::AsksYouToReply);
    let label = Labelled {
        provider_id: mail.provider_id.clone(),
        from_email: mail.from.email.clone(),
        scenario: "native-reply-semantics",
        description: "Independently authored reply pair",
        expect: Expect {
            reply: expected_reply,
            ..Expect::new(Who::Person, Home::Messages, false)
        },
        items: vec![],
        outbound: false,
        sender: SenderKind::Person,
        locale: "en-GB",
        family: "native-reply-semantics",
        twin_of: None,
        differs_by: "",
        review: None,
        held_out: false,
    };
    let mut grader = crate::handler::classify_eval_asks::AskGrader::default();
    grader.grade(
        crate::handler::classify_eval_asks::AskInput {
            label: &label,
            rows: crate::handler::classify_eval_asks::Row::from_asks(
                &answer,
                "Riley",
                &entry.projected_todos,
            ),
            reply_found: Some(reply),
            sent: mail.date,
            provenance_slices: vec![],
        },
        &mail.from.email,
    );
    let (summary, _) = grader.finish();
    let counts = summary.unwrap().reply_owed.counts;
    let semantic_errors = counts.false_positives + counts.false_negatives;
    let ResponseData::ModeMembership { threads } = request(
        fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(mail.thread_id.clone()),
            thread_ids: vec![],
        },
    )
    .await
    else {
        panic!("membership");
    };
    assert_eq!(
        threads[0].asks.iter().find(|a| a.message_id == mail.id),
        Some(&answer.summary())
    );
    let desk = crate::handler::desk::compose_desk(&fx.state, Some(&fx.account), now())
        .await
        .unwrap();
    let turn = desk.rows.iter().any(|row| {
        row.thread_id == mail.thread_id
            && matches!(
                row.lane,
                mxr_protocol::DeskLaneKind::Owed | mxr_protocol::DeskLaneKind::PeopleNew
            )
    });
    let (stored_revision, effective_revision): (i64, i64) = sqlx::query_as(
        "SELECT a.input_revision, i.effective_revision FROM mail_assessments a JOIN messages m ON m.id = a.message_id AND m.account_id = a.account_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id WHERE a.message_id = ? AND a.account_id = ? AND a.policy_version = ?"
    ).bind(mail.id.as_str()).bind(fx.account.as_str()).bind(ASKS_POLICY_VERSION)
        .fetch_one(fx.state.store.reader()).await.unwrap();
    assert_eq!(stored_revision, effective_revision);
    let source = fx
        .state
        .store
        .desk_messages_in_threads(&fx.account, std::slice::from_ref(&mail.thread_id))
        .await
        .unwrap();
    let senders = crate::handler::desk::Senders::load_raw(&fx.state, &fx.account, &source)
        .await
        .unwrap();
    let own = crate::handler::desk::self_matcher(&fx.state, &fx.account)
        .await
        .unwrap();
    let message = source.iter().find(|m| m.id == mail.id).unwrap();
    let reply_capable = !message.trashed
        && crate::handler::mail_kind::human_response(&senders.signals(message, &own));
    let units: Vec<_> = answer
        .units
        .iter()
        .map(|unit| {
            serde_json::json!({
                "quote": answer.quote(unit), "zone": answer.facts.zones[unit.zone].kind,
                "author": answer.facts.zones[unit.zone].author, "span": unit.span,
                "votes": unit.votes, "tags": unit.tags, "answer": unit.answer,
            })
        })
        .collect();
    eprintln!(
        "REPLY_PACKET {}",
        serde_json::json!({
            "source": mail.id, "policy": answer.policy_version, "fingerprint": answer.input_fingerprint,
        "stored_revision": stored_revision, "effective_revision": effective_revision,
        "reply_capable": reply_capable, "auto_generated": message.evidence.auto_generated,
        "bulk": message.evidence.bulk, "is_reply": message.evidence.is_reply,
        "thread_correspondence": message.evidence.thread_correspondence,
            "context": answer.context_token, "home": entry.home.map(|home| home.home),
            "role": answer.facts.recipient_role, "owner_authored": answer.facts.owner_authored,
            "owner_replied_afterwards": answer.facts.owner_replied_afterwards,
            "units": units, "semantic_reply": reply, "semantic_counts": counts, "delivered_turn": turn,
        })
    );
    (answer, turn, semantic_errors)
}

#[tokio::test]
async fn sender_response_commitment_and_owner_response_request_have_different_debt() {
    let fx = fixture().await;
    let promise = source(&fx, "I will respond tomorrow.").await;
    let request = source(&fx, "Could you respond tomorrow?").await;
    let (promise, promise_turn, promise_errors) = observe(&fx, &promise, false).await;
    let (request, request_turn, request_errors) = observe(&fx, &request, true).await;
    assert_eq!(request_errors, 0);
    assert!(request_turn);
    assert!(request
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYouToReply));
    assert_eq!(
        promise_errors, 0,
        "the sender's future response is not the owner's debt"
    );
    assert!(!promise_turn);
    assert!(!promise
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYouToReply));
    let unit = promise
        .units
        .iter()
        .find(|u| promise.quote(u) == Some("I will respond tomorrow."))
        .unwrap();
    let tags = unit.tags.as_ref().unwrap();
    assert_eq!(tags.act, mxr_core::asks::Act::Commitment);
    assert_eq!(tags.actor, mxr_core::asks::Actor::Correspondent);
    assert_eq!(tags.action, mxr_core::asks::ActionKind::Reply);
    assert_eq!(tags.state, mxr_core::asks::AskState::Pending);
    assert_eq!(unit.answer, AskAnswer::TheyPromisedYou);
}

#[tokio::test]
async fn later_primary_and_alias_responses_clear_the_stored_owner_debt() {
    for owner in [ME, "alias@example.com"] {
        let fx = fixture().await;
        if owner != ME {
            sqlx::query(
                "INSERT INTO account_addresses(account_id, email, is_primary) VALUES (?, ?, 0)",
            )
            .bind(fx.account.as_str())
            .bind(owner)
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        }
        let mail = source(&fx, "Could you respond tomorrow?").await;
        let (before, turn, errors) = observe(&fx, &mail, true).await;
        assert_eq!(errors, 0);
        assert!(turn);
        let mut sent = fx
            .message(
                &mail.thread_id,
                owner,
                "riley@example.com",
                Duration::minutes(1),
                mail.message_id_header.as_deref(),
            )
            .await;
        sent.date = now() - Duration::minutes(1);
        sent.flags |= MessageFlags::SENT;
        fx.store_envelope(&sent, MessageDirection::Outbound).await;
        body(&fx, &sent, "Here is my answer.", None).await;
        let (after, turn, errors) = observe(&fx, &mail, false).await;
        assert!(after.facts.owner_replied_afterwards);
        assert_ne!(before.input_fingerprint, after.input_fingerprint);
        assert_eq!(errors, 0);
        assert!(!turn);
        assert!(after
            .units
            .iter()
            .any(|u| after.quote(u) == Some("Could you respond tomorrow?")
                && u.answer == AskAnswer::Nothing));
    }
}

#[tokio::test]
async fn recipient_and_salutation_do_not_delegate_somebody_elses_response() {
    for (text, copied, expected) in [
        ("Morgan, could you respond tomorrow?", false, true),
        ("Dana, could you respond tomorrow?", false, false),
        ("Could you respond tomorrow?", true, false),
    ] {
        let fx = fixture().await;
        let mut mail = source(&fx, text).await;
        mail.to[0].name = Some("Morgan".into());
        if copied {
            mail.cc = mail.to.clone();
            mail.to[0].email = "dana@example.com".into();
            mail.to[0].name = Some("Dana".into());
        }
        fx.store_envelope(&mail, MessageDirection::Inbound).await;
        let (_, turn, errors) = observe(&fx, &mail, expected).await;
        assert_eq!(errors, 0, "{text}; copied={copied}");
        assert_eq!(turn, expected);
    }
}

#[tokio::test]
async fn generated_feedback_does_not_create_reply_debt_or_hide_a_real_signing_obligation() {
    let fx = fixture().await;
    let human = source(&fx, "Please respond to me.").await;
    let (_, turn, errors) = observe(&fx, &human, true).await;
    assert_eq!(errors, 0);
    assert!(turn);
    let generated = source(
        &fx,
        "Please respond using the feedback form.\nPlease sign the agreement.",
    )
    .await;
    body(
        &fx,
        &generated,
        "Please respond using the feedback form.\nPlease sign the agreement.",
        Some("Auto-Submitted: auto-generated\r\n"),
    )
    .await;
    let (answer, turn, errors) = observe(&fx, &generated, false).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYou
            && answer.quote(u) == Some("Please sign the agreement.")));
    let todos = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 20)
        .await
        .unwrap();
    assert!(todos
        .iter()
        .any(|t| t.source_message_id.as_ref() == Some(&generated.id)
            && t.reason == "Please sign the agreement."));
}

#[tokio::test]
async fn semantic_reply_survives_archiving_while_the_delivered_turn_does_not() {
    let fx = fixture().await;
    let mail = source(&fx, "Could you respond tomorrow?").await;
    let (before, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(turn);
    fx.state
        .store
        .set_message_labels(&mail.id, &[], mxr_core::EventSource::User)
        .await
        .unwrap();
    let (after, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    assert_eq!(before.units, after.units);
}

#[tokio::test]
async fn approved_visible_followup_and_notice_packets_expose_source_normalization() {
    let fx = fixture().await;
    // Only literals from the public terse people variant and rail-tickets notice.
    // No mixed generator or scenario selection is executed.
    let followup = source(
        &fx,
        "Thanks, that works for me. I'll update the doc and ping you when it's ready.\n\nRiley",
    )
    .await;
    let (answer, turn, errors) = observe(&fx, &followup, false).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::TheyPromisedYou));
    let text = "How was your journey with Railway Tickets? We'd love your feedback.";
    let mut notice = source(&fx, text).await;
    notice.from.email = "tickets@railway.demo.mxr.local".into();
    notice.from.name = Some("Railway Tickets".into());
    notice.subject = "Rate your journey".into();
    fx.store_envelope(&notice, MessageDirection::Inbound).await;
    let (answer, turn, errors) = observe(&fx, &notice, false).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    assert!(answer.units.iter().any(|u| answer.quote(u) == Some(text)
        && u.tags
            .as_ref()
            .is_some_and(|t| t.act == mxr_core::asks::Act::Offer)));
}

#[tokio::test]
async fn embedded_speaker_future_does_not_steal_an_owner_request() {
    let fx = fixture().await;
    let mail = source(&fx, "Could you confirm when I will respond?").await;
    let (answer, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(turn);
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYouToReply));
}

mod invitations;

mod projection;

mod speech_acts;

mod mixed_acts;
