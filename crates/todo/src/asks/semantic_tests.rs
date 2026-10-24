use super::tests::{answers, judge};
use super::*;

#[test]
fn yearless_deadline_is_usable_while_its_uncertain_year_stays_visible() {
    let result = judge(
        "Invoice number X-222\nAmount due £120.00\nDue date: 29 October",
        false,
    );
    let unit = result
        .units
        .iter()
        .find(|u| u.answer == AskAnswer::AsksYou)
        .expect("one positive bill");
    let date = &result.facts.dates[unit.tags.as_ref().expect("tags").due.expect("due")];
    assert!(
        !date.checked,
        "the inferred year is still unchecked provenance"
    );
    assert_eq!(date.at.date_naive().to_string(), "2026-10-29");
}

#[test]
fn related_payment_fields_share_a_unit_but_an_offer_does_not_veto_the_bill() {
    for body in [
        "Your council tax payment of £142.00 is due on 28 October 2026.",
        "Your municipal account shows amount due R230.00. Please pay by Friday.",
        "Your card was declined for £34.00. Update your card to retry.",
    ] {
        let a = judge(body, false);
        assert_eq!(
            a.units
                .iter()
                .filter(|u| u.answer == AskAnswer::AsksYou)
                .count(),
            1,
            "{body}: {a:?}"
        );
    }
    let a = judge(
        "Invoice amount due £100 by tomorrow.\n\nSet up a Direct Debit if you would like.",
        false,
    );
    assert_eq!(
        a.units
            .iter()
            .filter(|u| u.answer == AskAnswer::AsksYou)
            .count(),
        1
    );
}

#[test]
fn the_sender_purpose_clause_does_not_assign_their_booking_to_the_owner() {
    let a = judge("Please RSVP by Friday so I can book the room.", false);
    let u = a
        .units
        .iter()
        .find(|u| u.answer == AskAnswer::AsksYou)
        .expect("RSVP");
    assert_eq!(u.tags.as_ref().expect("tags").action, ActionKind::Reply);
    assert!(super::items::is_rsvp(&a, u));
    assert!(!a.units.iter().any(|u| u
        .tags
        .as_ref()
        .is_some_and(|t| t.action == ActionKind::Book)));
}

#[test]
fn sign_and_return_the_same_document_is_one_action_and_passive_consent_is_signing() {
    for body in [
        "Please sign and send it back by Friday.",
        "A document to review and sign: Tenancy renewal.",
        "The consent form must be signed and handed to the teacher by Friday.",
    ] {
        let a = judge(body, false);
        let actionable: Vec<_> = a
            .units
            .iter()
            .filter(|u| u.answer == AskAnswer::AsksYou)
            .collect();
        assert_eq!(actionable.len(), 1, "{body}: {a:?}");
        assert_eq!(
            actionable[0].tags.as_ref().expect("tags").action,
            ActionKind::Sign
        );
    }
}

#[test]
fn availability_and_review_then_reply_are_answers_while_a_firm_cv_request_is_an_action() {
    for body in [
        "Would you have twenty minutes next week?",
        "Could you confirm whether Thursday works?",
        "Could you share your availability for a call?",
        "Could you review the redline and reply with the next step?",
        "Can you try signing in and tell me if it works?",
    ] {
        let a = judge(body, false);
        assert!(
            answers(&a).contains(&AskAnswer::AsksYouToReply),
            "{body}: {a:?}"
        );
        assert!(!answers(&a).contains(&AskAnswer::AsksYou), "{body}");
    }
    assert!(
        answers(&judge("Please send me your CV by Friday.", false)).contains(&AskAnswer::AsksYou)
    );
    assert!(!answers(&judge(
        "If you want, please send me your CV by Friday.",
        false
    ))
    .contains(&AskAnswer::AsksYou));
}

#[test]
fn a_promise_to_send_you_a_document_is_owned_by_its_speaker() {
    let a = judge("I'll send you the signed form by Friday.", true);
    let u = a
        .units
        .iter()
        .find(|u| u.answer == AskAnswer::YouPromised)
        .expect("owner promise");
    assert_eq!(u.tags.as_ref().expect("tags").action, ActionKind::Send);
}

