//! Which amount, account and deadline a bill's words establish.
use super::tests::{judge, judge_complete};
use super::*;

/// The single bill the mail asks the owner to pay.
fn bill(a: &AskAssessment) -> (i64, String, Option<String>) {
    let bills: Vec<_> = a
        .units
        .iter()
        .filter(|u| u.answer == AskAnswer::AsksYou)
        .collect();
    assert_eq!(bills.len(), 1, "exactly one bill: {a:#?}");
    let tags = bills[0].tags.as_ref().expect("tags");
    assert_eq!(tags.action, ActionKind::Pay, "{a:#?}");
    let amount = &a.facts.amounts[tags.amount.expect("amount")];
    (
        amount.minor,
        amount.currency.clone(),
        tags.reference
            .map(|i| &a.facts.references[i])
            .map(|r| format!("{}:{}", r.kind, r.value)),
    )
}

fn bank_bill(reference: &str, amount: &str) -> String {
    format!(
        "Pay into: FNB\nAccount number: 62012345678\nPayment reference: {reference}\nAmount due: {amount}\nPlease pay by 5 October 2026."
    )
}

#[test]
fn a_payment_reference_is_never_the_amount() {
    let a = judge(
        "Amount due: R953.63\nPayment reference: R123456\nPlease pay by 5 October 2026.",
        false,
    );
    assert_eq!(
        bill(&a),
        (95363, "ZAR".into(), Some("reference:r123456".into()))
    );
}

#[test]
fn money_written_after_a_label_stays_money() {
    let a = judge("Invoice R953.63 is due by 5 October 2026.", false);
    assert_eq!(bill(&a), (95363, "ZAR".into(), None));
}

#[test]
fn a_reference_with_no_digit_is_ordinary_prose() {
    let a = judge(
        "Amount due: R953.63\nPayment reference: please quote your name\nPlease pay by 5 October 2026.",
        false,
    );
    assert_eq!(bill(&a).2, None);
}

#[test]
fn the_billers_bank_account_is_not_the_bill() {
    assert_eq!(
        bill(&judge(&bank_bill("A1234", "R953.63"), false)),
        (95363, "ZAR".into(), Some("reference:a1234".into()))
    );
}

#[test]
fn a_dotted_reference_is_read_whole() {
    let read = |reference| bill(&judge(&bank_bill(reference, "R953.63"), false)).2;
    assert_eq!(read("A1234.01"), Some("reference:a1234.01".into()));
    let sentence = judge("Account A1234. Amount due £25 by 5 October 2026.", false);
    assert_eq!(sentence.facts.references[0].value, "a1234");
}

#[test]
fn the_customers_own_account_survives_a_bank_block_that_has_ended() {
    let a = judge(
        "Pay into: FNB\nAccount number: 62012345678\n\nAccount number: 77665544\nAmount due: R953.63\nPlease pay by 5 October 2026.",
        false,
    );
    assert_eq!(bill(&a).2, Some("account:77665544".into()));
}

#[test]
fn an_account_named_only_in_the_subject_identifies_the_bill() {
    let body =
        "Your levy invoice is attached.\nAmount due: R953.63\nPlease pay by 30 October 2026.";
    let read = |subject| {
        bill(&judge_complete(
            body,
            false,
            RecipientRole::AloneTo,
            false,
            false,
            subject,
        ))
        .2
    };
    assert_eq!(
        read("Levy invoice - Account A1234"),
        Some("account:a1234".into())
    );
}

#[test]
fn a_subject_account_does_not_merge_two_bills_in_one_mail() {
    let a = judge_complete(
        "Levy amount due R100.00 by 30 October 2026.\n\nWater amount due R200.00 by 30 October 2026.",
        false,
        RecipientRole::AloneTo,
        false,
        false,
        "Statement for Account A1234",
    );
    let bills: Vec<_> = a
        .units
        .iter()
        .filter(|u| u.answer == AskAnswer::AsksYou)
        .collect();
    assert_eq!(bills.len(), 2, "{a:#?}");
    assert!(bills
        .iter()
        .all(|u| u.tags.as_ref().expect("tags").reference.is_none()));
}

