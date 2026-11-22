//! Which amount, account and deadline a bill's words establish.
use super::tests::{judge, judge_complete};
use super::*;

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
fn the_billers_bank_account_is_not_the_bill() {
    assert_eq!(
        bill(&judge(&bank_bill("A1234", "R953.63"), false)),
        (95363, "ZAR".into(), Some("reference:a1234".into()))
    );
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
