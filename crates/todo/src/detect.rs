//! Reads one message for a to-do: the action, its object, the
//! counterparty, the amount, the due date and the one link, with where each
//! came from.
//!
//! Precision over recall: a wrong to-do costs trust, a missed one is caught
//! by the smart tier later. So a bill needs a payment word and an amount or
//! a date; anything paid, collected by direct debit or card on file, or
//! marked "no action needed" is not a to-do (it's an update or a record);
//! a renewal needs something you'd compare quotes for and a date; mailing
//! list mail never becomes a renewal, sign, verify or RSVP to-do.

use crate::action_link::{email_domain, pick_link, LinkCandidate};
use crate::dates::{end_of_day, find_due, find_lifetime, order_for, trigger_regex, FoundDate};
use crate::money::{pick_amount, Amount};
use crate::provenance::{FieldProvenance, FieldSource, FieldSources};
use crate::schema_org::{self, SchemaDate, SchemaTodo};
use crate::text::{capitalise, clip, lower_first, truncate};
use crate::TodoKind;
use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

/// Everything detection needs from a message.
#[derive(Debug, Clone, Copy)]
pub struct MessageInput<'a> {
    pub subject: &'a str,
    /// Reader-cleaned plain text: quotes and signatures stripped.
    pub body_text: &'a str,
    /// Raw HTML, for schema.org markup and link text.
    pub body_html: Option<&'a str>,
    pub from_name: Option<&'a str>,
    pub from_email: &'a str,
    pub sent: DateTime<Utc>,
    /// Sent through a mailing list (`List-Id`).
    pub list_mail: bool,
}

/// Which layer found the to-do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Rule,
    Schema,
    Ics,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Schema => "schema",
            Self::Ics => "ics",
        }
    }

    fn source(self) -> FieldSource {
        match self {
            Self::Rule => FieldSource::Rule,
            Self::Schema => FieldSource::Schema,
            Self::Ics => FieldSource::Ics,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub kind: TodoKind,
    pub verb: String,
    pub doc_type: Option<String>,
    /// Verb plus object: "Pay council tax".
    pub title: String,
    /// The object, normalised, for the dedup key.
    pub object_key: String,
    pub counterparty: Option<String>,
    pub sender_domain: Option<String>,
    pub amount: Option<Amount>,
    pub due: Option<FoundDate>,
    pub event_start: Option<DateTime<Utc>>,
    pub link: Option<LinkCandidate>,
    pub origin: Origin,
    /// The words that put it here and the layer: `"payment due 9
    /// October" (rule)`.
    pub reason: String,
    pub fields: FieldSources,
}

/// Body text past this is not read: what a to-do asks is near the top.
const BODY_CHARS: usize = 8_000;

static PAID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(thank(s| you) for (your )?payment|payment (received|confirmation|confirmed|successful|complete|processed)|we('ve| have) received your payment|your receipt|receipt (for|from|#|number)|amount paid|paid on|has been paid|you('ve| have) paid|payment of [^.\n]{0,40}(was|has been) (received|processed|taken|successful)|refund(ed)?\b)")
        .expect("valid paid regex")
});

static AUTO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(direct debit|will be (collected|taken|debited|charged)|we('ll| will) (collect|take|charge|debit)|scheduled payment|upcoming payment|automatic(ally)? (payment|pay|charge[ds]?|renew(s|ed|al)?|collected|taken)|auto[- ]?(pay|renew(s|al)?)|card on file|no action (is )?(needed|required)|(you )?don'?t need to do anything|nothing (else )?(you need )?to do|standing order|recurring payment)")
        .expect("valid auto regex")
});

static FAILED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(payment (has )?(failed|was declined|declined|was unsuccessful|unsuccessful|didn'?t go through|could not be (processed|taken|collected))|(can'?t|cannot|couldn'?t|could not|were unable to|we'?re unable to|unable to|had (some )?trouble) (process(ing)?|take|taking|collect(ing)?|charg(e|ing)) (your )?(latest |recent |last |most recent )?(payment|card|direct debit|subscription)|card (was|has been) declined|payment (method|details|card) (has )?expired|problem with your (payment|card|billing)|direct debit (was )?(unpaid|returned|cancelled|failed))")
        .expect("valid failed regex")
});