#[test]
fn clear_debt_notices_ask_for_payment() {
    for (body, minor) in [
        (
            "Please settle the current arrears of R12 500.00 immediately.",
            1250000,
        ),
        (
            "Your account is overdue. Amount outstanding: R953.63.",
            95363,
        ),
    ] {
        assert_eq!(bill(&judge(body, false)).0, minor, "{body}");
    }
}

#[test]
fn words_near_debt_that_ask_for_nothing_stay_quiet() {
    for body in [
        "Your account is not in arrears.",
        "You have no arrears of R500.00 now.",
        "Amount outstanding: R953.63.",
        "Your account is overdue for a review.",
    ] {
        let a = judge(body, false);
        assert!(!answers_you(&a), "{body}: {a:#?}");
    }
}

#[test]
fn a_blank_line_ends_the_banking_details_however_it_is_written() {
    for blank in ["\r\n\r\n", "\n   \t\n", "\r\n \t\r\n", "\n\n"] {
        let body = format!(
            "Bank details:\r\nAccount number: 000111{blank}Invoice for Account A1234: amount due R100.00 by 30 October 2026."
        );
        let a = judge(&body, false);
        assert_eq!(
            bill(&a),
            (10000, "ZAR".into(), Some("account:a1234".into())),
            "{blank:?}"
        );
        assert_eq!(a.facts.references.len(), 1, "{blank:?}: {a:#?}");
        let found = &a.facts.references[0];
        assert_eq!(
            &a.facts.zones[found.zone].text[found.span.start..found.span.end],
            "Account A1234",
            "{blank:?}"
        );
    }
}

#[test]
fn the_billers_account_stays_inside_its_block_across_crlf_lines() {
    let a = judge(
        "Bank details:\r\nBranch: Central\r\nAccount number: 000111\r\nAmount due: R100.00\r\nPlease pay by 30 October 2026.",
        false,
    );
    assert!(a.facts.references.is_empty(), "{a:#?}");
}

#[test]
fn a_reference_line_with_its_own_bill_starts_a_new_bill() {
    let a = judge(
        "Levy invoice A123: amount due R100.00 by 30 October 2026.\nPayment reference B456 for water: amount due R200.00 by 30 October 2026.",
        false,
    );
    let bills: Vec<_> = a
        .units
        .iter()
        .filter(|u| u.answer == AskAnswer::AsksYou)
        .map(|u| {
            let tags = u.tags.as_ref().expect("tags");
            let reference = &a.facts.references[tags.reference.expect("reference")];
            (
                a.facts.amounts[tags.amount.expect("amount")].minor,
                format!("{}:{}", reference.kind, reference.value),
            )
        })
        .collect();
    assert_eq!(
        bills,
        [
            (10000, "invoice:a123".to_string()),
            (20000, "reference:b456".to_string())
        ],
        "{a:#?}"
    );
}

#[test]
fn every_field_label_with_its_own_amount_starts_a_new_bill() {
    for line in [
        "Payment reference B456 for water: amount due R200.00",
        "Reference number B456 for water: amount due R200.00",
        "Reference: B456 for water: amount due R200.00",
        "Ref: B456 for water: amount due R200.00",
        "Amount outstanding for water: R200.00",
        "Amount owed for water: R200.00",
        "Balance outstanding for water: R200.00",
    ] {
        let a = judge(
            &format!(
                "Levy invoice A123: amount due R100.00 by 30 October 2026.\n{line} by 30 October 2026."
            ),
            false,
        );
        let bills = a
            .units
            .iter()
            .filter(|u| u.answer == AskAnswer::AsksYou)
            .count();
        assert_eq!(bills, 2, "{line}: {a:#?}");
    }
}

#[test]
fn a_line_that_only_adds_a_reference_stays_with_its_bill() {
    for line in [
        "Payment reference: R123456",
        "Reference number: B456",
        "Reference: B456",
        "Ref: B456",
    ] {
        let a = judge(
            &format!("Levy invoice: amount due R100.00 by 30 October 2026.\n{line}"),
            false,
        );
        let (minor, _, reference) = bill(&a);
        assert_eq!(minor, 10000, "{line}");
        assert!(reference.is_some(), "{line}: {a:#?}");
    }
}

fn answers_you(a: &AskAssessment) -> bool {
    a.units.iter().any(|u| u.answer == AskAnswer::AsksYou)
}
