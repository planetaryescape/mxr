//! Independently authored statements and requests share response verbs, not debt.
use super::*;
use mxr_core::asks::{Act, ActionKind, Actor, AskState};

#[tokio::test]
async fn response_statements_and_requests_retain_their_person_and_speech_act() {
    let cases = [
        (
            "I will gladly respond tomorrow.",
            Act::Commitment,
            Actor::Correspondent,
            AskState::Pending,
            false,
            "I will gladly respond tomorrow.",
        ),
        (
            "I am going to respond tomorrow.",
            Act::Commitment,
            Actor::Correspondent,
            AskState::Pending,
            false,
            "I am going to respond tomorrow.",
        ),
        (
            "I can usually respond within a day.",
            Act::Offer,
            Actor::Correspondent,
            AskState::Pending,
            false,
            "I can usually respond within a day.",
        ),
        (
            "I have not been able to respond.",
            Act::Information,
            Actor::Correspondent,
            AskState::Pending,
            false,
            "I have not been able to respond.",
        ),
        (
            "I will not respond tomorrow.",
            Act::Information,
            Actor::Correspondent,
            AskState::Cancelled,
            false,
            "I will not respond tomorrow.",
        ),
        (
            "She will respond tomorrow.",
            Act::Commitment,
            Actor::ThirdParty,
            AskState::Pending,
            false,
            "She will respond tomorrow.",
        ),
        (
            "I will read the notes and respond tomorrow.",
            Act::Commitment,
            Actor::Correspondent,
            AskState::Pending,
            false,
            "I will read the notes and respond tomorrow.",
        ),
        (
            "If the meeting ends early, I will respond tomorrow.",
            Act::Commitment,
            Actor::Correspondent,
            AskState::Conditional,
            false,
            "If the meeting ends early, I will respond tomorrow.",
        ),
        (
            "I was hoping I could respond tomorrow.",
            Act::Information,
            Actor::Correspondent,
            AskState::Pending,
            false,
            "I was hoping I could respond tomorrow.",
        ),
        (
            "I was hoping you could respond tomorrow.",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "I was hoping you could respond tomorrow.",
        ),
        (
            "I ask you to respond tomorrow.",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "I ask you to respond tomorrow.",
        ),
        (
            "I need you to respond tomorrow.",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "I need you to respond tomorrow.",
        ),
        (
            "Could you respond tomorrow?",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "Could you respond tomorrow?",
        ),
        (
            "Are you going to respond tomorrow?",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "Are you going to respond tomorrow?",
        ),
        (
            "How soon can you respond?",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "How soon can you respond?",
        ),
        (
            "Have you been able to respond?",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "Have you been able to respond?",
        ),
        (
            "Please respond after reading the notes.",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "Please respond after reading the notes.",
        ),
        (
            "Could you tell me when she will respond?",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "Could you tell me when she will respond?",
        ),
        (
            "Let me know whether I should respond.",
            Act::Question,
            Actor::Owner,
            AskState::Pending,
            true,
            "Let me know whether I should respond.",
        ),
    ];
    let fx = fixture().await;
    let mut failures = Vec::new();
    for (text, act, actor, state, reply, quote) in cases {
        let mail = source(&fx, text).await;
        let (answer, turn, errors) = observe(&fx, &mail, reply).await;
        let units: Vec<_> = answer
            .units
            .iter()
            .filter(|u| {
                u.tags
                    .as_ref()
                    .is_some_and(|t| t.action == ActionKind::Reply)
            })
            .collect();
        if errors != 0 || turn != reply {
            failures.push(format!(
                "{text}: semantic errors={errors}, delivered turn={turn}, expected={reply}"
            ));
        }
        if units.len() != 1 {
            failures.push(format!(
                "{text}: expected one response unit, found {}",
                units.len()
            ));
            continue;
        }
        let tags = units[0].tags.as_ref().unwrap();
        if (tags.act, tags.actor, tags.state) != (act, actor, state)
            || answer.quote(units[0]) != Some(quote)
        {
            failures.push(format!("{text}: actual tags={tags:?}, quote={:?}; expected={act:?}/{actor:?}/{state:?}, quote={quote:?}", answer.quote(units[0])));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_response_statement_does_not_hide_an_independent_owner_request() {
    let fx = fixture().await;
    let mail = source(
        &fx,
        "I will not respond tomorrow. Could you send the agenda?",
    )
    .await;
    let (answer, _, errors) = observe(&fx, &mail, false).await;
    assert_eq!(errors, 0);
    assert!(answer.units.iter().any(|u| u.answer == AskAnswer::AsksYou
        && u.tags
            .as_ref()
            .is_some_and(|t| t.action == ActionKind::Send && t.actor == Actor::Owner)));
    assert!(!answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::AsksYouToReply));
}

#[tokio::test]
async fn an_owner_authored_response_commitment_keeps_owner_authority() {
    let fx = fixture().await;
    let mut mail = source(&fx, "I will gladly respond tomorrow.").await;
    mail.from.email = ME.into();
    mail.to = vec![mxr_core::Address {
        name: Some("Riley".into()),
        email: "riley@example.com".into(),
    }];
    fx.store_envelope(&mail, MessageDirection::Outbound).await;
    let (answer, turn, errors) = observe(&fx, &mail, false).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    assert!(answer
        .units
        .iter()
        .any(|u| u.answer == AskAnswer::YouPromised
            && u.tags.as_ref().is_some_and(|t| t.actor == Actor::Owner
                && t.act == Act::Commitment
                && t.action == ActionKind::Reply)));
}

#[tokio::test]
async fn nominal_response_requests_are_not_speaker_response_statements() {
    let fx = fixture().await;
    let mut failures = Vec::new();
    for (text, reply) in [
        ("I need a reply tomorrow.", true),
        ("I need an urgent reply tomorrow.", true),
        ("I need a clear and detailed reply tomorrow.", true),
        ("I would appreciate your reply tomorrow.", true),
        ("I received a reply yesterday.", false),
        ("I need a chance to reply tomorrow.", false),
    ] {
        let mail = source(&fx, text).await;
        let (answer, turn, errors) = observe(&fx, &mail, reply).await;
        if errors != 0 || turn != reply {
            failures.push(format!(
                "{text}: errors={errors}, turn={turn}, expected={reply}"
            ));
        }
        assert!(answer.units.iter().any(|u| answer.quote(u) == Some(text)));
        if reply {
            assert!(answer.units.iter().any(|u| u
                .tags
                .as_ref()
                .is_some_and(|t| t.act == Act::Request
                    && t.actor == Actor::Owner
                    && t.action == ActionKind::Reply)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn response_noun_mentions_do_not_invent_a_recipient_question() {
    let fx = fixture().await;
    let mut failures = Vec::new();
    for (text, reply) in [
        ("A reply arrived yesterday.", false),
        ("The reply contains the revised schedule.", false),
        ("Your reply is in the folder.", false),
        ("A reply was received yesterday.", false),
        ("Riley received a reply yesterday.", false),
        ("That reply resolved the question.", false),
        ("Reply tomorrow.", true),
        ("Respond tomorrow.", true),
        ("Let me know your decision.", true),
        ("Could you reply tomorrow?", true),
        ("When is the meeting?", true),
        ("I need a reply tomorrow.", true),
        ("I would appreciate your reply tomorrow.", true),
        ("Let me know when you will reply.", true),
    ] {
        let mail = source(&fx, text).await;
        let (answer, turn, errors) = observe(&fx, &mail, reply).await;
        if errors != 0 || turn != reply {
            failures.push(format!(
                "{text}: semantic errors={errors}, delivered turn={turn}, expected={reply}"
            ));
        }
        assert!(
            answer
                .units
                .iter()
                .any(|unit| answer.quote(unit) == Some(text)),
            "source quote retained: {text}"
        );
        if !reply
            && answer.units.iter().any(|unit| {
                unit.tags.as_ref().is_some_and(|tags| {
                    tags.actor == Actor::Owner && matches!(tags.act, Act::Request | Act::Question)
                })
            })
        {
            failures.push(format!(
                "{text}: a response mention invented an owner request/question"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_response_noun_mention_does_not_hide_an_independent_signature_request() {
    let fx = fixture().await;
    let mail = source(&fx, "A reply arrived yesterday. Please sign the form.").await;
    let (answer, _, errors) = observe(&fx, &mail, false).await;
    assert_eq!(errors, 0);
    assert!(answer
        .units
        .iter()
        .any(|unit| {
            unit.answer == AskAnswer::AsksYou
                && unit.tags.as_ref().is_some_and(|tags| {
                    tags.actor == Actor::Owner && tags.action == ActionKind::Sign
                })
        }));
    assert!(!answer
        .units
        .iter()
        .any(|unit| unit.answer == AskAnswer::AsksYouToReply));
}
