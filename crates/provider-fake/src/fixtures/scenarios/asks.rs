//! What an email asks the owner to do, as labels (label schema v2,
//! blueprint 37, "The eval grades what the person relies on").
//!
//! An email states zero or more expected [`Item`]s. Each has a kind of ask in
//! chapter 37's vocabulary, a direction, the counterparty it names, and the
//! amount and due words when there are any. Whether a reply is owed (Your
//! turn) is its own answer on [`super::Expect`].
//!
//! A due date written from the seed clock cannot be a label (labels never
//! move with the clock), so [`Due`] states it as hours after the email's own
//! date, or as the clock-free words of it ("Friday").

use super::Who;

/// What an item asks for, in blueprint 37's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AskKind {
    Pay,
    FixPayment,
    Sign,
    Verify,
    Reply,
    Send,
    Provide,
    Upload,
    Book,
    Attend,
    RenewOrCancel,
    Register,
    Contact,
    Other,
}

impl AskKind {
    /// Every kind, in the order the eval reports them.
    pub const ALL: [Self; 14] = [
        Self::Pay,
        Self::FixPayment,
        Self::Sign,
        Self::Verify,
        Self::Reply,
        Self::Send,
        Self::Provide,
        Self::Upload,
        Self::Book,
        Self::Attend,
        Self::RenewOrCancel,
        Self::Register,
        Self::Contact,
        Self::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pay => "pay",
            Self::FixPayment => "fix_payment",
            Self::Sign => "sign",
            Self::Verify => "verify",
            Self::Reply => "reply",
            Self::Send => "send",
            Self::Provide => "provide",
            Self::Upload => "upload",
            Self::Book => "book",
            Self::Attend => "attend",
            Self::RenewOrCancel => "renew_or_cancel",
            Self::Register => "register",
            Self::Contact => "contact",
            Self::Other => "other",
        }
    }
}

/// Whose move it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    /// Inbound mail asks the owner to do something.
    AsksYou,
    /// The owner's sent mail promises something.
    YouPromised,
}

impl Direction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsksYou => "asks_you",
            Self::YouPromised => "you_promised",
        }
    }
}

/// An amount in a currency's minor unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount {
    pub minor: i64,
    /// ISO 4217 code.
    pub currency: &'static str,
}

/// The deadline an item states.
///
/// A date written from the seed clock cannot be a label (labels never move
/// with the clock), but its distance from the email's own date can: the mail
/// and its date move together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// No deadline is stated.
    None,
    /// The deadline is the calendar day (UTC, as the mail writes it) this many
    /// hours after the email's own date. Negative for a date already past.
    AfterSent(i64),
    /// Clock-free words of the deadline ("Friday"). The eval resolves them
    /// from the email's date with the to-do code's own date reader, so this
    /// grades the row's date against that reading, not an independent one.
    Words(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskProvenance {
    pub quote: String,
    pub actor: String,
    pub state: String,
}

/// One thing an email asks of the owner, or the owner promised in sent mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: AskKind,
    pub direction: Direction,
    /// The person or organisation that asked (or was promised something).
    pub counterparty: Option<String>,
    pub amount: Option<Amount>,
    pub due: Due,
    /// The account, invoice or policy number, when the email carries one.
    /// Two items of one sender with different references are two rows.
    pub reference: Option<String>,
    pub provenance: Option<AskProvenance>,
}

impl Item {
    /// An ask in inbound mail.
    pub fn asks(kind: AskKind, counterparty: &str) -> Self {
        Self::new(kind, Direction::AsksYou, counterparty)
    }

    /// A promise in the owner's sent mail.
    pub fn promised(kind: AskKind, counterparty: &str) -> Self {
        Self::new(kind, Direction::YouPromised, counterparty)
    }

    fn new(kind: AskKind, direction: Direction, counterparty: &str) -> Self {
        Self {
            kind,
            direction,
            counterparty: Some(counterparty.to_string()),
            amount: None,
            due: Due::None,
            reference: None,
            provenance: None,
        }
    }

    pub fn provenance(mut self, quote: &str, actor: &str, state: &str) -> Self {
        self.provenance = Some(AskProvenance {
            quote: quote.into(),
            actor: actor.into(),
            state: state.into(),
        });
        self
    }

    pub fn amount(mut self, minor: i64, currency: &'static str) -> Self {
        self.amount = Some(Amount { minor, currency });
        self
    }

