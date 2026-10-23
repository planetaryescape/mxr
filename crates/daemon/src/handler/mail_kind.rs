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
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

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
    "billing",
    "invoice",
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

/// Explicit Messages visibility survives the copied/crowd rules. Visibility
/// alone never proves a human response or an obligation to reply.
pub(super) fn kept_in_messages(signals: &KindSignals<'_>) -> bool {
    let classification = classify(signals);
    classification.kind == SenderKind::Person
        && matches!(
            classification.rule,
            KindRuleData::CustomRule | KindRuleData::Moved | KindRuleData::Decision
        )
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

pub(super) fn no_reply_address(email: &str) -> bool {
    let lower = email.to_ascii_lowercase();
    let local = lower.split('@').next().unwrap_or_default();
    NO_REPLY_LOCAL_PARTS.iter().any(|part| local.contains(part))
}

/// A money amount with two decimals and its currency: "R437.77",
/// "£1,204.50", "USD 12.00".
static AMOUNT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:R|ZAR|USD|GBP|EUR|KES|NGN|ZWL|US\$)\s?|[$£€¥₹])\d{1,3}(?:[,\s]?\d{3})*\.\d{2}\b",
    )
    .expect("valid regex")
});

static DOCUMENT_AMOUNT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d+(?:[, ]\d{3})*\.\d{2}(?:\s+[a-z]{3})?$").expect("valid document amount")
});

/// A masked card or account number: "card..6131", "a/c ..748989",
/// "**** 1234", "ending in 1234".
static MASKED_NUMBER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:\.\.\s?|\*{2,}\s?|\bx{2,}|\bending(?:\s+in)?\s+)\d{3,}")
        .expect("valid regex")
});

/// A bank or card alert's subject: an amount with a masked card or account
/// number. Money words alone are personal correspondence too. A reply ("Re: paid
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
    AMOUNT.is_match(subject) && MASKED_NUMBER.is_match(subject)
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
    pub assessment: Option<&'a mxr_core::assessment::MailAssessment>,
    pub evidence: &'a mxr_core::mail_evidence::MailEvidence,
    pub reply_request: bool,
    pub recipients: usize,
    pub email: &'a str,
    pub subject: &'a str,
    pub has_list_id: bool,
    pub has_unsubscribe: bool,
    pub is_delivery: bool,
    pub is_invite: bool,
    /// What the sender's history says.
    pub sender: SenderFacts,
    /// The user's choice for this sender, if any.
    pub decision: Option<ScreenerDisposition>,
    /// You have written to this sender, anywhere in the account.
    pub written_to: bool,
    /// One of the account's owned addresses is positively matched in To.
    pub addressed: bool,
    /// The user moved this one email (`X`) and no later sender decision
    /// overrode it.
    pub moved: Option<SenderKind>,
    pub treatment: Option<(SenderKind, &'a str)>,
}

/// A message's kind and the rule that decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Classification {
    pub kind: SenderKind,
    pub rule: KindRuleData,
}

/// A visibility correction cannot turn an automated message into a human
/// response (and thereby clear a waiting timer).
pub(super) fn human_response(signals: &KindSignals<'_>) -> bool {
    let automatic = classify(&KindSignals {
        assessment: None,
        moved: None,
        decision: None,
        treatment: None,
        ..*signals
    });
    let purpose = purpose_evidence(signals);
    let correspondence = signals.evidence.is_reply || signals.evidence.thread_correspondence;
    (automatic.kind == SenderKind::Person && (purpose.has_authored_text || correspondence))
        || (purpose.purpose == MessagePurpose::Informational && purpose.human_reply)
}