static FIX_ASK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(update|fix) your (payment|billing|card)( method| details| information| info)?\b",
    )
    .expect("valid fix regex")
});

static BILL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(your (\w+ ){0,3}?(bill|invoice)\b|invoice\s*(#|no\.?|number|ref)|new invoice|amount (due|owed|outstanding|to pay)|balance (due|outstanding|to pay)|total due|payment (is )?(due|overdue)|payment (reminder|request)|(is|now) (overdue|past due)|due date|pay (by|before|now|your bill|online)|please pay|minimum payment|final (notice|reminder)|reminder to pay|request for payment)")
        .expect("valid bill regex")
});

static BANK_DETAILS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(sort code|account number|iban|bank transfer|bacs|routing number)\b")
        .expect("valid bank regex")
});

static RENEWAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(renew(al)? (now|today|by|before|date|notice|invitation|reminder|quote|is due|due)|due for renewal|time to renew|(is|are) due to (expire|renew|end)|(expire|expires|expiring|ends|lapses?) (on|soon|in)|renews on)\b")
        .expect("valid renewal regex")
});

static RENEWAL_OBJECT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\byour ((?:[a-z]+ ){0,3}?(insurance|policy|cover|mot|tv licen[cs]e|driving licen[cs]e|licen[cs]e|passport|visa|membership|domain|warranty|tenancy|lease|road tax|vehicle tax|breakdown cover|id card))\b")
        .expect("valid renewal object regex")
});

static PROMO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\d+% off|save \d+%|\bsale\b|limited[- ]time|exclusive offer|special offer|discount code|promo code)")
        .expect("valid promo regex")
});

static SIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(please (review and )?sign|ready (for your signature|to sign|for signing)|awaiting your signature|requires? your signature|signature (required|requested|needed)|sign (the|your|this) (document|contract|agreement|form|lease|letter|offer|nda)|please docusign|review and sign|complete with docusign)")
        .expect("valid sign regex")
});

static SIGN_DONE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(completed|signed|voided|declined)\b|has been (signed|completed)|signed by all|all parties have signed")
        .expect("valid sign done regex")
});

static VERIFY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(verify|confirm|activate|validate) (your |this |the )?(new )?(email( address)?|e-mail|account|identity|address|phone( number)?|mobile( number)?|registration|sign-?up|subscription|booking|reservation|attendance|appointment)\b")
        .expect("valid verify regex")
});

static SECURITY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b((verification|security|one[- ]time|login|sign[- ]in|access|confirmation) code|passcode|\botp\b|new sign[- ]?in (to|from|on|detected)|sign[- ]in attempt|login attempt|was this you|password (reset|change)|reset your password)")
        .expect("valid security regex")
});

static RSVP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(rsvp|please (let (us|me) know|confirm) (if|whether) you('ll| can| will| are able to)? ?(be able to )?(attend|come|make it|join)|kindly (reply|respond|confirm (your )?attendance)|will you (be )?(attending|joining|coming)|are you (able to )?(coming|attending|joining))")
        .expect("valid rsvp regex")
});

static BILL_DUE_SPECIFIC: Lazy<Regex> = Lazy::new(|| {
    trigger_regex(
        r"payment\s+due(?:\s+date)?|amount\s+due|due\s+date|due|pay(?:ment)?\s+by|payable\s+by|no\s+later\s+than|deadline|on\s+or\s+before",
    )
});
static BROAD_BY: Lazy<Regex> = Lazy::new(|| trigger_regex(r"by|before"));
static RENEWAL_DUE: Lazy<Regex> = Lazy::new(|| {
    trigger_regex(
        r"renew(?:al)?\s+date|renews|renewal\s+is\s+due|due\s+for\s+renewal|due\s+to\s+(?:expire|renew|end)|expiry\s+date|expires|expiring|expire|ends|lapses|renew\s+by|renew\s+before|valid\s+until",
    )
});
static SIGN_DUE: Lazy<Regex> =
    Lazy::new(|| trigger_regex(r"sign\s+by|signed\s+by|return\s+by|due|deadline|by"));
