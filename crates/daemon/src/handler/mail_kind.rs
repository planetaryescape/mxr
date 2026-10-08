//! Mail kinds: who a message is from, as far as treatment goes (a person,
//! a newsletter, a machine, or someone screened out), which rule decided it,
//! and the reason to show for it.
//!
//! Pure functions, shared by the desk (which keeps non-people mail off it)
//! and by Reading and Paper trail (which hold that mail). The user's choice
//! for one email (a move, `X`) wins, then the user's choice for a sender,
//! stored as a screener decision; after them come the automatic rules, most
//! specific first. Rule-based on purpose: every placement can say exactly
//! why.

use mxr_protocol::{KindRuleData, MailKindData, ModeKindData, SenderKindData};
use mxr_store::{DeskContact, ScreenerDisposition};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

/// Local parts of machines that notify: receipts, alerts, notifications.
/// Their mail is paper trail even when it carries list headers (GitHub's
/// notifications have List-Unsubscribe).
const NOTIFYING_LOCAL_PARTS: &[&str] = &[
    "notifications",
    "notification",
    "notify",
    "alerts",
    "alert",
    "automated",
    "mailer-daemon",
    "postmaster",
    "bounce",
    "receipts",
    "receipt",
    "billing",
    "invoice",
    "shipment",
    "tracking",
    "verify",
    "verification",
    "security",
    // Banks' card and account alerts (`inContact@`, `transactions@`,
    // `statements@`).
    "incontact",
    "transaction",
    "statement",
];

/// Local parts of newsletters.
const NEWSLETTER_LOCAL_PARTS: &[&str] = &["newsletter", "digest"];

/// Local parts that only say "don't answer": receipts and newsletters both
/// use them, so list headers decide which (marketing must carry
/// List-Unsubscribe; transactional mail usually doesn't).
const NO_REPLY_LOCAL_PARTS: &[&str] = &[
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "do_not_reply",
];

/// Local parts of a role, not a person: a team, a desk or a function
/// (`forex@`, `hello@`, `support@`). Matched against the whole local part
/// (before any `+tag`) or its first word (`team-uk@`). Only decisive for a
/// sender you have never written to: a support desk you correspond with is
/// someone you talk to.
const ROLE_LOCAL_PARTS: &[&str] = &[
    "forex",
    "fx",
    "team",
    "hello",
    "hi",
    "hey",
    "info",
    "support",
    "help",
    "helpdesk",
    "contact",
    "contactus",
    "service",
    "services",
    "customerservice",
    "customercare",
    "care",
    "admin",
    "account",
    "accounts",
    "marketing",
    "sales",
    "order",
    "orders",
    "payment",
    "payments",
    "members",
    "membership",
    "community",
    "welcome",
    "updates",
    "news",
    "office",
    "enquiries",
    "inquiries",
    "feedback",
    "developer",
    "store",
    "shop",
    "bookings",
    "booking",
    "reservations",
    "rewards",
    "offers",
    "events",
    "founders",
];

/// A sender's latest subjects needed before their variety counts.
const TEMPLATED_MIN_SUBJECTS: usize = 5;

/// Subdomain labels of notifying hosts (`alerts.example.com`).
const NOTIFYING_DOMAIN_LABELS: &[&str] = &[
    "alerts",
    "alert",
    "notifications",
    "notification",
    "notify",
    "bounce",
    "bounces",
    "mailer",
];

/// Subdomain labels of newsletter hosts (`news.example.com`).
const NEWSLETTER_DOMAIN_LABELS: &[&str] = &["newsletter", "news", "updates", "marketing"];

/// How a sender's mail is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SenderKind {
    Person,
    /// Newsletters and mailing lists: Reading.
    List,
    /// Receipts, notifications, deliveries, invites: Paper trail.
    Automated,
    /// Screened out by the user: never on the desk or in a place.
    Denied,
}

impl SenderKind {
    pub(super) const fn to_data(self) -> SenderKindData {
        match self {
            Self::Person => SenderKindData::People,
            Self::List => SenderKindData::Reading,
            Self::Automated => SenderKindData::PaperTrail,
            Self::Denied => SenderKindData::ScreenedOut,
        }
    }
}