/// Visibility, purpose, occurrence of a response and a new owner obligation
/// are independent. A user placement cannot turn an automatic CTA into debt.
pub(super) fn requires_reply(signals: &KindSignals<'_>) -> bool {
    signals.reply_request
        && classify(&KindSignals {
            assessment: None,
            moved: None,
            decision: None,
            treatment: None,
            ..*signals
        })
        .kind
            == SenderKind::Person
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessagePurpose {
    Correspondence,
    Informational,
    BillingDocument,
    Unknown,
}

pub(super) fn observed_facts(signals: &KindSignals<'_>) -> mxr_core::assessment::AssessmentFacts {
    mxr_core::assessment::AssessmentFacts {
        sender_shape: if no_reply_address(signals.email) {
            "no-reply address"
        } else if role_address(signals.email) {
            "role address"
        } else if address_hint(signals.email).is_some() {
            "notifying or list address"
        } else {
            "person-like address"
        }
        .into(),
        has_list_id: signals.has_list_id,
        has_unsubscribe: signals.has_unsubscribe,
        auto_generated: signals.evidence.auto_generated,
        bulk: signals.evidence.bulk,
        templated_sender: signals.sender.templated,
        written_to: signals.written_to || signals.sender.wrote_to,
        body_cached: signals.evidence.text.is_some(),
        recipients: signals.recipients,
        addressed: signals.addressed,
    }
}

pub(super) fn unknown_purpose(signals: &KindSignals<'_>) -> bool {
    purpose_evidence(signals).purpose == MessagePurpose::Unknown
}

struct PurposeEvidence {
    purpose: MessagePurpose,
    human_reply: bool,
    has_authored_text: bool,
}

#[derive(Clone, Copy)]
enum DocumentField {
    Reference,
    IssueDate,
    Order,
    Amount,
}

const REFERENCE_LABEL: &[&str] = &[
    "invoice",
    "document",
    "number",
    "no",
    "reference",
    "ref",
    "id",
];
const ISSUE_DATE_LABEL: &[&str] = &[
    "date", "of", "issue", "issued", "invoice", "document", "billing",
];
const ORDER_LABEL: &[&str] = &[
    "web",
    "purchase",
    "sales",
    "order",
    "number",
    "no",
    "reference",
    "ref",
    "id",
];
const AMOUNT_LABEL: &[&str] = &["total", "amount", "due", "invoice", "billing", "balance"];

/// Every field label is made only of these words, so a label containing any
/// other word cannot be a field.
fn label_word(word: &str) -> bool {
    [REFERENCE_LABEL, ISSUE_DATE_LABEL, ORDER_LABEL, AMOUNT_LABEL]
        .iter()
        .any(|allowed| allowed.contains(&word))
}

/// Labels are a small field grammar: order and qualifiers vary, but the
/// independent reference/date/order/amount families must still be present.
fn document_field(label: &str) -> Option<DocumentField> {
    let words: Vec<_> = label
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |word: &str| words.contains(&word);
    let only = |allowed: &[&str]| words.iter().all(|word| allowed.contains(word));
    let reference = has("number")
        || has("no")
        || has("reference")
        || has("ref")
        || has("id")
        || label.trim_end().ends_with('#');
    if (has("invoice") || has("document")) && reference && only(REFERENCE_LABEL) {
        Some(DocumentField::Reference)
    } else if has("date") && only(ISSUE_DATE_LABEL) {
        Some(DocumentField::IssueDate)
    } else if has("order") && reference && only(ORDER_LABEL) {
        Some(DocumentField::Order)
    } else if (has("total") || has("amount")) && only(AMOUNT_LABEL) {
        Some(DocumentField::Amount)
    } else {
        None
    }
}

fn document_value<'a>(line: &'a str, next: Option<&'a str>) -> Option<(DocumentField, &'a str)> {
    if let Some((label, value)) = line.split_once(':') {
        return Some((
            document_field(label)?,
            if value.trim().is_empty() {
                next?
            } else {
                value.trim()
            },
        ));
    }
    if let Some(field) = document_field(line) {
        return Some((field, next?));
    }
    // A label is a short run of label words, so only a split before the
    // first other word, or within the first few words, can match. Trying
    // every split of a long paragraph re-parsed it once per word: quadratic,
    // and most of a Now request.
    let label_end = label_end(line);
    line[..label_end]
        .char_indices()
        .rev()
        .filter(|(_, c)| c.is_whitespace())
        .find_map(|(index, _)| Some((document_field(&line[..index])?, line[index..].trim())))
}

/// No field label is longer than a few words ("web order reference
/// number"); a longer run of label words is prose, not a label.
const MAX_LABEL_WORDS: usize = 8;

/// Byte offset where a label must have ended: the start of the first word
/// that cannot appear in a label or of the word after `MAX_LABEL_WORDS`, or
/// the line's length. Words split on non-alphabetic characters, as in
/// `document_field`.
fn label_end(line: &str) -> usize {
    let mut start = None;
    let mut words = 0;
    for (index, c) in line.char_indices().chain([(line.len(), ' ')]) {
        match (start, c.is_alphabetic()) {
            (None, true) => {
                if words == MAX_LABEL_WORDS {
                    return index;
                }
                start = Some(index);
            }
            (Some(word_start), false) => {
                if !label_word(&line[word_start..index]) {
                    return word_start;
                }
                words += 1;
                start = None;
            }
            _ => {}
        }
    }
    line.len()
}

/// Recognize a document's field/value grammar, not an invoice subject word
/// or a sender identity. Independent reference/date/order/amount families
/// plus document-delivery framing are required.
fn billing_document(text: &str, asks_owner: bool) -> bool {
    // A record can also carry a current conversational ask. Its fields and
    // purchase framing do not erase that ask; explicit machine/list evidence
    // is handled separately by the classifier.
    if asks_owner {
        return false;
    }
    let mut reference = false;
    let mut date = false;
    let mut order = false;
    let mut amount = false;
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    for (index, line) in lines.iter().enumerate() {
        let line = line.trim_matches('*');
        let Some((field, value)) = document_value(line, lines.get(index + 1).copied()) else {
            continue;
        };
        let value = value.trim_start_matches(['#', '*', ' ']);
        if value.is_empty() || value.len() > 80 || !value.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        match field {
            DocumentField::Reference => reference = true,
            DocumentField::Order => order = true,
            DocumentField::IssueDate => {
                date |= {
                    [
                        "%Y-%m-%d",
                        "%d/%m/%Y",
                        "%m/%d/%Y",
                        "%d.%m.%Y",
                        "%d %B %Y",
                        "%d %b %Y",
                        "%B %d, %Y",
                        "%d-%b-%Y",
                    ]
                    .iter()
                    .any(|format| chrono::NaiveDate::parse_from_str(value, format).is_ok())
                };
            }
            DocumentField::Amount => {
                amount |= DOCUMENT_AMOUNT.is_match(value) || AMOUNT.is_match(value);
            }
        }
    }
    let purchase_delivery = lines.iter().any(|line| {
        line.strip_prefix("thank you for your ")
            .is_some_and(|rest| {
                rest.split(|c: char| !c.is_alphabetic())
                    .take(3)
                    .any(|word| matches!(word, "purchase" | "order"))
            })
    });
    let document_delivery = [
        "billing document",
        "invoice is attached",
        "invoice is available",
        "please find attached",
    ]
    .iter()
    .any(|phrase| text.contains(phrase));
    reference && date && (order || amount) && (purchase_delivery || document_delivery)
}