static VERIFY_DUE: Lazy<Regex> =
    Lazy::new(|| trigger_regex(r"expires|expiring|valid\s+until|before"));
static RSVP_DUE: Lazy<Regex> = Lazy::new(|| {
    trigger_regex(
        r"rsvp\s+by|reply\s+by|respond\s+by|let\s+(?:us|me)\s+know\s+by|confirm\s+by|rsvp\s+before",
    )
});
static EVENT_DATE: Lazy<Regex> = Lazy::new(|| trigger_regex(r"on|date|when|this"));

/// Bill subjects named as the thing, not the sender: "Pay council tax".
const BILL_THINGS: &[(&str, &str)] = &[
    ("council tax", "council tax"),
    ("water", "water bill"),
    ("electricity", "electricity bill"),
    ("energy", "energy bill"),
    ("gas", "gas bill"),
    ("broadband", "broadband bill"),
    ("phone", "phone bill"),
    ("mobile", "mobile bill"),
    ("internet", "internet bill"),
    ("credit card", "credit card bill"),
    ("rent", "rent"),
    ("service charge", "service charge"),
    ("school fees", "school fees"),
    ("nursery", "nursery fees"),
    ("parking", "parking charge"),
    ("penalty charge", "penalty charge"),
    ("tax", "tax bill"),
];

/// Display-name fragments that are roles, not organisations.
const NAME_NOISE: &[&str] = &[
    "no-reply",
    "noreply",
    "no reply",
    "do-not-reply",
    "donotreply",
    "notifications",
    "notification",
    "billing",
    "accounts",
    "account",
    "support",
    "team",
    "customer service",
    "payments",
    "info",
    "hello",
    "mailer",
];

/// Read `input` for a to-do. `tz` is the user's zone, for due dates.
pub fn detect<Tz>(input: &MessageInput<'_>, tz: &Tz) -> Option<Detection>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let body = truncate(input.body_text, BODY_CHARS);
    let text = format!("{}\n{}", input.subject, body);
    let sender_domain = email_domain(input.from_email);
    let counterparty = counterparty(input.from_name, sender_domain.as_deref());
    let anchor = input.sent.with_timezone(tz);
    let context = Context {
        input,
        text: &text,
        sender_domain: sender_domain.clone(),
        counterparty,
        anchor_order: order_for(sender_domain.as_deref().unwrap_or(""), None),
    };

    let schema = schema_org::read(input.body_html);
    let text_says_failed = FAILED.is_match(&text) || FIX_ASK.is_match(input.subject);
    match schema.todo {
        Some(todo @ SchemaTodo::PaymentFailed { .. }) => {
            return Some(context.schema_todo(todo, tz, &anchor));
        }
        // Markup never hides a failure the words report: a declined
        // payment under a stale "due" or "paid" status is still a to-do.
        _ if text_says_failed => return Some(context.payment_failed(&anchor)),
        Some(todo) => return Some(context.schema_todo(todo, tz, &anchor)),
        None if schema.settled => return None,
        None => {}
    }
    if PAID.is_match(&text) {
        return None;
    }
    if let Some(found) = BILL.find(&text) {
        if !AUTO.is_match(&text) {
            if let Some(detection) = context.bill(found.as_str(), &anchor) {
                return Some(detection);
            }
        }
    }
    if input.list_mail || PROMO.is_match(&text) {
        return None;
    }
    if let Some(found) = RENEWAL.find(&text) {
        if let Some(detection) = context.renewal(found.as_str(), &anchor) {
            return Some(detection);
        }
    }
    if let Some(found) = SIGN.find(&text) {
        if !SIGN_DONE.is_match(input.subject) && !SIGN_DONE.is_match(body) {
            return Some(context.sign(found.as_str(), &anchor));
        }
    }
    if SECURITY.is_match(&text) {
        return None;
    }
    if let Some(found) = VERIFY.find(&text) {
        if let Some(detection) = context.verify(found.as_str(), &anchor) {
            return Some(detection);
        }
    }
    if let Some(found) = RSVP.find(&text) {
        return context.rsvp(found.as_str(), &anchor, tz);
    }
    None
}

