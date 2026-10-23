use super::candidates::inside;
use mxr_core::asks::*;
use once_cell::sync::Lazy;
use regex::Regex;

macro_rules! pattern {
    ($name:ident, $pattern:literal) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($pattern).expect("document regex"));
    };
}
pattern!(
    COMPLETE,
    r"(?i)\b(payment (?:received|confirmed|successful|complete)|has been paid|already paid|nothing outstanding|no (?:payment|amount) (?:is )?due|now resolved|successfully retried)\b"
);
pattern!(
    CREDIT,
    r"(?i)\b(credit note|refund|credit balance|in credit)\b"
);
pattern!(
    OFFER,
    r"(?i)\b(special offer|discount|sale|save \d+%|why not|if you(?:'d| would) like|set up (?:a |your )?direct debit|register (?:for|with) (?:our|the) app)\b"
);
pattern!(
    AUTO,
    r"(?i)\b(will be (?:collected|taken|debited|charged)|we(?:'ll| will) collect|automatically (?:paid|collected)|autopay|scheduled (?:direct debit|payment)|direct debit (?:will|is scheduled))\b"
);
pattern!(
    FAILED,
    r"(?i)\b(payment (?:has )?(?:failed|declined|was declined|was unsuccessful)|card (?:was )?declined|unpaid direct debit|update your (?:card|payment details|payment method))\b"
);
pattern!(
    PAST,
    r"(?i)\b(last month|previously|used to|had failed|was resolved)\b"
);
pattern!(
    BILL,
    r"(?i)\b(invoice|bill|amount (?:due|owed|outstanding)|balance (?:due|outstanding)|payment (?:is )?(?:due|overdue)|minimum payment|please pay|pay (?:by|now)|past due)\b"
);
pattern!(STATEMENT, r"(?i)\b(statement|minimum payment)\b");
pattern!(MINIMUM, r"(?i)\bminimum (?:payment|amount)\b");
pattern!(NOW, r"(?i)\b(now|immediately|overdue|past due)\b");

pub(super) fn read(text: &str, zone: usize, span: &AskSpan, facts: &AskFacts) -> DocumentReading {
    let has = |regex: &Regex| regex.is_match(text);
    let (mut kind, mut status) = if has(&COMPLETE) {
        (DocumentKind::PaymentConfirmation, PaymentStatus::Complete)
    } else if has(&CREDIT) {
        (DocumentKind::Credit, PaymentStatus::Credit)
    } else if has(&OFFER) {
        (DocumentKind::Offer, PaymentStatus::None)
    } else if has(&AUTO) {
        (DocumentKind::AutomaticPayment, PaymentStatus::Automatic)
    } else if has(&FAILED) {
        (
            DocumentKind::FailedPayment,
            if has(&PAST) {
                PaymentStatus::Complete
            } else {
                PaymentStatus::Failed
            },
        )
    } else if has(&BILL) {
        (
            if has(&STATEMENT) {
                DocumentKind::Statement
            } else {
                DocumentKind::Bill
            },
            if has(&NOW) {
                PaymentStatus::PastDue
            } else {
                PaymentStatus::Due
            },
        )
    } else {
        (DocumentKind::Other, PaymentStatus::None)
    };
    let amounts: Vec<_> = facts
        .amounts
        .iter()
        .filter(|a| inside(zone, span, a.zone, &a.span))
        .collect();
    if matches!(kind, DocumentKind::Bill | DocumentKind::Statement)
        && !amounts.is_empty()
        && amounts.iter().all(|a| a.minor <= 0)
    {
        kind = DocumentKind::Credit;
        status = PaymentStatus::Credit;
    }
    let unique_positive = amounts.first().is_some_and(|first| {
        first.minor > 0
            && amounts
                .iter()
                .all(|a| a.minor == first.minor && a.currency == first.currency)
    });
    let dates: Vec<_> = facts
        .dates
        .iter()
        .filter(|d| d.checked && inside(zone, span, d.zone, &d.span))
        .collect();
    let has_date = dates
        .first()
        .is_some_and(|first| dates.iter().all(|d| d.at == first.at));
    let required_fields_present = match kind {
        DocumentKind::Bill => unique_positive && (has_date || has(&NOW)),
        DocumentKind::Statement => unique_positive && has_date && has(&MINIMUM),
        DocumentKind::FailedPayment => status == PaymentStatus::Failed,
        _ => false,
    };
    DocumentReading {
        kind,
        status,
        required_fields_present,
    }
}