/// The kind a move of one email to `mode` gives it. To do and Archive add
/// the email there instead of moving it, so they have no kind.
pub(super) const fn kind_for_mode(mode: ModeKindData) -> Option<SenderKind> {
    match mode {
        ModeKindData::Messages => Some(SenderKind::Person),
        ModeKindData::Updates => Some(SenderKind::Automated),
        ModeKindData::Reading => Some(SenderKind::List),
        ModeKindData::Todo | ModeKindData::Archive => None,
    }
}

/// A stored move (`arrivals.now_mode`) as the kind it gives.
pub(super) fn kind_for_stored_mode(mode: &str) -> Option<SenderKind> {
    ModeKindData::parse(mode).and_then(kind_for_mode)
}

/// The screener decision that puts a sender in `kind`.
pub(super) const fn disposition_for(kind: SenderKindData) -> ScreenerDisposition {
    match kind {
        SenderKindData::People => ScreenerDisposition::Allow,
        SenderKindData::Reading => ScreenerDisposition::Feed,
        SenderKindData::PaperTrail => ScreenerDisposition::PaperTrail,
        SenderKindData::ScreenedOut => ScreenerDisposition::Deny,
    }
}

/// The kind a screener decision chose; `Unknown` chose none.
pub(super) const fn kind_for(disposition: ScreenerDisposition) -> Option<SenderKindData> {
    match disposition {
        ScreenerDisposition::Allow => Some(SenderKindData::People),
        ScreenerDisposition::Feed => Some(SenderKindData::Reading),
        ScreenerDisposition::PaperTrail => Some(SenderKindData::PaperTrail),
        ScreenerDisposition::Deny => Some(SenderKindData::ScreenedOut),
        ScreenerDisposition::Unknown => None,
    }
}

/// What an address alone says about its sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddressHint {
    Notifying(KindRuleData),
    Newsletter(KindRuleData),
    NoReply,
}

fn address_hint(email: &str) -> Option<AddressHint> {
    let email = email.trim().to_ascii_lowercase();
    let (local, domain) = email.split_once('@').unwrap_or((email.as_str(), ""));
    let local_has = |parts: &[&str]| !local.is_empty() && parts.iter().any(|p| local.contains(p));
    // Only subdomain labels: the registrable name itself ("news.com") can
    // belong to a person's employer.
    let labels: Vec<&str> = domain.split('.').collect();
    let subdomains = &labels[..labels.len().saturating_sub(2)];
    let subdomain_in = |set: &[&str]| subdomains.iter().any(|label| set.contains(label));
    if local_has(NOTIFYING_LOCAL_PARTS) {
        Some(AddressHint::Notifying(KindRuleData::AutomatedAddress))
    } else if local_has(NEWSLETTER_LOCAL_PARTS) {
        Some(AddressHint::Newsletter(KindRuleData::NewsletterAddress))
    } else if subdomain_in(NOTIFYING_DOMAIN_LABELS) {
        Some(AddressHint::Notifying(KindRuleData::AutomatedDomain))
    } else if subdomain_in(NEWSLETTER_DOMAIN_LABELS) {
        Some(AddressHint::Newsletter(KindRuleData::NewsletterDomain))
    } else if local_has(NO_REPLY_LOCAL_PARTS) {
        Some(AddressHint::NoReply)
    } else {
        None
    }
}

/// A money amount with two decimals and its currency: "R437.77",
/// "£1,204.50", "USD 12.00".
static AMOUNT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:\b(?:R|ZAR|USD|GBP|EUR|KES|NGN|ZWL|US\$)\s?|[$£€¥₹])\d{1,3}(?:[,\s]?\d{3})*\.\d{2}\b",
    )
    .expect("valid regex")
});

/// What a bank or card says happened to the money.
static TRANSACTION_WORDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:reserved|purchases?|purchased|paid|payments?|debit(?:ed)?|credit(?:ed)?|withdrawal|withdrawn|transactions?|transfer(?:red)?|deposit(?:ed)?|spent|charged|refund(?:ed)?|declined|authori[sz]ed)\b",
    )
    .expect("valid regex")
});

/// A masked card or account number: "card..6131", "a/c ..748989",
/// "**** 1234", "ending in 1234".
static MASKED_NUMBER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:\.\.\s?|\*{2,}\s?|\bx{2,}|\bending(?:\s+in)?\s+)\d{3,}")
        .expect("valid regex")
});