struct Context<'a> {
    input: &'a MessageInput<'a>,
    text: &'a str,
    sender_domain: Option<String>,
    counterparty: Option<String>,
    anchor_order: crate::dates::DateOrder,
}

impl Context<'_> {
    fn base(
        &self,
        kind: TodoKind,
        title: String,
        object: &str,
        origin: Origin,
        evidence: &str,
    ) -> Detection {
        let mut fields = FieldSources::default();
        fields.set(
            "kind",
            FieldProvenance::with_evidence(origin.source(), evidence),
        );
        fields.set(
            "title",
            FieldProvenance::with_evidence(origin.source(), evidence),
        );
        if self.counterparty.is_some() {
            fields.set(
                "counterparty",
                FieldProvenance::with_evidence(FieldSource::Rule, "the sender's name"),
            );
        }
        Detection {
            kind,
            verb: kind.default_verb().to_string(),
            doc_type: None,
            title,
            object_key: normalise(object),
            counterparty: self.counterparty.clone(),
            sender_domain: self.sender_domain.clone(),
            amount: None,
            due: None,
            event_start: None,
            link: None,
            origin,
            reason: format!("\"{}\" ({})", clip(evidence, 80), origin_label(origin)),
            fields,
        }
    }

    fn party(&self) -> String {
        self.counterparty
            .clone()
            .or_else(|| self.sender_domain.clone())
            .unwrap_or_else(|| "the sender".to_string())
    }

    fn due<Tz>(
        &self,
        triggers: &[&Regex],
        anchor: &DateTime<Tz>,
        currency: Option<&str>,
    ) -> Option<FoundDate>
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let order = order_for(self.sender_domain.as_deref().unwrap_or(""), currency);
        triggers
            .iter()
            .find_map(|triggers| find_due(self.text, triggers, anchor, order))
    }

    fn set_due(&self, detection: &mut Detection, due: Option<FoundDate>, source: FieldSource) {
        if let Some(due) = &due {
            let mut provenance = FieldProvenance::with_evidence(source, due.words.clone());
            if !due.checked {
                provenance = provenance.unchecked();
            }
            detection.fields.set("due_at", provenance);
            detection.reason = format!("\"{}\" ({})", clip(&due.words, 80), source_label(source));
        }
        detection.due = due;
    }

    fn set_amount(detection: &mut Detection, amount: Option<Amount>, source: FieldSource) {
        if let Some(amount) = &amount {
            let mut provenance = FieldProvenance::with_evidence(source, amount.text.clone());
            if !amount.checked {
                provenance = provenance.unchecked();
            }
            detection.fields.set("amount", provenance);
        }
        detection.amount = amount;
    }

    /// The one link: the markup's own when it names one, else the link
    /// whose words match the verb.
    fn set_link(&self, detection: &mut Detection, markup_url: Option<String>) {
        let from_markup = markup_url.and_then(|url| {
            let host = url::Url::parse(&url).ok()?.host_str()?.to_ascii_lowercase();
            Some(LinkCandidate {
                url,
                anchor: String::new(),
                host,
            })
        });
        let provenance = match &from_markup {
            Some(_) => FieldProvenance::with_evidence(FieldSource::Schema, "the markup's url"),
            None => {
                let Some(link) =
                    pick_link(self.input.body_html, self.input.body_text, detection.kind)
                else {
                    return;
                };
                let provenance = FieldProvenance::with_evidence(
                    FieldSource::Rule,
                    format!("the link \"{}\"", clip(&link.anchor, 60)),
                );
                detection.link = Some(link);
                provenance
            }
        };
        if from_markup.is_some() {
            detection.link = from_markup;
        }
        detection.fields.set("action_url", provenance);
    }

    fn schema_todo<Tz>(&self, todo: SchemaTodo, tz: &Tz, anchor: &DateTime<Tz>) -> Detection
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let schema_due = |date: Option<SchemaDate>, words: Option<String>| -> Option<FoundDate> {
            let at = match date? {
                SchemaDate::Day(day) => end_of_day(day, tz),
                SchemaDate::At(at) => at,
            };
            Some(FoundDate {
                at,
                words: words.unwrap_or_default(),
                checked: true,
            })
        };
        match todo {
            SchemaTodo::Bill {
                provider,
                description,
                amount,
                due,
                due_text,
                url,
            } => {
                let (thing, title_from) = match bill_thing(self.input.subject) {
                    Some(thing) => (Some(thing), "the subject"),
                    None => (
                        description.as_deref().and_then(bill_thing).or_else(|| {
                            description
                                .as_deref()
                                .filter(|d| short_phrase(d))
                                .map(lower_first)
                        }),
                        "Invoice.description",
                    ),
                };
                let party = provider.clone().unwrap_or_else(|| self.party());
                let title = format!("Pay {}", thing.clone().unwrap_or_else(|| party.clone()));
                let evidence = format!("Invoice from {party}");
                let mut detection = self.base(
                    TodoKind::Bill,
                    title,
                    thing.as_deref().unwrap_or(&party),
                    Origin::Schema,
                    &evidence,
                );
                if thing.is_some() {
                    detection.fields.set(
                        "title",
                        FieldProvenance::with_evidence(FieldSource::Rule, title_from),
                    );
                }
                if let Some(provider) = provider {
                    detection.counterparty = Some(provider);
                    detection.fields.set(
                        "counterparty",
                        FieldProvenance::with_evidence(FieldSource::Schema, "Invoice.provider"),
                    );
                }
                let amount = amount
                    .map(|amount| Amount {
                        minor: amount.minor,
                        currency: amount.currency,
                        text: amount.text,
                        checked: true,
                    })
                    .map(|amount| (amount, FieldSource::Schema))
                    .or_else(|| pick_amount(self.text).map(|amount| (amount, FieldSource::Rule)));
                if let Some((amount, source)) = amount {
                    Self::set_amount(&mut detection, Some(amount), source);
                }
                match schema_due(due, due_text) {
                    Some(due) => {
                        let day = due.at.with_timezone(tz).format("%-d %B %Y").to_string();
                        self.set_due(&mut detection, Some(due), FieldSource::Schema);
                        detection.reason = format!("an invoice due {day} (schema.org)");
                    }
                    None => {
                        let due = self.due(
                            &[&BILL_DUE_SPECIFIC],
                            anchor,
                            detection.amount.as_ref().map(|a| a.currency.as_str()),
                        );
                        self.set_due(&mut detection, due, FieldSource::Rule);
                    }
                }
                self.set_link(&mut detection, url);
                detection.doc_type = Some(pay_method(&detection, self.text).to_string());
                detection
            }
            SchemaTodo::PaymentFailed {
                provider,
                amount,
                url,
            } => {
                let mut detection = self.payment_failed(anchor);
                detection.origin = Origin::Schema;
                detection.reason = "an invoice whose payment was declined (schema.org)".to_string();
                detection.fields.set(
                    "kind",
                    FieldProvenance::with_evidence(FieldSource::Schema, "Invoice.paymentStatus"),
                );
                if let Some(provider) = provider {
                    detection.title = format!("Fix payment for {provider}");
                    detection.object_key = normalise(&provider);
                    detection.counterparty = Some(provider);
                    detection.fields.set(
                        "counterparty",
                        FieldProvenance::with_evidence(FieldSource::Schema, "Invoice.provider"),
                    );
                }
                if let Some(amount) = amount {
                    let amount = Amount {
                        minor: amount.minor,
                        currency: amount.currency,
                        text: amount.text,
                        checked: true,
                    };
                    Self::set_amount(&mut detection, Some(amount), FieldSource::Schema);
                }
                if url.is_some() {
                    self.set_link(&mut detection, url);
                }
                detection
            }
            SchemaTodo::PendingReservation {
                name,
                provider,
                starts,
                starts_text,
                url,
            } => {
                let place = name.or(provider).unwrap_or_else(|| self.party());
                let mut detection = self.base(
                    TodoKind::Other,
                    format!("Confirm your booking at {place}"),
                    &place,
                    Origin::Schema,
                    "Reservation pending",
                );
                detection.verb = "confirm".to_string();
                detection.event_start = schema_due(starts, starts_text).map(|found| found.at);
                if let Some(start) = detection.event_start {
                    // Confirm before it starts: the start is the deadline.
                    self.set_due(
                        &mut detection,
                        Some(FoundDate {
                            at: start,
                            words: "the booking's start".to_string(),
                            checked: true,
                        }),
                        FieldSource::Schema,
                    );
                    detection.reason =
                        "a booking waiting for you to confirm it (schema.org)".to_string();
                }
                self.set_link(&mut detection, url);
                detection
            }
        }
    }

    fn payment_failed<Tz>(&self, anchor: &DateTime<Tz>) -> Detection
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let evidence = FAILED
            .find(self.text)
            .or_else(|| FIX_ASK.find(self.input.subject))
            .map_or("payment failed", |found| found.as_str());
        let party = self.party();
        let mut detection = self.base(
            TodoKind::PaymentFailed,
            format!("Fix payment for {party}"),
            &party,
            Origin::Rule,
            evidence,
        );
        let amount = pick_amount(self.text);
        let currency = amount.as_ref().map(|amount| amount.currency.clone());
        Self::set_amount(&mut detection, amount, FieldSource::Rule);
        let due = self.due(&[&BILL_DUE_SPECIFIC], anchor, currency.as_deref());
        self.set_due(&mut detection, due, FieldSource::Rule);
        // Lead with the failure, not the date: that is why it's here.
        detection.reason = format!("\"{}\" (rule)", clip(evidence, 80));
        self.set_link(&mut detection, None);
        detection
    }

    fn bill<Tz>(&self, evidence: &str, anchor: &DateTime<Tz>) -> Option<Detection>
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let amount = pick_amount(self.text);
        let currency = amount.as_ref().map(|amount| amount.currency.clone());
        let due = self.due(
            &[&BILL_DUE_SPECIFIC, &BROAD_BY],
            anchor,
            currency.as_deref(),
        );
        if amount.is_none() && due.is_none() {
            return None;
        }
        let thing = bill_thing(self.input.subject)
            .or_else(|| bill_thing(truncate(self.input.body_text, 400)));
        let party = self.party();
        let title = match &thing {
            Some(thing) => format!("Pay {thing}"),
            None if evidence.to_ascii_lowercase().contains("invoice") => {
                format!("Pay invoice from {party}")
            }
            None => format!("Pay {party}"),
        };
        let mut detection = self.base(
            TodoKind::Bill,
            title,
            thing.as_deref().unwrap_or(&party),
            Origin::Rule,
            evidence,
        );
        Self::set_amount(&mut detection, amount, FieldSource::Rule);
        self.set_due(&mut detection, due, FieldSource::Rule);
        self.set_link(&mut detection, None);
        detection.doc_type = Some(pay_method(&detection, self.text).to_string());
        Some(detection)
    }

    fn renewal<Tz>(&self, evidence: &str, anchor: &DateTime<Tz>) -> Option<Detection>
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let object = RENEWAL_OBJECT
            .captures(self.text)?
            .get(1)?
            .as_str()
            .to_ascii_lowercase();
        let due = self.due(&[&RENEWAL_DUE, &BROAD_BY], anchor, None)?;
        let (kind, doc_type) = if object.contains("passport") {
            (TodoKind::Document, Some("passport"))
        } else if object.contains("visa") {
            (TodoKind::Document, Some("visa"))
        } else if object.contains("driving") || object.contains("id card") {
            (TodoKind::Document, Some("licence"))
        } else {
            (TodoKind::Renewal, None)
        };
        let mut detection = self.base(
            kind,
            format!("Renew {object}"),
            &object,
            Origin::Rule,
            evidence,
        );
        detection.doc_type = doc_type.map(str::to_string);
        Self::set_amount(&mut detection, pick_amount(self.text), FieldSource::Rule);
        self.set_due(&mut detection, Some(due), FieldSource::Rule);
        self.set_link(&mut detection, None);
        Some(detection)
    }

    fn sign<Tz>(&self, evidence: &str, anchor: &DateTime<Tz>) -> Detection
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let object = sign_object(self.input.subject);
        let title = match &object {
            Some(object) => format!("Sign {object}"),
            None => format!("Sign the document from {}", self.party()),
        };
        let key = object.clone().unwrap_or_else(|| self.party());
        let mut detection = self.base(TodoKind::Sign, title, &key, Origin::Rule, evidence);
        let due = self.due(&[&SIGN_DUE], anchor, None);
        self.set_due(&mut detection, due, FieldSource::Rule);
        self.set_link(&mut detection, None);
        detection
    }

    fn verify<Tz>(&self, evidence: &str, anchor: &DateTime<Tz>) -> Option<Detection>
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let lower = evidence.to_ascii_lowercase();
        let verb = lower
            .split_whitespace()
            .next()
            .unwrap_or("verify")
            .to_string();
        let what = lower
            .split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ");
        let party = self.party();
        let mut detection = self.base(
            TodoKind::Verify,
            format!("{} {what} for {party}", capitalise(&verb)),
            &format!("{what} {party}"),
            Origin::Rule,
            evidence,
        );
        detection.verb = verb;
        self.set_link(&mut detection, None);
        // A verify request without its link is a notice, not a to-do.
        detection.link.as_ref()?;
        let due = find_lifetime(self.text, self.input.sent)
            .or_else(|| self.due(&[&VERIFY_DUE], anchor, None));
        self.set_due(&mut detection, due, FieldSource::Rule);
        Some(detection)
    }

    fn rsvp<Tz>(&self, evidence: &str, anchor: &DateTime<Tz>, tz: &Tz) -> Option<Detection>
    where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        let reply_by = self.due(&[&RSVP_DUE], anchor, None);
        let event = find_due(self.text, &EVENT_DATE, anchor, self.anchor_order);
        if reply_by.is_none() && event.is_none() {
            return None;
        }
        let what = event_title(self.input.subject);
        let mut detection = self.base(
            TodoKind::Rsvp,
            format!("RSVP to {what}"),
            &what,
            Origin::Rule,
            evidence,
        );
        if let Some(event) = &event {
            // The event's day, from its start: the window closes as it begins.
            let day = event.at.with_timezone(tz).date_naive();
            detection.event_start = Some(crate::timing::start_of_day(day, tz, 0));
            detection.fields.set(
                "event_start",
                FieldProvenance::with_evidence(FieldSource::Rule, event.words.clone()),
            );
        }
        self.set_due(&mut detection, reply_by, FieldSource::Rule);
        self.set_link(&mut detection, None);
        Some(detection)
    }
}