    pub fn due(mut self, due: Due) -> Self {
        self.due = due;
        self
    }

    pub fn reference(mut self, reference: &str) -> Self {
        self.reference = Some(reference.to_string());
        self
    }
}

/// Who the sender is, for slicing the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SenderKind {
    Person,
    /// A company or institution writing as itself: a biller, a shop, a bank.
    Organisation,
    /// A service that speaks for other people's activity: a signing service,
    /// a code host, a payments or chat product.
    Platform,
    List,
}

impl SenderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organisation => "organisation",
            Self::Platform => "platform",
            Self::List => "list",
        }
    }
}

/// Domains whose mail is a platform's, not an organisation's own words.
/// A suffix match, so `accounts.google.com` and `google.com` both count.
const PLATFORM_DOMAINS: [&str; 14] = [
    "docusign.net",
    "github.com",
    "vercel.com",
    "linear.app",
    "stripe.com",
    "google.com",
    "plausible.io",
    "linearstatus.example",
    "alerts.demo.mxr.local",
    "orbit.demo.mxr.local",
    "keystone-id.demo.mxr.local",
    "workspace.demo.mxr.local",
    "events.platformhq.example",
    "acme-login.example",
];

/// The sender kind of a labelled email. `who` already separates people and
/// lists; machine mail is an organisation's unless its domain is a platform's.
pub fn sender_kind(who: Who, from_email: &str) -> SenderKind {
    match who {
        Who::Person => SenderKind::Person,
        Who::List => SenderKind::List,
        Who::Machine | Who::ScreenedOut => {
            let domain = from_email.rsplit('@').next().unwrap_or("").to_lowercase();
            if PLATFORM_DOMAINS
                .iter()
                .any(|platform| domain == *platform || domain.ends_with(&format!(".{platform}")))
            {
                SenderKind::Platform
            } else {
                SenderKind::Organisation
            }
        }
    }
}

/// Senders whose wording is American. Everything not listed here or ending in
/// `.za` is written in British English, which is what most of the corpus is.
const US_DOMAINS: [&str; 7] = [
    "amazon.com",
    "ups.com",
    "usps.com",
    "1password.com",
    "github.com",
    "vercel.com",
    "linear.app",
];

/// The locale of a sender's wording: `en-ZA` for South African domains,
/// `en-US` for the listed American ones, else `en-GB`. A scenario whose
/// domain does not say (the corpus's invented `.example` senders) names its
/// locale itself.
pub fn locale_of(from_email: &str) -> &'static str {
    let domain = from_email.rsplit('@').next().unwrap_or("").to_lowercase();
    if domain.ends_with(".za") {
        "en-ZA"
    } else if US_DOMAINS
        .iter()
        .any(|us| domain == *us || domain.ends_with(&format!(".{us}")))
    {
        "en-US"
    } else {
        "en-GB"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_chapter_37_without_repeats() {
        let names: Vec<&str> = AskKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(
            names.join(" "),
            "pay fix_payment sign verify reply send provide upload book attend \
             renew_or_cancel register contact other"
        );
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }

    #[test]
    fn a_platform_is_told_from_an_organisation_by_its_domain() {
        assert_eq!(
            sender_kind(Who::Machine, "dse@docusign.net"),
            SenderKind::Platform
        );
        assert_eq!(
            sender_kind(Who::Machine, "no-reply@accounts.google.com"),
            SenderKind::Platform
        );
        assert_eq!(
            sender_kind(Who::Machine, "billing@thameswater.co.uk"),
            SenderKind::Organisation
        );
        // A look-alike suffix is not a subdomain.
        assert_eq!(
            sender_kind(Who::Machine, "x@notgithub.com"),
            SenderKind::Organisation
        );
        assert_eq!(sender_kind(Who::Person, "a@github.com"), SenderKind::Person);
        assert_eq!(sender_kind(Who::List, "a@b.example"), SenderKind::List);
    }

    #[test]
    fn locale_follows_the_domain() {
        assert_eq!(locale_of("a@kloof.co.za"), "en-ZA");
        assert_eq!(locale_of("a@mail.amazon.com"), "en-US");
        assert_eq!(locale_of("a@thameswater.co.uk"), "en-GB");
        assert_eq!(locale_of("a@notamazon.com"), "en-GB");
    }
}
