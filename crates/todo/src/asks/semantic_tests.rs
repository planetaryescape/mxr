use super::tests::{answers, judge};
use super::*;

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

fn judge_unnamed_recipient(
    body: &str,
    role: RecipientRole,
    named_other: Option<&str>,
) -> AskAssessment {
    let addresses =
        mxr_core::OwnedAddresses::new(Some("alex@demo.mxr.local"), std::iter::empty::<&str>());
    let is_owner = |email: &str| addresses.contains(email);
    let owner = Address {
        name: None,
        email: "alex@demo.mxr.local".into(),
    };
    let other = Address {
        name: named_other.map(str::to_string),
        email: "other@example.com".into(),
    };
    let to = match role {
        RecipientRole::AloneTo | RecipientRole::List => vec![owner.clone()],
        RecipientRole::SharedTo => vec![owner.clone(), other.clone()],
        _ => vec![],
    };
    let cc = if role == RecipientRole::Cc {
        vec![owner.clone()]
    } else if named_other.is_some() {
        vec![other]
    } else {
        vec![]
    };
    let bcc = if role == RecipientRole::Bcc {
        vec![owner]
    } else {
        vec![]
    };
    assess(
        &AskInput {
            message_id: &MessageId::new(),
            account_id: &AccountId::new(),
            subject: "Lease renewal",
            body: Some(body),
            from: &Address {
                name: Some("Sam Okafor".into()),
                email: "sam@riverside.example".into(),
            },
            to: &to,
            cc: &cc,
            bcc: &bcc,
            list_mail: role == RecipientRole::List,
            bulk: false,
            owner_names: &[],
            owner_addresses: &["alex@demo.mxr.local".into()],
            is_owner: &is_owner,
            sent: "2026-11-02T12:00:00Z"
                .parse()
                .expect("fixed message timestamp"),
            predecessor: None,
            owner_replied_afterwards: false,
            reply_capable: true,
        },
        &Utc,
        "anonymous-recipient".into(),
    )
}

#[test]
fn an_unknown_owner_name_does_not_claim_shared_or_copied_requests() {
    for role in [
        RecipientRole::SharedTo,
        RecipientRole::Cc,
        RecipientRole::Bcc,
        RecipientRole::List,
    ] {
        let assessment = judge_unnamed_recipient("Hi Alex,\nPlease sign the contract.", role, None);
        assert!(
            !answers(&assessment).contains(&AskAnswer::AsksYou),
            "{role:?}: {assessment:?}"
        );
    }
    let named_other = judge_unnamed_recipient(
        "Hi Alex,\nPlease sign the contract.",
        RecipientRole::AloneTo,
        Some("Alex Smith"),
    );
    assert!(
        !answers(&named_other).contains(&AskAnswer::AsksYou),
        "a known copied addressee still names another actor"
    );
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
fn security_notice_is_not_identity_work_but_independent_tasks_remain() {
    let notice = judge("We detected an unusual sign-in attempt on your account. To protect it, confirm your identity now or access will be limited.", false);
    assert!(
        !answers(&notice).contains(&AskAnswer::AsksYou),
        "{notice:?}"
    );
    let mixed = judge(
        "We detected a new sign-in attempt. Confirm your identity now. Please sign the agreement.",
        false,
    );
    let actions: Vec<_> = mixed
        .units
        .iter()
        .filter(|u| u.answer == AskAnswer::AsksYou)
        .map(|u| u.tags.as_ref().expect("actionable unit tags").action)
        .collect();
    assert_eq!(actions, [ActionKind::Sign]);
    assert!(
        answers(&judge("Please verify your email address.", false)).contains(&AskAnswer::AsksYou)
    );
}