fn authored_content(evidence: &mxr_core::mail_evidence::MailEvidence) -> String {
    let Some(text) = evidence.text.as_deref() else {
        return String::new();
    };
    let earlier: Vec<_> = evidence
        .previous_text
        .as_deref()
        .map(|text| mxr_reader::EarlierMessage {
            text,
            same_author: false,
        })
        .into_iter()
        .collect();
    mxr_reader::authored_text(text, &earlier)
}

/// Most authored-text entries, and bytes of text, a daemon keeps before
/// starting over. Evidence text is cut to 8 KiB when it is read, so the
/// byte budget only matters if a caller ever passes a whole body.
const AUTHORED_CACHE_LIMIT: usize = 4096;
const AUTHORED_CACHE_BYTES: usize = 16 * 1024 * 1024;
/// A result larger than this is returned but not kept.
const AUTHORED_CACHE_ENTRY_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct AuthoredCache {
    texts: HashMap<u64, Arc<str>>,
    bytes: usize,
}

static AUTHORED_CACHE: Lazy<Mutex<AuthoredCache>> =
    Lazy::new(|| Mutex::new(AuthoredCache::default()));

/// The lowercased authored text, computed once per distinct body. Every
/// classification of a message (kind, reason, directed request) needs it,
/// and stripping quoted history is the costly part of a mode read; the
/// result depends only on the two cached texts, so it is keyed by them.
fn authored_lower(evidence: &mxr_core::mail_evidence::MailEvidence) -> Arc<str> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (evidence.text.as_deref(), evidence.previous_text.as_deref()).hash(&mut hasher);
    let key = hasher.finish();
    if let Some(found) = AUTHORED_CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .texts
        .get(&key)
    {
        return Arc::clone(found);
    }
    let lower: Arc<str> = authored_content(evidence).to_lowercase().into();
    if lower.len() > AUTHORED_CACHE_ENTRY_BYTES {
        return lower;
    }
    let mut cache = AUTHORED_CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if cache.texts.len() >= AUTHORED_CACHE_LIMIT || cache.bytes + lower.len() > AUTHORED_CACHE_BYTES
    {
        *cache = AuthoredCache::default();
    }
    cache.bytes += lower.len();
    if let Some(replaced) = cache.texts.insert(key, Arc::clone(&lower)) {
        cache.bytes -= replaced.len();
    }
    lower
}

fn purpose_evidence(signals: &KindSignals<'_>) -> PurposeEvidence {
    let text = authored_lower(signals.evidence);
    let has_authored_text = !text.trim().is_empty();
    let document = billing_document(&text, signals.reply_request);
    let frames = [
        "fyi",
        "for your information",
        "for visibility",
        "for your records",
    ];
    let framed_information = text.lines().take(4).any(|line| {
        frames.iter().any(|frame| {
            line.trim_start()
                .strip_prefix(*frame)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', ':', ',', '.', '\n']))
        })
    });
    let report_delivery = signals
        .subject
        .to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .any(|word| word == "report")
        && (text.contains("attached")
            || text.contains("report below")
            || text.contains("report is available"));
    let information = !signals.reply_request
        && ((framed_information && (signals.recipients > 1 || !signals.evidence.is_reply))
            || (signals.recipients > 1 && report_delivery));
    let human_reply = (signals.evidence.is_reply || signals.evidence.thread_correspondence)
        && has_authored_text
        && !document
        && !signals.evidence.auto_generated
        && !signals.has_list_id
        && signals.evidence.list_id.is_none()
        && !no_reply_address(signals.email)
        && !matches!(address_hint(signals.email), Some(AddressHint::Notifying(_)));
    let purpose = if document {
        MessagePurpose::BillingDocument
    } else if information {
        MessagePurpose::Informational
    } else if human_reply || signals.reply_request {
        MessagePurpose::Correspondence
    } else {
        MessagePurpose::Unknown
    };
    PurposeEvidence {
        purpose,
        human_reply,
        has_authored_text,
    }
}

fn directed_request(text: &str) -> bool {
    let Some(request) = [
        "can you ",
        "could you ",
        "would you ",
        "will you ",
        "please ",
    ]
    .iter()
    .find_map(|prefix| text.strip_prefix(*prefix)) else {
        return false;
    };
    // Positive conversational speech acts, not arbitrary ACTION items such
    // as "please pay" or informational "please find attached".
    [
        "reply",
        "respond",
        "confirm",
        "review",
        "send",
        "share",
        "decide",
        "choose",
        "approve",
        "check",
        "tell me",
        "provide",
        "let me know",
    ]
    .iter()
    .any(|verb| {
        request
            .strip_prefix(*verb)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '?', '.', '!']))
    })
}