#[test]
fn decimal_comma_and_grouped_amounts_keep_the_full_exact_candidate() {
    for (words, minor, currency) in [
        ("€1.234,56", 123456, "EUR"),
        ("R1 234,56", 123456, "ZAR"),
        ("R450,00", 45000, "ZAR"),
        ("-€1.234,56", -123456, "EUR"),
        ("£1,234.56", 123456, "GBP"),
        ("EUR 1\u{202f}234,56", 123456, "EUR"),
    ] {
        let a = judge(&format!("Invoice amount due {words} by tomorrow."), false);
        assert_eq!(a.facts.amounts.len(), 1, "{words}: {a:?}");
        let candidate = &a.facts.amounts[0];
        assert_eq!(
            (candidate.minor, candidate.currency.as_str()),
            (minor, currency),
            "{words}"
        );
        assert_eq!(
            &a.facts.zones[candidate.zone].text[candidate.span.start..candidate.span.end],
            words
        );
    }
    for words in ["£1.234", "R1 23,45", "£1,23,45"] {
        assert!(
            judge(words, false).facts.amounts.is_empty(),
            "partial malformed price: {words}"
        );
    }
}

#[test]
fn a_yearless_past_due_stays_in_the_past_and_word_durations_use_natural_time() {
    let a = judge(
        "Invoice amount due £25 was due on 20 October. Pay now.",
        false,
    );
    assert_eq!(a.facts.dates[0].at.date_naive().to_string(), "2026-10-20");
    let a = judge(
        "The balance of £86 is now payable; please settle within seven days.",
        false,
    );
    let unit = a
        .units
        .iter()
        .find(|u| u.answer == AskAnswer::AsksYou)
        .expect("payable");
    let date = &a.facts.dates[unit.tags.as_ref().expect("tags").due.expect("duration")];
    assert_eq!(
        date.at,
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 10, 30, 23, 50, 0)
            .single()
            .expect("fixture")
    );
}

#[test]
fn public_webinar_rsvp_is_an_offer_but_personal_rsvp_keeps_the_response_task() {
    let public = judge(
        "Join us for our product webinar on Friday. RSVP here: https://example.com/rsvp",
        false,
    );
    assert!(!answers(&public).contains(&AskAnswer::AsksYou));
    let personal = judge("We are having friends for dinner on Friday. Please RSVP by Thursday so I know how much to cook.", false);
    assert!(answers(&personal).contains(&AskAnswer::AsksYou));
}

#[test]
fn unmarked_account_headers_keep_equal_bills_distinct() {
    let a = judge("Service charge for Flat 2, account AG-1001\nAmount due £250\nDue date: 2 November\n\nService charge for Flat 4, account AG-1002\nAmount due £250\nDue date: 2 November", false);
    let refs: Vec<_> = a
        .units
        .iter()
        .filter(|u| u.answer == AskAnswer::AsksYou)
        .map(|u| {
            &a.facts.references[u
                .tags
                .as_ref()
                .expect("tags")
                .reference
                .expect("explicit account")]
            .value
        })
        .collect();
    assert_eq!(refs, ["ag-1001", "ag-1002"]);
}

#[test]
fn a_postposed_condition_attaches_to_its_task_but_an_embedded_question_is_firm() {
    for body in [
        "Please sign the agreement if the bank approves it.",
        "Please send me your CV if you want to apply.",
    ] {
        let a = judge(body, false);
        assert!(
            !answers(&a).contains(&AskAnswer::AsksYou),
            "contingent task: {body}: {a:?}"
        );
        assert!(a.units.iter().any(|u| u
            .tags
            .as_ref()
            .is_some_and(|t| t.state == AskState::Conditional)));
    }
    assert!(
        answers(&judge("Please sign the agreement by Friday.", false))
            .contains(&AskAnswer::AsksYou)
    );
    assert!(
        answers(&judge("Please let me know if Friday works.", false))
            .contains(&AskAnswer::AsksYouToReply)
    );
    let a = judge(
        "If you want to apply, please send me your CV. Please sign the agreement by Friday.",
        false,
    );
    assert_eq!(
        a.units
            .iter()
            .filter(|u| u.answer == AskAnswer::AsksYou)
            .count(),
        1
    );
    let a = judge(
        "Please send the contract if the bank approves it and let me know if Friday works.",
        false,
    );
    assert!(
        !answers(&a).contains(&AskAnswer::AsksYou),
        "a second embedded question cannot erase the task's condition"
    );
}