/// A bank or card alert's subject: an amount with what happened to it, or
/// with a masked card or account number. A person's reply ("Re: paid
/// £40.00?") is never one: alerts are not replies.
fn transaction_alert(subject: &str) -> bool {
    let subject = subject.trim_start();
    let lower = subject.to_ascii_lowercase();
    if ["re:", "fw:", "fwd:"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return false;
    }
    AMOUNT.is_match(subject)
        && (TRANSACTION_WORDS.is_match(subject) || MASKED_NUMBER.is_match(subject))
}

fn role_address(email: &str) -> bool {
    let email = email.trim().to_ascii_lowercase();
    let local = email
        .split_once('@')
        .map_or(email.as_str(), |(local, _)| local);
    let local = local.split('+').next().unwrap_or(local);
    let first_word = local.split(['.', '-', '_']).next().unwrap_or(local);
    ROLE_LOCAL_PARTS.contains(&local) || ROLE_LOCAL_PARTS.contains(&first_word)
}

/// A subject with what changes between a template's sends taken out: reply
/// prefixes, numbers, amounts and dates.
fn subject_template(subject: &str) -> String {
    let mut rest = subject.trim();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(prefix) = ["re:", "fwd:", "fw:", "aw:"]
            .iter()
            .find(|prefix| lower.starts_with(*prefix))
        else {
            break;
        };
        rest = rest[prefix.len()..].trim_start();
    }
    let mut template = String::with_capacity(rest.len());
    let mut in_number = false;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            if !in_number {
                template.push('#');
            }
            in_number = true;
        } else if in_number && matches!(c, '.' | ',') {
            // "1,204.50" is one number.
        } else {
            in_number = false;
            template.extend(c.to_lowercase());
        }
    }
    template.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a sender's history says about them, beyond any one message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SenderFacts {
    /// You have written to them.
    pub wrote_to: bool,
    /// Their latest message came through bulk-mail infrastructure.
    pub bulk_headers: bool,
    /// Their latest subjects are a few templates repeated ("Payment
    /// Confirmation", "Foreign Payment", over and over).
    pub templated: bool,
}

impl SenderFacts {
    /// From the contact row and the history its loader read. No contact
    /// row: nothing is known, so nothing here decides.
    pub(super) fn of(contact: Option<&DeskContact>) -> Self {
        let Some(contact) = contact else {
            return Self::default();
        };
        let subjects = &contact.history.subjects;
        let templates: HashSet<String> = subjects
            .iter()
            .map(|subject| subject_template(subject))
            .collect();
        Self {
            wrote_to: contact.total_outbound > 0,
            bulk_headers: contact.history.bulk_headers,
            templated: subjects.len() >= TEMPLATED_MIN_SUBJECTS
                && templates.len() * 2 <= subjects.len(),
        }
    }
}

/// A machine's address, not a person's: newsletters, notifications and
/// no-reply senders alike.
pub(super) fn looks_automated(email: &str) -> bool {
    address_hint(email).is_some()
}

/// Everything about one message and its sender that decides its kind.
#[derive(Debug, Clone, Copy)]
pub(super) struct KindSignals<'a> {
    pub email: &'a str,
    pub subject: &'a str,
    pub has_list_id: bool,
    pub has_unsubscribe: bool,
    pub is_delivery: bool,
    pub is_invite: bool,
    /// The contacts table knows this sender writes to lists.
    pub list_sender: bool,
    /// What the sender's history says.
    pub sender: SenderFacts,
    /// The user's choice for this sender, if any.
    pub decision: Option<ScreenerDisposition>,
    /// You have written to this sender, anywhere in the account.
    pub written_to: bool,
    /// You are in To or Bcc, not only copied.
    pub addressed: bool,
    /// The user moved this one email (`X`) and no later sender decision
    /// overrode it.
    pub moved: Option<SenderKind>,
}

/// A message's kind and the rule that decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Classification {
    pub kind: SenderKind,
    pub rule: KindRuleData,
}

