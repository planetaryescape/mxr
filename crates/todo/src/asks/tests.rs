use super::*;
use chrono::TimeZone;
use mxr_core::OwnedAddresses;

pub(super) fn judge(body: &str, outbound: bool) -> AskAssessment {
    judge_as(body, outbound, RecipientRole::AloneTo, false, false)
}

fn judge_as(
    body: &str,
    outbound: bool,
    role: RecipientRole,
    replied: bool,
    list: bool,
) -> AskAssessment {
    judge_complete(body, outbound, role, replied, list, "")
}

pub(super) fn judge_complete(
    body: &str,
    outbound: bool,
    role: RecipientRole,
    replied: bool,
    list: bool,
    subject: &str,
) -> AskAssessment {
    let account = AccountId::new();
    let message = MessageId::new();
    let owner = Address {
        name: Some("Owner".into()),
        email: "owner@example.com".into(),
    };
    let correspondent = Address {
        name: Some("Alex".into()),
        email: "alex@example.com".into(),
    };
    let group = [owner.clone(), correspondent.clone()];
    let addresses = OwnedAddresses::new(Some(&owner.email), ["alias@example.com"]);
    let matcher = |email: &str| addresses.contains(email);
    assess(
        &AskInput {
            message_id: &message,
            account_id: &account,
            subject,
            body: Some(body),
            from: if outbound { &owner } else { &correspondent },
            to: if outbound {
                std::slice::from_ref(&correspondent)
            } else if role == RecipientRole::SharedTo {
                &group
            } else if role == RecipientRole::AloneTo {
                std::slice::from_ref(&owner)
            } else {
                &[]
            },
            cc: if role == RecipientRole::Cc {
                std::slice::from_ref(&owner)
            } else {
                &[]
            },
            bcc: if role == RecipientRole::Bcc {
                std::slice::from_ref(&owner)
            } else {
                &[]
            },
            list_mail: list,
            bulk: false,
            owner_names: &["Owner".into()],
            owner_addresses: &["owner@example.com".into(), "alias@example.com".into()],
            is_owner: &matcher,
            sent: Utc
                .with_ymd_and_hms(2026, 10, 23, 23, 50, 0)
                .single()
                .expect("resolved asks fixture"),
            predecessor: None,
            owner_replied_afterwards: replied,
            reply_capable: true,
        },
        &Utc,
        "test".into(),
    )
}

pub(super) fn answers(assessment: &AskAssessment) -> Vec<AskAnswer> {
    assessment.units.iter().map(|u| u.answer).collect()
}

#[test]
fn bill_needs_positive_amount_and_message_anchored_due() {
    for body in [
        "Invoice amount due £0.00 by tomorrow.",
        "Invoice amount due -£10 by tomorrow.",
        "Invoice amount due £12.",
    ] {
        assert!(
            !answers(&judge(body, false)).contains(&AskAnswer::AsksYou),
            "{body}"
        );
    }
    let result = judge("Invoice amount due £12 by tomorrow.", false);
    assert!(answers(&result).contains(&AskAnswer::AsksYou));
    assert_eq!(
        result.facts.dates[0].at.date_naive().to_string(),
        "2026-10-24"
    );
}

#[test]
fn owner_requests_and_promises_have_opposite_directions() {
    assert_eq!(
        answers(&judge("Could you send me the report?", true)),
        [AskAnswer::YouAskedThem]
    );
    assert_eq!(
        answers(&judge("I'll send the report by tomorrow.", true)),
        [AskAnswer::YouPromised]
    );
    assert_eq!(
        answers(&judge("I'll send the report by tomorrow.", false)),
        [AskAnswer::TheyPromisedYou]
    );
    assert_eq!(
        answers(&judge("Please send me the report by tomorrow.", false)),
        [AskAnswer::AsksYou]
    );
}

#[test]
fn quoted_and_forwarded_asks_are_context() {
    for body in ["> Please sign the contract.", "FYI\n---------- Forwarded message ---------\nFrom: Other <other@example.com>\nPlease sign the contract."] {
        let result = judge(body, false);
        assert!(!answers(&result).contains(&AskAnswer::AsksYou));
        assert!(result.facts.zones.iter().any(|z| matches!(z.kind, ZoneKind::Quoted | ZoneKind::Forwarded)));
    }
}