fn without_greeting(text: &str) -> &str {
    [
        "good morning ",
        "good afternoon ",
        "good evening ",
        "hi ",
        "hello ",
        "hey ",
        "dear ",
    ]
    .iter()
    .find_map(|prefix| text.strip_prefix(*prefix))
    .unwrap_or(text)
    .trim_start_matches('@')
}

/// A nameless salutation/pleasantry is not evidence of another addressee.
/// Only strip it at a boundary, leaving "Hi Alice" to named-address handling.
fn without_bare_greeting(text: &str) -> &str {
    for greeting in [
        "good morning",
        "good afternoon",
        "good evening",
        "hope you're well",
        "hope you are well",
        "hope all is well",
        "i hope you're well",
        "i hope you are well",
        "thank you",
        "thanks again",
        "thanks",
        "hello",
        "hey",
        "hi",
        "dear",
    ] {
        if let Some(rest) = text.strip_prefix(greeting) {
            if rest.is_empty() || rest.starts_with([',', ':', '!', '.']) {
                return rest.trim_start_matches([',', ':', '!', '.', ' ']);
            }
            if directed_request(rest.trim_start()) {
                return rest.trim_start();
            }
        }
    }
    text
}

fn has_named_addressee(text: &str) -> bool {
    let named = without_greeting(text);
    let candidate = named.split_once([',', ':', '!']).map_or_else(
        || if named != text { named.trim() } else { "" },
        |(name, _)| name.trim(),
    );
    !candidate.is_empty()
        && candidate.split_whitespace().count() <= 3
        && candidate
            .chars()
            .all(|c| c.is_alphabetic() || c.is_whitespace() || matches!(c, '@' | '.' | '-' | '\''))
        && !directed_request(candidate)
}

/// Conservative positive evidence of a directed request, not a general language
/// classifier. Quoted history, infrastructure and past correspondence cannot
/// create debt. Unsupported languages/phrasing remain visible without a turn.
pub(super) fn needs_reply(
    message: &mxr_store::DeskMessage,
    is_self: &dyn Fn(&str) -> bool,
) -> bool {
    directed_to_owner(&message.to, &message.evidence, is_self)
}

pub(super) fn directed_to_owner(
    to: &[mxr_core::Address],
    evidence: &mxr_core::mail_evidence::MailEvidence,
    is_self: &dyn Fn(&str) -> bool,
) -> bool {
    let owned: Vec<_> = to.iter().filter(|a| is_self(&a.email)).collect();
    if owned.is_empty() || evidence.auto_generated || evidence.list_id.is_some() {
        return false;
    }
    if evidence.text.is_none() {
        return false;
    }
    let lower = authored_lower(evidence);
    let names: Vec<_> = owned
        .iter()
        .flat_map(|address| {
            let local = address.email.split('@').next().unwrap_or_default();
            let name = address.name.as_deref().unwrap_or(local);
            [name, name.split_whitespace().next().unwrap_or(name), local]
        })
        .filter(|name| name.len() >= 2)
        .map(str::to_lowercase)
        .collect();
    let names_owner = |line: &str| {
        names.iter().any(|name| {
            let line = without_greeting(line);
            line.strip_prefix(name.as_str()).is_some_and(|rest| {
                rest.trim().is_empty()
                    || rest.starts_with([',', ':', '!'])
                    || directed_request(rest.trim_start())
            })
        })
    };
    let opening = lower
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    let opening = without_bare_greeting(opening);
    let owner_greeting = names_owner(opening);
    let other_greeting = !owner_greeting && has_named_addressee(opening);
    // Require a request addressed to "you"; a question about an invoice,
    // an FYI or "Alice, can you ..." is not evidence of the owner's debt.
    lower.lines().any(|line| {
        let line = without_bare_greeting(line.trim().trim_start_matches([',', ' ']));
        let request =
            (owner_greeting || (to.len() == 1 && !other_greeting)) && directed_request(line);
        let named_request = names.iter().any(|name| {
            let line = without_greeting(line);
            line.strip_prefix(name.as_str())
                .is_some_and(|tail| directed_request(tail.trim_start_matches([',', ':', ' '])))
        });
        (request || named_request)
            && ![
                "no need to",
                "don't ",
                "do not ",
                "not asking",
                "you don't need",
                "you not ",
            ]
            .iter()
            .any(|negation| line.contains(negation))
    })
}