pub(super) fn classify(signals: &KindSignals<'_>) -> Classification {
    if let Some(kind) = signals.moved {
        return Classification {
            kind,
            rule: KindRuleData::Moved,
        };
    }
    let decided = |kind| Classification {
        kind,
        rule: KindRuleData::Decision,
    };
    match signals.decision {
        Some(ScreenerDisposition::Deny) => return decided(SenderKind::Denied),
        Some(ScreenerDisposition::Feed) => return decided(SenderKind::List),
        Some(ScreenerDisposition::PaperTrail) => return decided(SenderKind::Automated),
        Some(ScreenerDisposition::Allow) => return decided(SenderKind::Person),
        Some(ScreenerDisposition::Unknown) | None => {}
    }
    let automated = |rule| Classification {
        kind: SenderKind::Automated,
        rule,
    };
    let list = |rule| Classification {
        kind: SenderKind::List,
        rule,
    };
    if signals.is_delivery {
        return automated(KindRuleData::Delivery);
    }
    if signals.is_invite {
        return automated(KindRuleData::Invite);
    }
    let hint = address_hint(signals.email);
    match hint {
        Some(AddressHint::Notifying(rule)) => return automated(rule),
        Some(AddressHint::Newsletter(rule)) => return list(rule),
        Some(AddressHint::NoReply) | None => {}
    }
    // Never sorted away (D119, N1): mail addressed to you from someone
    // you've written to reaches Messages over any list or no-reply rule.
    // The rule is named only where it overrode one, so a plain person's
    // mail keeps its own reason.
    let never_bury = signals.written_to && signals.addressed;
    let unless_never_bury = |classification: Classification| {
        if never_bury {
            Classification {
                kind: SenderKind::Person,
                rule: KindRuleData::WrittenTo,
            }
        } else {
            classification
        }
    };
    if signals.has_list_id {
        return unless_never_bury(list(KindRuleData::ListId));
    }
    if signals.has_unsubscribe {
        return unless_never_bury(list(KindRuleData::ListUnsubscribe));
    }
    if transaction_alert(signals.subject) {
        return automated(KindRuleData::TransactionAlert);
    }
    if hint == Some(AddressHint::NoReply) {
        return unless_never_bury(automated(KindRuleData::NoReplyAddress));
    }
    if signals.list_sender {
        return unless_never_bury(list(KindRuleData::ListSender));
    }
    // Person mail is mail from someone you write to, or from someone who
    // writes like a person: their own address, no bulk-mail service, and
    // subjects that vary. A sender you never answered fails on any one.
    if !signals.sender.wrote_to {
        if role_address(signals.email) {
            return automated(KindRuleData::RoleAddress);
        }
        if signals.sender.bulk_headers {
            return automated(KindRuleData::BulkSender);
        }
        if signals.sender.templated {
            return automated(KindRuleData::TemplatedSender);
        }
    }
    Classification {
        kind: SenderKind::Person,
        rule: KindRuleData::Person,
    }
}

/// The reason to show for a classification, in plain words. A machine's
/// mail also says what list headers it carries, since that is what makes a
/// one-key unsubscribe possible.
fn reason(signals: &KindSignals<'_>, classification: Classification) -> String {
    let list_headers = if signals.has_unsubscribe {
        ", has List-Unsubscribe"
    } else if signals.has_list_id {
        ", sent to a mailing list"
    } else {
        ""
    };
    match classification.rule {
        KindRuleData::Decision => match classification.kind {
            SenderKind::Person => "you marked this sender as a person",
            SenderKind::List => "you moved this sender to Reading",
            SenderKind::Automated => "you moved this sender to Paper trail",
            SenderKind::Denied => "you screened this sender out",
        }
        .to_string(),
        KindRuleData::Delivery => "delivery update".to_string(),
        KindRuleData::Invite => "calendar invite".to_string(),
        KindRuleData::AutomatedAddress => format!("automated sender{list_headers}"),
        KindRuleData::AutomatedDomain => format!("automated sending domain{list_headers}"),
        KindRuleData::NewsletterAddress => format!("newsletter address{list_headers}"),
        KindRuleData::NewsletterDomain => format!("newsletter sending domain{list_headers}"),
        KindRuleData::NoReplyAddress => "no-reply sender, no list headers".to_string(),
        KindRuleData::TransactionAlert => "bank or card transaction alert".to_string(),
        KindRuleData::RoleAddress => "a role address you have never written to".to_string(),
        KindRuleData::BulkSender => {
            "sent through a bulk-mail service, and you have never written to them".to_string()
        }
        KindRuleData::TemplatedSender => {
            "the same few subjects every time, and you have never written to them".to_string()
        }
        KindRuleData::ListId if signals.has_unsubscribe => {
            "mailing list, has List-Unsubscribe".to_string()
        }
        KindRuleData::ListId => "mailing list".to_string(),
        KindRuleData::ListUnsubscribe => "has List-Unsubscribe".to_string(),
        KindRuleData::ListSender => "sends to mailing lists".to_string(),
        KindRuleData::Person => "from a person".to_string(),
        KindRuleData::Copied => COPIED_REASON.to_string(),
        KindRuleData::WrittenTo => WRITTEN_TO_REASON.to_string(),
        KindRuleData::Moved => "you moved this email".to_string(),
    }
}