/// A to-do from a calendar invite still waiting for an answer.
pub fn from_invite(
    summary: Option<&str>,
    organizer: Option<&str>,
    from_email: &str,
    starts: Option<DateTime<Utc>>,
) -> Detection {
    let what = summary
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .map_or_else(|| "the invite".to_string(), |summary| clip(summary, 60));
    let mut fields = FieldSources::default();
    fields.set(
        "kind",
        FieldProvenance::with_evidence(FieldSource::Ics, "an invite asking for a reply"),
    );
    fields.set(
        "title",
        FieldProvenance::with_evidence(FieldSource::Ics, "the invite's summary"),
    );
    if starts.is_some() {
        fields.set(
            "event_start",
            FieldProvenance::with_evidence(FieldSource::Ics, "the invite's start"),
        );
    }
    let sender_domain = email_domain(from_email);
    Detection {
        kind: TodoKind::Rsvp,
        verb: "rsvp".to_string(),
        doc_type: None,
        title: format!("RSVP to {what}"),
        object_key: normalise(&what),
        counterparty: organizer.map(str::to_string),
        sender_domain,
        amount: None,
        due: None,
        event_start: starts,
        link: None,
        origin: Origin::Ics,
        reason: "an invite asking for your reply (calendar invite)".to_string(),
        fields,
    }
}