#[test]
fn card_expiry_and_payment_method_deadline_are_distinct_date_roles() {
    let a = judge("Your card expires on 26 October. Update your payment method before 30 October or your subscription will lapse.", false);
    let unit = a
        .units
        .iter()
        .find(|u| u.answer == AskAnswer::AsksYou)
        .expect("payment method task");
    let date = &a.facts.dates[unit.tags.as_ref().expect("tags").due.expect("deadline")];
    assert_eq!(date.at.date_naive().to_string(), "2026-10-30");
}

#[test]
fn explicit_salutations_do_not_assign_another_persons_actions_to_the_owner() {
    for body in [
        "Alice,\nCould you send the deck?",
        "Hi Brian, could you send the deck?",
        "Dear Alice:\nPlease sign the agreement.",
    ] {
        let answer = judge(body, false);
        assert!(
            !answer
                .units
                .iter()
                .any(|unit| matches!(unit.answer, AskAnswer::AsksYou | AskAnswer::AsksYouToReply)),
            "{body}: {answer:?}"
        );
    }
    for body in [
        "Hi,\nCould you send the deck?",
        "Owner,\nCould you send the deck?",
        "Hi Owner, could you send the deck?",
        "Hope you're well,\nCould you send the deck?",
    ] {
        assert!(
            answers(&judge(body, false)).contains(&AskAnswer::AsksYou),
            "{body}"
        );
    }
    let separate = judge(
        "Alice, please sign the agreement.\n\nPlease send the deck.",
        false,
    );
    assert_eq!(
        separate
            .units
            .iter()
            .filter(|unit| unit.answer == AskAnswer::AsksYou)
            .count(),
        1,
        "a named action does not veto another unit"
    );
}

#[test]
fn reply_cancellation_is_attached_without_cancelling_an_independent_action() {
    let cancelled = judge("Hi, could you confirm? No need to reply.", false);
    assert!(!answers(&cancelled).contains(&AskAnswer::AsksYouToReply));
    assert!(cancelled.units.iter().any(|unit| unit
        .tags
        .as_ref()
        .is_some_and(|tags| tags.state == AskState::Cancelled)
        && cancelled
            .quote(unit)
            .is_some_and(|quote| quote.contains("No need to reply"))));
    let another_actor = judge("Could you confirm? Alice, no need to reply.", false);
    assert!(answers(&another_actor).contains(&AskAnswer::AsksYouToReply));
    let sign = judge("Please sign the agreement. No need to reply.", false);
    assert!(answers(&sign).contains(&AskAnswer::AsksYou));
    let independent = judge(
        "Could you confirm the meeting?\n\nNo need to reply to Alice's other email.",
        false,
    );
    assert!(answers(&independent).contains(&AskAnswer::AsksYouToReply));
    let scoped = judge(
        "Alice,\nPlease sign the agreement.\n\nPlease send the deck.",
        false,
    );
    assert_eq!(
        scoped
            .units
            .iter()
            .filter(|u| u.answer == AskAnswer::AsksYou)
            .count(),
        1
    );
}

#[test]
fn salutation_review_reminder_heading_is_not_a_person() {
    let a = judge("Reminder: Please sign the agreement.", false);
    assert!(answers(&a).contains(&AskAnswer::AsksYou), "{a:?}");
}

#[test]
fn salutation_review_action_heading_is_not_a_person() {
    let a = judge("Action Required: Please send the deck.", false);
    assert!(answers(&a).contains(&AskAnswer::AsksYou), "{a:?}");
}

#[test]
fn salutation_review_generic_role_is_not_another_person() {
    let a = judge("Dear Ratepayer,\n\nYour account 123456 shows an amount due of £100. Please pay by 28 October 2026.", false);
    assert!(answers(&a).contains(&AskAnswer::AsksYou), "{a:?}");
}

#[test]
fn salutation_review_greeting_uses_the_observed_owner_name() {
    let a = judge("Howzit Owner,\n\nCould you send the deck?", false);
    assert!(answers(&a).contains(&AskAnswer::AsksYou), "{a:?}");
}

#[test]
fn salutation_review_another_emails_cancellation_does_not_cancel_this_reply() {
    let a = judge(
        "Could you confirm the meeting? No need to reply to Alice's other email.",
        false,
    );
    assert!(answers(&a).contains(&AskAnswer::AsksYouToReply), "{a:?}");
}