/// The rule in one word, for an arrival's line: "→ Updates · automated".
pub(super) const fn rule_tag(rule: KindRuleData) -> &'static str {
    match rule {
        KindRuleData::Decision => "chosen",
        KindRuleData::Delivery => "delivery",
        KindRuleData::Invite => "invite",
        KindRuleData::AutomatedAddress | KindRuleData::AutomatedDomain => "automated",
        KindRuleData::NewsletterAddress | KindRuleData::NewsletterDomain => "newsletter",
        KindRuleData::ListId | KindRuleData::ListUnsubscribe | KindRuleData::ListSender => "list",
        KindRuleData::NoReplyAddress => "no-reply",
        KindRuleData::TransactionAlert => "bank alert",
        KindRuleData::RoleAddress => "role address",
        KindRuleData::BulkSender => "bulk sender",
        KindRuleData::TemplatedSender => "templated",
        KindRuleData::Person => "person",
        KindRuleData::Copied => "copied",
        KindRuleData::WrittenTo => "written to",
        KindRuleData::Moved => "moved",
    }
}

/// Why mail from someone you've written to is in Messages (N1).
pub(super) const WRITTEN_TO_REASON: &str = "addressed to you by someone you've written to";

/// Why a person's mail is in Updates: the thread's shape, not the sender.
pub(super) const COPIED_REASON: &str = "copied to you, or sent to a crowd, and not your turn";