pub(super) fn classify(signals: &KindSignals<'_>) -> Classification {
    if let Some(assessment) = signals.assessment {
        let described = super::assessments::describe(assessment);
        return Classification {
            kind: match described.kind {
                SenderKindData::People => SenderKind::Person,
                SenderKindData::Reading => SenderKind::List,
                SenderKindData::PaperTrail => SenderKind::Automated,
                SenderKindData::ScreenedOut => SenderKind::Denied,
            },
            rule: described.rule,
        };
    }
    if signals.decision == Some(ScreenerDisposition::Deny) {
        return Classification {
            kind: SenderKind::Denied,
            rule: KindRuleData::Decision,
        };
    }
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
    if let Some((kind, _)) = signals.treatment {
        return Classification {
            kind,
            rule: KindRuleData::CustomRule,
        };
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
    let purpose = purpose_evidence(signals);
    let personal_reply = purpose.human_reply;
    let hint = address_hint(signals.email);
    match hint {
        Some(AddressHint::Notifying(rule)) => return automated(rule),
        Some(AddressHint::Newsletter(rule)) if !personal_reply => return list(rule),
        _ => {}
    }
    if signals.has_list_id || signals.evidence.list_id.is_some() {
        return list(KindRuleData::ListId);
    }
    // An ATS/support reply can carry an opt-out header. That delivery
    // mechanism alone loses to positive message-level response evidence,
    // unlike an actual List-Id or an auto-generated declaration.
    if (signals.has_unsubscribe || signals.evidence.unsubscribe)
        && !personal_reply
        && purpose.purpose != MessagePurpose::BillingDocument
    {
        return list(KindRuleData::ListUnsubscribe);
    }
    if !personal_reply && transaction_alert(signals.subject) {
        return automated(KindRuleData::TransactionAlert);
    }
    if no_reply_address(signals.email) {
        return automated(KindRuleData::NoReplyAddress);
    }
    if signals.evidence.auto_generated {
        return automated(KindRuleData::BulkSender);
    }
    if matches!(
        purpose.purpose,
        MessagePurpose::BillingDocument | MessagePurpose::Informational
    ) {
        return automated(KindRuleData::BulkSender);
    }
    // History can support a personal interpretation of a role address or
    // templated bulk mail, but cannot erase the per-message evidence above.
    if !signals.sender.wrote_to
        && !signals.written_to
        && !signals.evidence.is_reply
        && !signals.reply_request
    {
        if role_address(signals.email) {
            return automated(KindRuleData::RoleAddress);
        }
        if signals.sender.templated && signals.evidence.bulk {
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
        KindRuleData::Fallback => "No rule decided".into(),
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
        KindRuleData::NoReplyAddress if signals.has_unsubscribe || signals.evidence.unsubscribe => {
            "no-reply sender, has List-Unsubscribe".to_string()
        }
        KindRuleData::NoReplyAddress => "no-reply sender, no list headers".to_string(),
        KindRuleData::TransactionAlert => "bank or card transaction alert".to_string(),
        KindRuleData::RoleAddress => "a role address you have never written to".to_string(),
        KindRuleData::BulkSender if signals.evidence.auto_generated => "message declares itself automatically generated".to_string(),
        KindRuleData::BulkSender => match purpose_evidence(signals).purpose {
            MessagePurpose::BillingDocument => "structured billing document: document reference, date, order or amount, and delivery framing".to_string(),
            MessagePurpose::Informational => "informational update: FYI or report delivery, recipient context, and no directed request".to_string(),
            _ => "automatic message-purpose evidence".to_string(),
        },
        KindRuleData::TemplatedSender => {
            "repeated subject templates with bulk precedence, and no correspondence history"
                .to_string()
        }
        KindRuleData::ListId if signals.has_unsubscribe => {
            "mailing list, has List-Unsubscribe".to_string()
        }
        KindRuleData::ListId => "mailing list".to_string(),
        KindRuleData::ListUnsubscribe => "has List-Unsubscribe".to_string(),
        KindRuleData::ListSender => "sends to mailing lists".to_string(),
        KindRuleData::Person if purpose_evidence(signals).human_reply && signals.reply_request => "authored reply with a request directed to an owned recipient".to_string(),
        KindRuleData::Person if purpose_evidence(signals).human_reply => "authored reply in existing correspondence; no new directed request detected".to_string(),
        KindRuleData::Person if signals.reply_request => "authored request directed to an owned recipient".to_string(),
        KindRuleData::Person => {
            "possible personal mail; no decisive list or automation evidence".to_string()
        }
        KindRuleData::Copied => COPIED_REASON.to_string(),
        KindRuleData::WrittenTo => WRITTEN_TO_REASON.to_string(),
        KindRuleData::Moved => "you moved this email".to_string(),
        KindRuleData::CustomRule => format!(
            "sorted by rule: {}",
            signals.treatment.map_or("", |(_, name)| name)
        ),
    }
}

/// The rule in one word, for an arrival's line: "→ Updates · automated".
pub(super) const fn rule_tag(rule: KindRuleData) -> &'static str {
    match rule {
        KindRuleData::Fallback => "not sure",
        KindRuleData::Decision => "chosen",
        KindRuleData::Delivery => "delivery",
        KindRuleData::Invite => "invite",
        KindRuleData::AutomatedAddress | KindRuleData::AutomatedDomain => "automated",
        KindRuleData::NewsletterAddress | KindRuleData::NewsletterDomain => "newsletter",
        KindRuleData::ListId | KindRuleData::ListUnsubscribe | KindRuleData::ListSender => "list",
        KindRuleData::NoReplyAddress => "no-reply",
        KindRuleData::TransactionAlert => "bank alert",
        KindRuleData::RoleAddress => "role address",
        KindRuleData::BulkSender => "message purpose",
        KindRuleData::TemplatedSender => "templated",
        KindRuleData::Person => "possible personal",
        KindRuleData::Copied => "copied",
        KindRuleData::WrittenTo => "written to",
        KindRuleData::Moved => "moved",
        KindRuleData::CustomRule => "custom rule",
    }
}

/// Why mail from someone you've written to is in Messages (N1).
pub(super) const WRITTEN_TO_REASON: &str = "addressed to you by someone you've written to";

/// Why a person's mail is in Updates: the thread's shape, not the sender.
pub(super) const COPIED_REASON: &str = "copied to you, or sent to a crowd, and not your turn";

/// The wire form: kind, rule, reason and whether the user chose it.
pub(super) fn describe(signals: &KindSignals<'_>) -> MailKindData {
    describe_classified(signals, classify(signals))
}

/// `describe` for a caller that already classified these signals.
pub(super) fn describe_classified(
    signals: &KindSignals<'_>,
    classification: Classification,
) -> MailKindData {
    if let Some(assessment) = signals.assessment {
        return super::assessments::describe(assessment);
    }
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
            assessment: None,
            evidence: Lazy::force(&EMPTY_EVIDENCE),
            reply_request: false,
            recipients: 1,
            email,
            subject: "",
            sender: SenderFacts::default(),
            has_list_id: false,
            has_unsubscribe: false,
            is_delivery: false,
            is_invite: false,
            decision: None,
            written_to: false,
            addressed: true,
            moved: None,
            treatment: None,
        }
    }

    static EMPTY_EVIDENCE: Lazy<mxr_core::mail_evidence::MailEvidence> =
        Lazy::new(Default::default);

    #[test]
    fn messages_visibility_is_not_a_human_response() {
        let chosen = KindSignals {
            moved: Some(SenderKind::Person),
            ..signals("no-reply@studio.example")
        };
        assert_eq!(classify(&chosen).kind, SenderKind::Person);
        assert!(!human_response(&chosen));
        let acknowledgement = KindSignals {
            written_to: true,
            evidence: &mxr_core::mail_evidence::MailEvidence {
                text: Some("Thanks, received.".into()),
                ..Default::default()
            },
            ..signals("sam@studio.example")
        };
        assert!(human_response(&acknowledgement));
        assert!(!kept_in_messages(&KindSignals {
            moved: Some(SenderKind::List),
            decision: Some(ScreenerDisposition::Allow),
            ..signals("sam@studio.example")
        }));
    }

    #[test]
    fn positive_human_invoice_reply_beats_address_hints_not_message_automation() {
        let evidence = mxr_core::mail_evidence::MailEvidence {
            is_reply: true,
            text: Some("Could you confirm the invoice reference?".into()),
            ..Default::default()
        };
        let reply = KindSignals {
            evidence: &evidence,
            reply_request: true,
            subject: "Invoice INV-123 for USD 12.00",
            ..signals("billing@studio.example")
        };
        assert_eq!(classify(&reply).kind, SenderKind::Person);
        assert_eq!(
            classify(&KindSignals {
                has_list_id: true,
                ..reply
            })
            .kind,
            SenderKind::List
        );
        let generated = mxr_core::mail_evidence::MailEvidence {
            auto_generated: true,
            ..evidence.clone()
        };
        assert_eq!(
            classify(&KindSignals {
                evidence: &generated,
                ..reply
            })
            .kind,
            SenderKind::Automated
        );
    }

    #[test]
    fn bare_greetings_do_not_invent_another_addressee_or_an_owned_alias() {
        let owner = [mxr_core::Address {
            name: None,
            email: "bk@example.com".into(),
        }];
        let own = |email: &str| email == "bk@example.com";
        for text in [
            "Hi,\nCould you send the deck by Friday?",
            "Hi, could you confirm the meeting time?",
            "Hello,\nCould you review the plan?",
            "Hey, could you reply today?",
            "Good morning,\nCould you confirm?",
            "Hope you're well,\nCould you send the deck?",
            "Thanks,\nCould you confirm?",
        ] {
            let evidence = mxr_core::mail_evidence::MailEvidence {
                text: Some(text.into()),
                ..Default::default()
            };
            assert!(directed_to_owner(&owner, &evidence, &own), "{text}");
        }
        for text in ["Alice,\nCould you send the deck?", "Alice: could you confirm?", "Hi Alice,\nCould you send the deck?", "Hi Brian,\nCould you confirm?", "Hi, could you confirm? No need to reply.", "Hi,\n\nFrom: Alice <alice@example.com>\nSent: Monday\nTo: BK <bk@example.com>\nSubject: Old ask\n\nCould you confirm?"] {
            let evidence = mxr_core::mail_evidence::MailEvidence { text: Some(text.into()), ..Default::default() };
            assert!(!directed_to_owner(&owner, &evidence, &own), "{text}");
        }
        let group = [
            owner[0].clone(),
            mxr_core::Address {
                name: None,
                email: "alice@example.com".into(),
            },
        ];
        let evidence = mxr_core::mail_evidence::MailEvidence {
            text: Some("Hi, could you confirm?".into()),
            ..Default::default()
        };
        assert!(!directed_to_owner(&group, &evidence, &own));
        assert!(!directed_to_owner(&group[1..], &evidence, &own));
    }

    #[test]
    fn a_reply_can_answer_without_requesting_another_reply() {
        let evidence = mxr_core::mail_evidence::MailEvidence {
            is_reply: true,
            text: Some("I've corrected the account name. No action is required.".into()),
            ..Default::default()
        };
        let reply = KindSignals {
            evidence: &evidence,
            written_to: true,
            ..signals("billing@vendor.example")
        };
        assert_eq!(classify(&reply).kind, SenderKind::Person);
        assert!(human_response(&reply));
        assert!(!requires_reply(&reply));
        let generated = mxr_core::mail_evidence::MailEvidence {
            auto_generated: true,
            ..evidence.clone()
        };
        let automatic = KindSignals {
            evidence: &generated,
            ..reply
        };
        assert_eq!(classify(&automatic).kind, SenderKind::Automated);
        assert!(!human_response(&automatic));
    }

    #[test]
    fn headerless_document_purpose_needs_independent_fields_and_delivery_context() {
        let evidence = mxr_core::mail_evidence::MailEvidence { text: Some(
            "Invoice number: AB12345\nInvoice date: 2026-10-18\nOrder number: PO23456\nThank you for your purchase. Your billing document is attached.".into()), ..Default::default() };
        let document = KindSignals {
            evidence: &evidence,
            written_to: true,
            ..signals("alex@vendor.example")
        };
        assert_eq!(classify(&document).kind, SenderKind::Automated);
        assert!(describe(&document)
            .reason
            .contains("structured billing document"));
        assert_eq!(
            classify(&KindSignals {
                has_unsubscribe: true,
                ..document
            })
            .kind,
            SenderKind::Automated,
            "a delivery opt-out does not turn a structured billing document into a newsletter"
        );
        let paired_fields = mxr_core::mail_evidence::MailEvidence { text: Some("Document number\nXY7788\nBilling date\n18/10/2026\nAmount due\n12.00 USD\nYour billing document is available.".into()), ..Default::default() };
        assert_eq!(
            classify(&KindSignals {
                evidence: &paired_fields,
                ..document
            })
            .kind,
            SenderKind::Automated
        );
        let personal = mxr_core::mail_evidence::MailEvidence { is_reply: true, text: Some(
            "Hi User,\nCould you confirm the amended invoice?\nInvoice number: AB12345\nInvoice date: 2026-10-18\nOrder number: PO23456\nAmount due: USD 12.00\nThank you for your purchase. Your billing document is attached.".into()), ..Default::default() };
        let discussion = KindSignals {
            evidence: &personal,
            reply_request: true,
            ..document
        };
        assert_eq!(classify(&discussion).kind, SenderKind::Person);
        assert!(human_response(&discussion));
        assert!(requires_reply(&discussion));
        for (generated, list, email) in [
            (true, false, "alex@vendor.example"),
            (false, true, "alex@vendor.example"),
            (false, false, "no-reply@vendor.example"),
        ] {
            let evidence = mxr_core::mail_evidence::MailEvidence {
                auto_generated: generated,
                ..personal.clone()
            };
            let automatic = KindSignals {
                evidence: &evidence,
                has_list_id: list,
                email,
                ..discussion
            };
            assert_ne!(classify(&automatic).kind, SenderKind::Person);
            assert!(!human_response(&automatic) && !requires_reply(&automatic));
        }
        let incomplete = mxr_core::mail_evidence::MailEvidence {
            text: Some("Invoice number: AB12345\nCan we discuss this invoice?".into()),
            ..Default::default()
        };
        assert_eq!(
            classify(&KindSignals {
                evidence: &incomplete,
                ..document
            })
            .kind,
            SenderKind::Person
        );
    }

    /// The cache returns what stripping returns, and the earlier message is
    /// part of the key: the same body reads differently against it.
    #[test]
    fn authored_text_cache_matches_stripping_and_keys_on_the_earlier_message() {
        let prior = "Could you confirm the launch date by Monday and send the signed copy?\nPlease also share the final reference with everyone before the Friday review meeting.";
        let alone = mxr_core::mail_evidence::MailEvidence {
            text: Some(format!("FYI, this was handled.\n\n{prior}")),
            ..Default::default()
        };
        let against_earlier = mxr_core::mail_evidence::MailEvidence {
            previous_text: Some(prior.into()),
            ..alone.clone()
        };
        for evidence in [&alone, &against_earlier, &alone] {
            assert_eq!(
                &*authored_lower(evidence),
                authored_content(evidence).to_lowercase()
            );
        }
        assert_ne!(
            authored_content(&alone),
            authored_content(&against_earlier),
            "the fixture must depend on the earlier message"
        );
    }

    /// The bounded split search must find exactly what trying every split
    /// found, and a long unbroken paragraph must stay linear.
    #[test]
    fn document_value_matches_every_split_search_and_stays_linear() {
        fn every_split(line: &str) -> Option<(DocumentField, &str)> {
            line.char_indices()
                .rev()
                .filter(|(_, c)| c.is_whitespace())
                .find_map(|(index, _)| {
                    Some((document_field(&line[..index])?, line[index..].trim()))
                })
        }
        let field = |found: Option<(DocumentField, &str)>| {
            found.map(|(field, value)| (field as u8, value.to_string()))
        };
        for line in [
            "invoice number 12345",
            "invoice no. 778",
            "web order number 1234-55",
            "date of issue 12 march 2026",
            "invoice date 2026-03-12 total due 40",
            "amount due $12.40",
            "total £9.99 thanks",
            "order # 42",
            "your order number 42",
            "reference 9 for invoice 3",
            "invoice-number 12",
            "café total 12",
            "no",
            "",
            "   ",
            "hello there invoice number 9",
        ] {
            assert_eq!(
                field(document_value(line, None)),
                field(document_field(line).map_or_else(|| every_split(line), |_| None)),
                "{line:?}"
            );
        }

        // Prose, and a long run of words that could each be in a label
        // (untrusted mail can be written to be slow).
        for paragraph in [
            "word ".repeat(5_000) + "total 12",
            "invoice order number ".repeat(2_000) + "12",
            "invoice ".repeat(5_000) + "12",
        ] {
            let started = std::time::Instant::now();
            assert!(document_value(&paragraph, None).is_none());
            assert!(
                started.elapsed() < std::time::Duration::from_millis(50),
                "{:?}",
                started.elapsed()
            );
        }
    }

    #[test]
    fn document_field_labels_vary_without_changing_the_independent_evidence_grammar() {
        for (reference, date, order) in [
            (
                "Invoice Number:AB12345",
                "Date of Issue:14.10.2026",
                "Web order number: W12345678",
            ),
            (
                "Document Ref.: AB12345",
                "Issue Date: 2026-10-14",
                "Purchase Order Reference: W12345678",
            ),
            (
                "Invoice # AB12345",
                "Billing Date\n14/10/2026",
                "Order No. W12345678",
            ),
        ] {
            let text = format!("{reference}\n{date}\n\nDear User,\nThank you for your recent purchase from Vendor.\n{order}\nPlease find attached your billing document AB12345.");
            let evidence = mxr_core::mail_evidence::MailEvidence {
                text: Some(text),
                ..Default::default()
            };
            let document = KindSignals {
                evidence: &evidence,
                ..signals("ordinary@vendor.example")
            };
            assert_eq!(classify(&document).kind, SenderKind::Automated);
            assert!(describe(&document)
                .reason
                .contains("structured billing document"));
        }
        for text in [
            "Invoice Number: AB12345\nDate of Issue: not-a-date\nWeb Order Number: W12345678\nYour billing document is attached.",
            "Invoice Number: AB12345\nDate of Issue:14.10.2026\nYour billing document is attached.",
            "The invoice amount is interesting; could we discuss it?",
        ] {
            let evidence = mxr_core::mail_evidence::MailEvidence { text: Some(text.into()), ..Default::default() };
            assert_eq!(classify(&KindSignals { evidence: &evidence, ..signals("ordinary@vendor.example") }).kind, SenderKind::Person);
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
            "I paid £40.00 for the tickets",
            "Invoice 2026-114 for USD 12.00",
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
        assert_eq!(described.kind, SenderKindData::People);
        assert_eq!(described.rule, KindRuleData::Person);
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
            evidence: &mxr_core::mail_evidence::MailEvidence {
                bulk: true,
                ..Default::default()
            },
            ..signals("desk.officer@bank.example")
        });
        assert_eq!(described.kind, SenderKindData::PaperTrail);
        assert_eq!(described.rule, KindRuleData::TemplatedSender);
        assert_eq!(
            described.reason,
            "repeated subject templates with bulk precedence, and no correspondence history"
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
    fn correspondence_history_does_not_erase_message_list_evidence() {
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
        ] {
            let known = describe(&KindSignals {
                written_to: true,
                ..list
            });
            assert_eq!(known.kind, SenderKindData::Reading);
            assert_ne!(known.rule, KindRuleData::WrittenTo);
            // Only copied: still theirs, not Reading. Placement then asks
            // about it (the thread is copied), and `kept_in_messages` holds
            // only what is addressed to you.
            let copied_signals = KindSignals {
                written_to: true,
                addressed: false,
                ..list
            };
            assert_eq!(describe(&copied_signals).kind, SenderKindData::Reading);
            assert!(!kept_in_messages(&copied_signals));
            assert!(!kept_in_messages(&KindSignals {
                written_to: true,
                ..list
            }));
            // A stranger's list mail stays Reading.
            assert_eq!(describe(&list).kind, SenderKindData::Reading);
        }
        // A newsletter address gives way too.
        let by_address = describe(&KindSignals {
            written_to: true,
            ..signals("newsletter@orbit.example")
        });
        assert_eq!(by_address.kind, SenderKindData::Reading);
        assert_eq!(by_address.rule, KindRuleData::NewsletterAddress);
        assert_eq!(
            describe(&signals("newsletter@orbit.example")).kind,
            SenderKindData::Reading
        );
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