fn origin_label(origin: Origin) -> &'static str {
    source_label(origin.source())
}

fn source_label(source: FieldSource) -> &'static str {
    match source {
        FieldSource::Schema => "schema.org",
        FieldSource::Ics => "calendar invite",
        FieldSource::Rule | FieldSource::Table => "rule",
        FieldSource::Model => "model",
        FieldSource::User => "you",
    }
}

/// Pay link, bank transfer, or neither: picks the bill's act-by rule.
fn pay_method(detection: &Detection, text: &str) -> &'static str {
    if detection.link.is_none() && BANK_DETAILS.is_match(text) {
        "bill_bank"
    } else {
        "bill_link"
    }
}

fn bill_thing(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    BILL_THINGS.iter().find_map(|(needle, thing)| {
        let found = lower.find(needle)?;
        let before = lower[..found].chars().last();
        let after = lower[found + needle.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric());
        (boundary(before) && boundary(after)).then(|| (*thing).to_string())
    })
}

static SIGN_SUBJECT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:re:\s*|fwd?:\s*)*(?:please\s+)?(?:docusign|sign|signature\s+(?:requested|required)|review\s+and\s+sign|action\s+required)\s*(?:on|for)?\s*:\s*(.+)$")
        .expect("valid sign subject regex")
});