/// The wire form: kind, rule, reason and whether the user chose it.
pub(super) fn describe(signals: &KindSignals<'_>) -> MailKindData {
    let classification = classify(signals);
    MailKindData {
        kind: classification.kind.to_data(),
        rule: classification.rule,
        reason: reason(signals, classification),
        corrected: matches!(
            classification.rule,
            KindRuleData::Decision | KindRuleData::Moved
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(email: &str) -> KindSignals<'_> {
        KindSignals {
            email,
            subject: "",
            sender: SenderFacts::default(),
            has_list_id: false,
            has_unsubscribe: false,
            is_delivery: false,
            is_invite: false,
            list_sender: false,
            decision: None,
            written_to: false,
            addressed: true,
            moved: None,
        }
    }

    #[test]
    fn notifying_senders_are_paper_trail_whatever_their_list_headers() {
        let github = KindSignals {
            has_unsubscribe: true,
            has_list_id: true,
            ..signals("notifications@github.com")
        };
        let described = describe(&github);
        assert_eq!(described.kind, SenderKindData::PaperTrail);
        assert_eq!(described.rule, KindRuleData::AutomatedAddress);
        assert_eq!(described.reason, "automated sender, has List-Unsubscribe");
        assert!(!described.corrected);

        // Account security mail is a machine's, however it's addressed.
        let verify = describe(&signals("verify@security-mail.example.com"));
        assert_eq!(verify.kind, SenderKindData::PaperTrail);
        assert_eq!(verify.rule, KindRuleData::AutomatedAddress);

        let alerts = describe(&signals("uptime@alerts.example.com"));
        assert_eq!(alerts.rule, KindRuleData::AutomatedDomain);
        assert_eq!(alerts.reason, "automated sending domain");
    }

    #[test]
    fn a_no_reply_sender_is_decided_by_its_list_headers() {
        let receipt = describe(&signals("no-reply@shop.example"));
        assert_eq!(receipt.kind, SenderKindData::PaperTrail);
        assert_eq!(receipt.rule, KindRuleData::NoReplyAddress);
        assert_eq!(receipt.reason, "no-reply sender, no list headers");

        let marketing = describe(&KindSignals {
            has_unsubscribe: true,
            ..signals("no-reply@shop.example")
        });
        assert_eq!(marketing.kind, SenderKindData::Reading);
        assert_eq!(marketing.rule, KindRuleData::ListUnsubscribe);
    }

    #[test]
    fn newsletter_addresses_are_reading() {
        let digest = describe(&KindSignals {
            has_unsubscribe: true,
            ..signals("digest@hn.example")
        });
        assert_eq!(digest.kind, SenderKindData::Reading);
        assert_eq!(digest.rule, KindRuleData::NewsletterAddress);
        assert_eq!(digest.reason, "newsletter address, has List-Unsubscribe");
        let news = describe(&signals("editor@news.example.com"));
        assert_eq!(news.rule, KindRuleData::NewsletterDomain);
        assert!(looks_automated("digest@hn.example"));
    }

    #[test]
    fn list_mail_is_reading_by_its_strongest_header() {
        let list = KindSignals {
            has_list_id: true,
            has_unsubscribe: true,
            ..signals("editor@weekly.example")
        };
        let described = describe(&list);
        assert_eq!(described.kind, SenderKindData::Reading);
        assert_eq!(described.rule, KindRuleData::ListId);
        assert_eq!(described.reason, "mailing list, has List-Unsubscribe");

        let unsubscribe_only = describe(&KindSignals {
            has_unsubscribe: true,
            ..signals("editor@weekly.example")
        });
        assert_eq!(unsubscribe_only.rule, KindRuleData::ListUnsubscribe);
        assert_eq!(unsubscribe_only.reason, "has List-Unsubscribe");

        let known = describe(&KindSignals {
            list_sender: true,
            ..signals("editor@weekly.example")
        });
        assert_eq!(known.rule, KindRuleData::ListSender);
    }

    #[test]
    fn deliveries_and_invites_are_paper_trail_before_anything_automatic() {
        let delivery = describe(&KindSignals {
            is_delivery: true,
            has_list_id: true,
            ..signals("maya@example.com")
        });
        assert_eq!(delivery.kind, SenderKindData::PaperTrail);
        assert_eq!(delivery.rule, KindRuleData::Delivery);
        let invite = describe(&KindSignals {
            is_invite: true,
            ..signals("maya@example.com")
        });
        assert_eq!(invite.rule, KindRuleData::Invite);
    }

    #[test]
    fn the_users_choice_beats_every_automatic_rule() {
        let robot = KindSignals {
            has_list_id: true,
            is_delivery: true,
            ..signals("no-reply@shop.example.com")
        };
        for (disposition, kind, reason) in [
            (
                ScreenerDisposition::Allow,
                SenderKindData::People,
                "you marked this sender as a person",
            ),
            (
                ScreenerDisposition::Feed,
                SenderKindData::Reading,
                "you moved this sender to Reading",
            ),
            (
                ScreenerDisposition::PaperTrail,
                SenderKindData::PaperTrail,
                "you moved this sender to Paper trail",
            ),
            (
                ScreenerDisposition::Deny,
                SenderKindData::ScreenedOut,
                "you screened this sender out",
            ),
        ] {
            let described = describe(&KindSignals {
                decision: Some(disposition),
                ..robot
            });
            assert_eq!(described.kind, kind);
            assert_eq!(described.rule, KindRuleData::Decision);
            assert_eq!(described.reason, reason);
            assert!(described.corrected);
            assert_eq!(kind_for(disposition_for(kind)), Some(kind));
        }
        // "No decision yet" is automatic.
        let unknown = describe(&KindSignals {
            decision: Some(ScreenerDisposition::Unknown),
            ..robot
        });
        assert_eq!(unknown.rule, KindRuleData::Delivery);
        assert_eq!(kind_for(ScreenerDisposition::Unknown), None);
    }

    #[test]
    fn bank_and_card_alerts_are_paper_trail() {
        // The local part alone: alert senders that say nothing about money.
        for email in [
            "inContact@bank.example",
            "transactions@card.example",
            "statements@bank.example",
        ] {
            let described = describe(&signals(email));
            assert_eq!(described.kind, SenderKindData::PaperTrail, "{email}");
            assert_eq!(described.rule, KindRuleData::AutomatedAddress, "{email}");
        }
        // The subject: an amount with what happened to it, from an address
        // that looks like anyone's.
        for subject in [
            "Bank:-) R437.77 reserved for purchase @ Example Store from a/c..123456 using card..6131",
            "You spent £12.50 at Corner Shop",
            "$1,204.00 debited from your account",
            "Card ending in 4321: USD 9.99",
        ] {
            let described = describe(&KindSignals {
                subject,
                ..signals("care@bank.example")
            });
            assert_eq!(described.kind, SenderKindData::PaperTrail, "{subject}");
            assert_eq!(described.rule, KindRuleData::TransactionAlert, "{subject}");
            assert_eq!(described.reason, "bank or card transaction alert");
        }
        // A person writing about money is still a person.
        for subject in [
            "Re: paid £40.00 for the tickets",
            "Dinner on Friday?",
            "Invoice 2026-114",
            "Can you send £40 for the tickets",
        ] {
            let described = describe(&KindSignals {
                subject,
                ..signals("maya@orbit.example")
            });
            assert_eq!(described.kind, SenderKindData::People, "{subject}");
        }
    }

    fn contact(outbound: u32, subjects: &[&str], bulk_headers: bool) -> DeskContact {
        DeskContact {
            email: "x@example.com".into(),
            display_name: None,
            first_seen_at: chrono::Utc::now(),
            total_inbound: subjects.len() as u32,
            total_outbound: outbound,
            is_list_sender: false,
            cadence_seconds: None,
            history: mxr_store::SenderHistory {
                subjects: subjects.iter().map(ToString::to_string).collect(),
                bulk_headers,
            },
        }
    }

    #[test]
    fn a_sender_you_never_wrote_to_must_write_like_a_person() {
        let never = contact(0, &["Hello"], false);
        // A role address: a desk, not a person.
        for email in [
            "forex@bank.example",
            "hello@toys.example",
            "team-uk@app.example",
            "support+uk@app.example",
        ] {
            let described = describe(&KindSignals {
                sender: SenderFacts::of(Some(&never)),
                ..signals(email)
            });
            assert_eq!(described.kind, SenderKindData::PaperTrail, "{email}");
            assert_eq!(described.rule, KindRuleData::RoleAddress, "{email}");
        }
        // Bulk-mail infrastructure on their latest message.
        let bulk = contact(0, &["Your monthly statement"], true);
        let described = describe(&KindSignals {
            sender: SenderFacts::of(Some(&bulk)),
            ..signals("statements-desk@broker.example")
        });
        assert_eq!(described.rule, KindRuleData::AutomatedAddress);
        let described = describe(&KindSignals {
            sender: SenderFacts::of(Some(&bulk)),
            ..signals("dan@broker.example")
        });
        assert_eq!(described.kind, SenderKindData::PaperTrail);
        assert_eq!(described.rule, KindRuleData::BulkSender);
        // Forty of the same two templates, numbers aside.
        let templated: Vec<String> = (0..40)
            .map(|n| {
                if n % 2 == 0 {
                    format!("Payment Confirmation {n}")
                } else {
                    format!("Re: Foreign Payment ref {}", 1000 + n)
                }
            })
            .collect();
        let templated: Vec<&str> = templated.iter().map(String::as_str).collect();
        let robot = contact(0, &templated, false);
        let described = describe(&KindSignals {
            sender: SenderFacts::of(Some(&robot)),
            ..signals("desk.officer@bank.example")
        });
        assert_eq!(described.kind, SenderKindData::PaperTrail);
        assert_eq!(described.rule, KindRuleData::TemplatedSender);
        assert_eq!(
            described.reason,
            "the same few subjects every time, and you have never written to them"
        );
    }

    #[test]
    fn someone_you_write_to_or_who_writes_like_a_person_stays_a_person() {
        // Varied subjects from their own address.
        let varied = contact(
            0,
            &[
                "Dinner on Friday?",
                "Photos from the trip",
                "Re: the flat",
                "Book recommendation",
                "Lunch next week",
                "Happy birthday!",
            ],
            false,
        );
        let described = describe(&KindSignals {
            sender: SenderFacts::of(Some(&varied)),
            ..signals("maya@orbit.example")
        });
        assert_eq!(described.kind, SenderKindData::People);
        // Too few messages to judge the subjects.
        let new = contact(0, &["Invoice 1", "Invoice 2"], false);
        assert_eq!(
            describe(&KindSignals {
                sender: SenderFacts::of(Some(&new)),
                ..signals("sam@studio.example")
            })
            .kind,
            SenderKindData::People
        );
        // You wrote to the support desk and the bulk sender: both people.
        let answered = contact(2, &["Payment Confirmation"; 10], true);
        for email in ["support@app.example", "dan@broker.example"] {
            assert_eq!(
                describe(&KindSignals {
                    sender: SenderFacts::of(Some(&answered)),
                    ..signals(email)
                })
                .kind,
                SenderKindData::People,
                "{email}"
            );
        }
        // Your choice beats every history rule.
        let robot = contact(0, &["Payment Confirmation"; 10], true);
        let described = describe(&KindSignals {
            sender: SenderFacts::of(Some(&robot)),
            decision: Some(ScreenerDisposition::Allow),
            ..signals("forex@bank.example")
        });
        assert_eq!(described.kind, SenderKindData::People);
        assert!(described.corrected);
    }

    #[test]
    fn subject_templates_set_numbers_and_reply_prefixes_aside() {
        assert_eq!(
            subject_template("RE: Fwd: Payment of R1,204.50 on 2026-10-01"),
            "payment of r# on #-#-#"
        );
        assert_eq!(subject_template("  Your   statement "), "your statement");
    }

    #[test]
    fn mail_addressed_to_you_from_someone_you_wrote_to_is_never_sorted_away() {
        // A list, a no-reply sender and a list sender all give way.
        for list in [
            KindSignals {
                has_list_id: true,
                has_unsubscribe: true,
                ..signals("maya@orbit.example")
            },
            KindSignals {
                has_unsubscribe: true,
                ..signals("no-reply@orbit.example")
            },
            KindSignals {
                list_sender: true,
                ..signals("maya@orbit.example")
            },
        ] {
            let known = describe(&KindSignals {
                written_to: true,
                ..list
            });
            assert_eq!(known.kind, SenderKindData::People);
            assert_eq!(known.rule, KindRuleData::WrittenTo);
            assert_eq!(known.reason, WRITTEN_TO_REASON);
            // Only copied: the rule doesn't apply.
            let copied = describe(&KindSignals {
                written_to: true,
                addressed: false,
                ..list
            });
            assert_ne!(copied.kind, SenderKindData::People);
            // A stranger's list mail stays Reading.
            assert_eq!(describe(&list).kind, SenderKindData::Reading);
        }
        // Plain mail from someone you wrote to keeps its own reason.
        let plain = describe(&KindSignals {
            written_to: true,
            ..signals("maya@orbit.example")
        });
        assert_eq!(plain.rule, KindRuleData::Person);
        // Your own decision and machines' addresses still win.
        let decided = describe(&KindSignals {
            written_to: true,
            decision: Some(ScreenerDisposition::Feed),
            ..signals("maya@orbit.example")
        });
        assert_eq!(decided.kind, SenderKindData::Reading);
        let machine = describe(&KindSignals {
            written_to: true,
            ..signals("notifications@github.com")
        });
        assert_eq!(machine.kind, SenderKindData::PaperTrail);
    }

    #[test]
    fn a_move_of_one_email_beats_the_senders_decision_and_every_rule() {
        let moved = describe(&KindSignals {
            moved: Some(SenderKind::List),
            decision: Some(ScreenerDisposition::Allow),
            ..signals("maya@orbit.example")
        });
        assert_eq!(moved.kind, SenderKindData::Reading);
        assert_eq!(moved.rule, KindRuleData::Moved);
        assert_eq!(moved.reason, "you moved this email");
        assert!(moved.corrected);
        assert_eq!(
            kind_for_mode(ModeKindData::Updates),
            Some(SenderKind::Automated)
        );
        assert_eq!(kind_for_mode(ModeKindData::Todo), None);
        assert_eq!(kind_for_stored_mode("messages"), Some(SenderKind::Person));
        assert_eq!(kind_for_stored_mode("archive"), None);
    }

    #[test]
    fn a_plain_address_is_a_person() {
        let described = describe(&signals("maya@orbit.example"));
        assert_eq!(described.kind, SenderKindData::People);
        assert_eq!(described.rule, KindRuleData::Person);
        assert!(!looks_automated("editor@news.com"));
    }
}
