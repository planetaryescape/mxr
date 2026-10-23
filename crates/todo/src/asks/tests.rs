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

fn judge_complete(
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
fn signed_zero_and_rand_candidates_keep_verbatim_spans() {
    let result = judge(
        "Credit -£12.50. Balance €0.00. Invoice R1,234.50 due tomorrow.",
        false,
    );
    let values: Vec<_> = result
        .facts
        .amounts
        .iter()
        .map(|a| (a.minor, a.currency.as_str()))
        .collect();
    assert_eq!(values, [(-1250, "GBP"), (0, "EUR"), (123450, "ZAR")]);
    assert_eq!(
        result
            .facts
            .amounts
            .iter()
            .map(|amount| {
                &result.facts.zones[amount.zone].text[amount.span.start..amount.span.end]
            })
            .collect::<Vec<_>>(),
        ["-£12.50", "€0.00", "R1,234.50"]
    );
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
fn offer_does_not_veto_an_independent_minimum_payment() {
    let result = judge(
        "Minimum payment £25 due tomorrow. Why not set up a Direct Debit?",
        false,
    );
    assert_eq!(answers(&result), [AskAnswer::AsksYou, AskAnswer::Nothing]);
    let automatic = judge("Your payment of £25 will be collected by tomorrow.", false);
    assert!(!answers(&automatic).contains(&AskAnswer::AsksYou));
}

#[test]
fn resolved_failure_and_current_failure_are_different() {
    assert_eq!(
        answers(&judge(
            "Your payment failed last month but is now resolved.",
            false
        )),
        [AskAnswer::Nothing]
    );
    assert_eq!(
        answers(&judge(
            "Your payment failed. Please update your payment details.",
            false
        )),
        [AskAnswer::AsksYou, AskAnswer::AsksYou]
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
fn pleasantry_is_not_reply_but_a_real_question_is() {
    assert_eq!(answers(&judge("How are you?", false)), [AskAnswer::Nothing]);
    assert_eq!(
        answers(&judge("Does this work for you?", false)),
        [AskAnswer::AsksYouToReply]
    );
}

#[test]
fn unsupported_words_abstain_without_inventing_check_tasks() {
    let result = judge("The situation requires some attention.", false);
    assert_eq!(answers(&result), [AskAnswer::NotSure]);
    assert!(result.units[0].tags.is_none());
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
fn same_issuer_references_and_periods_are_separate_candidates() {
    let result = judge("Invoice #AB-123 amount due GBP 42 by tomorrow. Invoice #AB-456 amount due GBP 15 by Friday.", false);
    assert_eq!(
        result
            .facts
            .references
            .iter()
            .map(|r| r.value.as_str())
            .collect::<Vec<_>>(),
        ["ab-123", "ab-456"]
    );
    assert_eq!(
        result
            .units
            .iter()
            .map(|u| u.tags.as_ref().and_then(|t| t.reference))
            .collect::<Vec<_>>(),
        [Some(0), Some(1)]
    );
    assert_eq!(answers(&result), [AskAnswer::AsksYou, AskAnswer::AsksYou]);
}

#[test]
fn authored_delegation_is_evidence_and_the_forward_stays_context() {
    let body = "Please sign the agreement below.\n---------- Forwarded message ---------\nFrom: Other <other@example.com>\nPlease pay £99 by tomorrow.";
    let result = judge(body, false);
    assert_eq!(
        result
            .units
            .iter()
            .filter(|u| u.answer == AskAnswer::AsksYou)
            .count(),
        1
    );
    assert_eq!(
        result.quote(&result.units[0]),
        Some("Please sign the agreement below.")
    );
    let forwarded = result
        .facts
        .zones
        .iter()
        .find(|z| z.kind == ZoneKind::Forwarded && z.text.contains("Please pay"))
        .expect("resolved asks fixture");
    assert_eq!(forwarded.author.as_deref(), Some("other@example.com"));
}

#[test]
fn sign_and_pay_in_one_sentence_are_independent_units() {
    let result = judge(
        "Please sign the agreement and pay invoice #A123 amount due £30 by tomorrow.",
        false,
    );
    assert_eq!(answers(&result), [AskAnswer::AsksYou, AskAnswer::AsksYou]);
    assert_eq!(
        result
            .units
            .iter()
            .map(|u| u.tags.as_ref().expect("resolved asks fixture").action)
            .collect::<Vec<_>>(),
        [ActionKind::Sign, ActionKind::Pay]
    );
}

#[test]
fn a_multiline_document_keeps_required_fields_together() {
    let result = judge(
        "Invoice #ABC123\nAmount due £25\nDue tomorrow\n\nWhy not set up a Direct Debit?",
        false,
    );
    assert_eq!(answers(&result), [AskAnswer::AsksYou, AskAnswer::Nothing]);
    let tags = result.units[0]
        .tags
        .as_ref()
        .expect("resolved asks fixture");
    assert_eq!(tags.amount, Some(0));
    assert_eq!(tags.due, Some(0));
}

#[test]
fn disagreeing_document_amounts_abstain() {
    let result = judge("Invoice amount due £25 or £40 by tomorrow.", false);
    assert_eq!(answers(&result), [AskAnswer::NotSure]);
}

#[test]
fn a_task_question_is_an_action_and_an_unsupported_task_is_not_a_reply_guess() {
    let result = judge("Could you review the launch plan?", false);
    assert_eq!(answers(&result), [AskAnswer::AsksYou]);
    assert_eq!(
        result.units[0]
            .tags
            .as_ref()
            .expect("resolved asks fixture")
            .action,
        ActionKind::Other
    );
    assert_eq!(
        answers(&judge(
            "Could you take care of the difficult situation?",
            false
        )),
        [AskAnswer::NotSure]
    );
}

#[test]
fn cancelled_body_prevents_a_stale_subject_from_creating_a_task() {
    let result = judge_complete(
        "Please do not sign the contract.",
        false,
        RecipientRole::AloneTo,
        false,
        false,
        "Please sign the contract",
    );
    assert_eq!(answers(&result), [AskAnswer::Nothing, AskAnswer::Nothing]);
}

#[test]
fn owner_identity_keeps_primary_and_aliases_from_the_shared_matcher() {
    let result = judge("Please send me the report.", false);
    assert_eq!(
        result.facts.owner_addresses,
        ["owner@example.com", "alias@example.com"]
    );
    assert_eq!(result.facts.recipient_role, RecipientRole::AloneTo);
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
fn owner_payment_promises_keep_the_speaker_across_a_conjunction() {
    assert_eq!(
        answers(&judge("I'll pay the invoice by tomorrow.", true)),
        [AskAnswer::YouPromised]
    );
    let result = judge(
        "I'll sign the agreement and pay invoice #A123 amount due £30 by tomorrow.",
        true,
    );
    assert_eq!(
        answers(&result),
        [AskAnswer::YouPromised, AskAnswer::YouPromised]
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
fn informational_abstentions_do_not_enter_might_need_you() {
    for text in [
        "The report arrived yesterday.",
        "For your records.",
        "A new article is available.",
    ] {
        let result = judge(text, false);
        assert!(
            result
                .units
                .iter()
                .all(|unit| unit.answer == AskAnswer::Nothing),
            "{text}"
        );
    }
    assert!(
        answers(&judge("Could you arrange the unspecified thing?", false))
            .contains(&AskAnswer::NotSure)
    );
}

#[test]
fn summary_bounds_text_without_altering_source_evidence() {
    let result = judge(&format!("Please send me {}.", "x".repeat(4096)), false);
    let summary = result.summary();
    assert_eq!(summary.units.len(), 1);
    assert!(summary.units[0].quote.is_none());
    assert!(
        result
            .quote(&result.units[0])
            .expect("resolved asks fixture")
            .len()
            > 4096
    );
}

#[test]
fn reference_headers_bind_following_payment_units_without_crossing_paragraphs() {
    let result = judge("Invoice number A100.\nAmount due £42 by tomorrow.", false);
    let pay = result
        .units
        .iter()
        .find(|u| u.tags.as_ref().is_some_and(|t| t.action == ActionKind::Pay))
        .expect("resolved asks fixture");
    assert_eq!(
        pay.tags.as_ref().expect("resolved asks fixture").reference,
        Some(0)
    );
    let separate = judge("Invoice number A100.\n\nAmount due £42 by tomorrow.", false);
    let pay = separate
        .units
        .iter()
        .find(|u| u.tags.as_ref().is_some_and(|t| t.action == ActionKind::Pay))
        .expect("resolved asks fixture");
    assert_eq!(
        pay.tags.as_ref().expect("resolved asks fixture").reference,
        None
    );
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
