//! Mail kinds: who a message is from, as far as treatment goes (a person,
//! a newsletter, a machine, or someone screened out), which rule decided it,
//! and the reason to show for it.
//!
//! Pure functions, shared by the desk (which keeps non-people mail off it)
//! and by Reading and Paper trail (which hold that mail). The user's choice
//! for a sender, stored as a screener decision, always wins; after it come
//! the automatic rules, most specific first. Rule-based on purpose: every
//! placement can say exactly why.

use mxr_protocol::{KindRuleData, MailKindData, SenderKindData};
use mxr_store::ScreenerDisposition;

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

/// A machine's address, not a person's: newsletters, notifications and
/// no-reply senders alike.
pub(super) fn looks_automated(email: &str) -> bool {
    address_hint(email).is_some()
}

/// Everything about one message and its sender that decides its kind.
#[derive(Debug, Clone, Copy)]
pub(super) struct KindSignals<'a> {
    pub email: &'a str,
    pub has_list_id: bool,
    pub has_unsubscribe: bool,
    pub is_delivery: bool,
    pub is_invite: bool,
    /// The contacts table knows this sender writes to lists.
    pub list_sender: bool,
    /// The user's choice for this sender, if any.
    pub decision: Option<ScreenerDisposition>,
}

/// A message's kind and the rule that decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Classification {
    pub kind: SenderKind,
    pub rule: KindRuleData,
}

pub(super) fn classify(signals: &KindSignals<'_>) -> Classification {
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
    if signals.has_list_id {
        return list(KindRuleData::ListId);
    }
    if signals.has_unsubscribe {
        return list(KindRuleData::ListUnsubscribe);
    }
    if hint == Some(AddressHint::NoReply) {
        return automated(KindRuleData::NoReplyAddress);
    }
    if signals.list_sender {
        return list(KindRuleData::ListSender);
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
        KindRuleData::ListId if signals.has_unsubscribe => {
            "mailing list, has List-Unsubscribe".to_string()
        }
        KindRuleData::ListId => "mailing list".to_string(),
        KindRuleData::ListUnsubscribe => "has List-Unsubscribe".to_string(),
        KindRuleData::ListSender => "sends to mailing lists".to_string(),
        KindRuleData::Person => "from a person".to_string(),
    }
}

/// The wire form: kind, rule, reason and whether the user chose it.
pub(super) fn describe(signals: &KindSignals<'_>) -> MailKindData {
    let classification = classify(signals);
    MailKindData {
        kind: classification.kind.to_data(),
        rule: classification.rule,
        reason: reason(signals, classification),
        corrected: classification.rule == KindRuleData::Decision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(email: &str) -> KindSignals<'_> {
        KindSignals {
            email,
            has_list_id: false,
            has_unsubscribe: false,
            is_delivery: false,
            is_invite: false,
            list_sender: false,
            decision: None,
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
    fn a_plain_address_is_a_person() {
        let described = describe(&signals("maya@orbit.example"));
        assert_eq!(described.kind, SenderKindData::People);
        assert_eq!(described.rule, KindRuleData::Person);
        assert!(!looks_automated("editor@news.com"));
    }
}