#[test]
fn separate_actions_have_separate_exact_quotes() {
    let body = "Please sign the agreement. Invoice #AB-123 amount due EUR 42 by tomorrow.";
    let result = judge(body, false);
    assert_eq!(answers(&result), [AskAnswer::AsksYou, AskAnswer::AsksYou]);
    assert_eq!(
        result.units[0]
            .tags
            .as_ref()
            .expect("resolved asks fixture")
            .action,
        ActionKind::Sign
    );
    assert_eq!(
        result.units[1]
            .tags
            .as_ref()
            .expect("resolved asks fixture")
            .reference,
        Some(0)
    );
    for unit in &result.units {
        assert!(body.contains(result.quote(unit).expect("resolved asks fixture")));
    }
}

#[test]
fn conditional_and_negated_asks_never_become_tasks() {
    for body in [
        "If you would like, please sign the contract.",
        "Please do not sign the contract.",
        "You no longer need to sign the contract.",
    ] {
        assert_eq!(answers(&judge(body, false)), [AskAnswer::Nothing], "{body}");
    }
}

#[test]
fn group_and_copied_mail_cannot_assign_an_unknown_owners_task() {
    let body = "Please sign the contract.";
    assert_eq!(
        answers(&judge_as(
            body,
            false,
            RecipientRole::SharedTo,
            false,
            false
        )),
        [AskAnswer::NotSure]
    );
    assert_eq!(
        answers(&judge_as(body, false, RecipientRole::Cc, false, false)),
        [AskAnswer::Nothing]
    );
    assert_eq!(
        answers(&judge_as(body, false, RecipientRole::AloneTo, false, true)),
        [AskAnswer::Nothing]
    );
    assert_eq!(
        answers(&judge_as(
            "Owner, please sign the contract.",
            false,
            RecipientRole::SharedTo,
            false,
            false
        )),
        [AskAnswer::AsksYou]
    );
    assert_eq!(
        answers(&judge_as(
            "Alex, please sign the contract.",
            false,
            RecipientRole::SharedTo,
            false,
            false
        )),
        [AskAnswer::Nothing]
    );
}

#[test]
fn an_owner_reply_settles_reply_only_but_does_not_complete_a_task() {
    assert_eq!(
        answers(&judge_as(
            "Does this work for you?",
            false,
            RecipientRole::AloneTo,
            true,
            false
        )),
        [AskAnswer::Nothing]
    );
    assert_eq!(
        answers(&judge_as(
            "Please sign the contract.",
            false,
            RecipientRole::AloneTo,
            true,
            false
        )),
        [AskAnswer::AsksYou]
    );
}

#[test]
fn signature_footer_and_forward_authors_do_not_become_asks() {
    let result = judge("FYI\n-- \nPlease sign the contract.\nUnsubscribe", false);
    assert!(!answers(&result).contains(&AskAnswer::AsksYou));
    assert!(result
        .facts
        .zones
        .iter()
        .any(|z| z.kind == ZoneKind::Signature));
    let quote = judge(
        "On Friday, Other <other@example.com> wrote:\n> Please sign the contract.",
        false,
    );
    assert_eq!(
        quote
            .facts
            .zones
            .iter()
            .find(|z| z.kind == ZoneKind::Quoted)
            .expect("resolved asks fixture")
            .author
            .as_deref(),
        Some("other@example.com")
    );
}

#[test]
fn a_rand_shaped_invoice_reference_is_not_money_owed() {
    let result = judge("Invoice #R123 is due tomorrow.", false);
    assert!(result.facts.amounts.is_empty());
    assert_eq!(result.facts.references[0].value, "r123");
    assert_eq!(answers(&result), [AskAnswer::NotSure]);
}

#[test]
fn stored_calendar_rsvp_is_reply_provenance_and_never_an_attending_claim() {
    let mut result = judge("For your records.", false);
    let calendar = mxr_core::CalendarMetadata {
        uid: Some("event-uid".into()),
        summary: Some("Planning".into()),
        method: Some("REQUEST".into()),
        rsvp_requested: true,
        ..Default::default()
    };
    super::include_calendar(&mut result, Some(calendar));
    let unit = result.units.last().expect("resolved asks fixture");
    assert_eq!(unit.answer, AskAnswer::AsksYou);
    assert_eq!(
        unit.tags.as_ref().expect("resolved asks fixture").action,
        ActionKind::Reply
    );
    assert_eq!(result.facts.zones[unit.zone].kind, ZoneKind::Calendar);
    assert_eq!(result.quote(unit), Some("Planning"));
    assert_eq!(unit.votes[0].rule, "ics_request_rsvp_needs_action");
    assert_eq!(
        result
            .calendar
            .expect("resolved asks fixture")
            .uid
            .as_deref(),
        Some("event-uid")
    );
}