fn sign_object(subject: &str) -> Option<String> {
    let object = SIGN_SUBJECT
        .captures(subject.trim())?
        .get(1)?
        .as_str()
        .trim();
    (!object.is_empty()).then(|| clip(object, 60))
}

static EVENT_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:re:\s*|fwd?:\s*)*(?:rsvp|invitation|invite|you'?re invited(?: to)?|save the date|join us(?: for)?)\s*[:\-]?\s*")
        .expect("valid event prefix regex")
});

fn event_title(subject: &str) -> String {
    let stripped = EVENT_PREFIX.replace(subject.trim(), "");
    let stripped = stripped.trim();
    if stripped.is_empty() {
        "the invite".to_string()
    } else {
        clip(stripped, 60)
    }
}

/// An organisation's name from the display name, or the domain's own
/// label ("Spotify" from spotify.com) when the name is a role.
pub fn counterparty(from_name: Option<&str>, sender_domain: Option<&str>) -> Option<String> {
    let cleaned = from_name
        .map(|name| name.trim().trim_matches(['"', '\'']).trim())
        .map(|name| name.split(" via ").next().unwrap_or(name).trim())
        .filter(|name| !name.is_empty() && !name.contains('@'))
        .map(|name| {
            let mut kept = name.to_string();
            for noise in NAME_NOISE {
                let lower = kept.to_ascii_lowercase();
                if let Some(found) = lower.find(noise) {
                    kept.replace_range(found..found + noise.len(), "");
                }
            }
            kept.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '|' | ',' | ':'))
                .to_string()
        })
        .filter(|name| name.chars().any(char::is_alphabetic));
    cleaned
        .or_else(|| {
            let label = sender_domain?.split('.').next()?;
            Some(capitalise(label))
        })
        .map(|name| clip(&name, 40))
}

fn short_phrase(value: &str) -> bool {
    let words = value.split_whitespace().count();
    (1..=4).contains(&words) && !value.chars().any(|c| c.is_ascii_digit() || c == '#')
}

/// Lowercase words, single spaces: the dedup key's object.
pub fn normalise(value: &str) -> String {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
