//! One message, one fact: what an update says in a line, with the numbers
//! it quotes, its rule signal, its window and, for builds and incidents,
//! the state of the thing it tracks. Rules only (blueprint 22, phase 4);
//! the fast tier arrives in phase 7.

use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::numbers::{extract_numbers, Quoted};
use crate::source::{source_key, source_name};
use crate::template::{clean_subject, template_key};
use crate::text::{capitalise, clip, first_informative_line, pick_link};
use crate::window::{window, Window, WindowKind};
use crate::Signal;

/// Longest fact line, in characters.
pub const FACT_MAX: usize = 140;
/// Numbers kept per message.
const NUMBERS_MAX: usize = 6;

/// What one message gives the rules.
#[derive(Debug, Clone, Copy)]
pub struct FactInput<'a> {
    pub subject: &'a str,
    /// Cleaned plain text of the body, when stored.
    pub body: Option<&'a str>,
    pub snippet: &'a str,
    pub from_email: &'a str,
    pub from_name: Option<&'a str>,
    pub list_id: Option<&'a str>,
    pub date: DateTime<Utc>,
}

/// Where the fact line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactSource {
    /// The cleaned subject.
    Subject,
    /// The first informative line of the body, for a generic subject.
    Body,
}

impl FactSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Body => "body",
        }
    }
}

/// Why an update leads Needs a look with a suggested to-do. mxr never
/// adds the to-do itself; `t` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedsYou {
    SignIn,
    PaymentFailed,
    DeliveryException,
}

impl NeedsYou {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SignIn => "sign_in",
            Self::PaymentFailed => "payment_failed",
            Self::DeliveryException => "delivery_exception",
        }
    }

    /// "new sign-in alert", for the why line.
    pub const fn label(self) -> &'static str {
        match self {
            Self::SignIn => "new sign-in alert",
            Self::PaymentFailed => "failed payment",
            Self::DeliveryException => "delivery problem",
        }
    }
}

/// A thing with a lifecycle that mail reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedKind {
    Build,
    Incident,
}

impl TrackedKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Incident => "incident",
        }
    }
}

/// How a tracked thing stands: ended well, ended badly, or under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedOutcome {
    Good,
    Bad,
    Progress,
}

impl TrackedOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Progress => "progress",
        }
    }
}

/// The state one message reports for a tracked thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracked {
    pub kind: TrackedKind,
    /// The thing, across messages: the template with its state masked,
    /// so "Run failed: CI - main" and "Run succeeded: CI - main" are one.
    pub key: String,
    /// The state word as written: "failed", "resolved".
    pub state: String,
    pub outcome: TrackedOutcome,
}

/// Everything the rules derive from one message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub source_key: String,
    pub source_name: String,
    pub template_key: String,
    /// One line: "Run failed: CI - main", "New sign-in from Chrome on
    /// Windows".
    pub text: String,
    pub fact_source: FactSource,
    pub numbers: Vec<Quoted>,
    /// Needs-you or anomaly from a rule; the daemon adds history.
    pub base_signal: Option<Signal>,
    pub needs_you: Option<NeedsYou>,
    pub window: Option<Window>,
    pub tracked: Option<Tracked>,
    /// The one place to go deeper. Never on a needs-you line or a link
    /// about money: those open the email.
    pub link: Option<String>,
    /// What `t` prefills: "Check new sign-in to Google
    /// (accounts.google.com)". The sending host sits beside the display
    /// name so branding is visible.
    pub todo_title: String,
    /// The host the mail came from, as the address says it:
    /// "accounts.google.com".
    #[serde(default)]
    pub sender_host: String,
}

/// A generic subject says nothing on its own: the body's first
/// informative line is the fact instead.
static GENERIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?ix)^(
        (your\s+|a\s+|the\s+)?(new\s+|latest\s+|weekly\s+|daily\s+|monthly\s+)?
        (update|notification|summary|digest|report|activity|news|newsletter|alert|message|recap|roundup)s?
        (\s+(is\s+)?(here|ready|available|inside))?
      | you\s+have\s+(a\s+|\d+\s+)?(new\s+|unread\s+)?(notification|message|activity|alert|update)s?
      | (important\s+)?(account\s+)?(update|notice|information)
      | (there'?s|there\s+is)\s+(something\s+)?new.*
      | (hello|hi)\b.*
      | \(no\s+subject\)
    )[.!]?$")
    .expect("valid generic regex")
});

static PAYMENT_FAILED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\b(payment|payout|charge|transfer|direct\s+debit|renewal|invoice)\b[^\n]{0,40}?\b(failed|declined|unsuccessful|didn'?t\s+go\s+through|could\s+not\s+be\s+(processed|completed|collected)|was\s+returned|bounced)\b|\bcard\s+(was\s+|has\s+been\s+)?declined\b|\b(couldn'?t|could\s+not|unable\s+to|were\s+unable\s+to)\s+(process|charge|collect|take)\s+(your\s+)?(latest\s+|recent\s+)?(payment|card))")
        .expect("valid payment failed regex")
});

static ANOMALY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(failed|failing|failure|down|outage|degraded|broken|declined|suspended|blocked|rejected|bounced|overdue|exceeded|critical|errored|could\s+not|couldn'?t|unable\s+to|action\s+required|attention\s+required|immediate\s+action)\b")
        .expect("valid anomaly regex")
});

static BUILD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(build|run|workflow|pipeline|deploy|deployment|checks?|job|ci|tests?)\b")
        .expect("valid build regex")
});

static INCIDENT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(incident|outage|degraded|disruption|service\s+issue)\b|^\s*\[(investigating|identified|monitoring|resolved)\]")
        .expect("valid incident regex")
});

static STATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(still\s+failing|passed|succeeded|successful|success|fixed|green|completed|resolved|recovered|failed|failing|failure|broken|errored|red|cancell?ed|investigating|identified|monitoring|scheduled|update)\b")
        .expect("valid state regex")
});

/// Money in a link: never opened from a row.
static MONEY_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(pay|billing|checkout|invoice|payment|wallet|bank)")
        .expect("valid money link regex")
});

fn outcome(state: &str) -> TrackedOutcome {
    match state.to_ascii_lowercase().as_str() {
        "passed" | "succeeded" | "successful" | "success" | "fixed" | "green" | "completed"
        | "resolved" | "recovered" => TrackedOutcome::Good,
        "failed" | "failing" | "failure" | "broken" | "errored" | "red" | "investigating"
        | "identified" => TrackedOutcome::Bad,
        state if state.starts_with("still") => TrackedOutcome::Bad,
        _ => TrackedOutcome::Progress,
    }
}

/// The tracked thing a subject reports on. Read from the subject with its
/// tags, since a status page puts the state in one ("[Resolved]").
fn tracked(subject: &str) -> Option<Tracked> {
    let state = STATE.find(subject)?;
    let outcome = outcome(state.as_str());
    let kind = if INCIDENT.is_match(subject) {
        TrackedKind::Incident
    } else if BUILD.is_match(subject) && outcome != TrackedOutcome::Progress {
        TrackedKind::Build
    } else {
        return None;
    };
    let masked = STATE.replace_all(subject, "<state>");
    Some(Tracked {
        kind,
        key: template_key(&masked),
        state: state.as_str().to_ascii_lowercase(),
        outcome,
    })
}

/// Derive one message's fact. `tz` resolves "sale ends Sunday".
pub fn derive<Tz: TimeZone>(input: &FactInput<'_>, tz: &Tz) -> Fact {
    let key = source_key(input.from_email, input.list_id, input.subject);
    let name = source_name(input.from_name, input.from_email, &key);
    let cleaned = clean_subject(input.subject, &name);
    let body_line = input
        .body
        .and_then(|body| first_informative_line(body, FACT_MAX))
        .or_else(|| first_informative_line(input.snippet, FACT_MAX));
    let generic = cleaned.is_empty() || GENERIC.is_match(&cleaned);
    let tracked = tracked(input.subject);
    let (text, fact_source) = match (&body_line, generic) {
        (Some(line), true) => (line.clone(), FactSource::Body),
        _ if cleaned.is_empty() => ("(no subject)".to_string(), FactSource::Subject),
        // A state only the tag carried leads the line: "Resolved: ...".
        _ => match &tracked {
            Some(t) if !cleaned.to_lowercase().contains(&t.state) => (
                clip(&format!("{}: {cleaned}", capitalise(&t.state)), FACT_MAX),
                FactSource::Subject,
            ),
            _ => (clip(&cleaned, FACT_MAX), FactSource::Subject),
        },
    };

    let mut numbers = extract_numbers(&text, NUMBERS_MAX);
    if fact_source == FactSource::Subject {
        if let Some(line) = &body_line {
            for quoted in extract_numbers(line, NUMBERS_MAX) {
                if numbers.len() < NUMBERS_MAX && !numbers.iter().any(|n| n.raw == quoted.raw) {
                    numbers.push(quoted);
                }
            }
        }
    }

    // Rules read the subject and the first informative line together.
    let rule_text = match &body_line {
        Some(line) => format!("{cleaned}\n{line}"),
        None => cleaned.clone(),
    };
    let window = window(
        &cleaned,
        body_line.as_deref().unwrap_or_default(),
        input.date,
        tz,
    );
    let needs_you = if PAYMENT_FAILED.is_match(&rule_text) {
        Some(NeedsYou::PaymentFailed)
    } else if window
        .as_ref()
        .is_some_and(|w| w.kind == WindowKind::SignIn)
    {
        Some(NeedsYou::SignIn)
    } else {
        None
    };
    let base_signal = if needs_you.is_some() {
        Some(Signal::NeedsYou)
    } else {
        match &tracked {
            Some(t) if t.outcome == TrackedOutcome::Bad => Some(Signal::Anomaly),
            Some(_) => None,
            None => ANOMALY.is_match(&cleaned).then_some(Signal::Anomaly),
        }
    };
    let link = if needs_you.is_some() {
        None
    } else {
        input
            .body
            .and_then(pick_link)
            .filter(|url| !MONEY_LINK.is_match(url))
    };
    let sender_host = sender_host(input.from_email);
    let branded = branded_name(&name, &sender_host);
    let todo_title = match needs_you {
        Some(NeedsYou::SignIn) => format!("Check new sign-in to {branded}"),
        Some(NeedsYou::PaymentFailed) => format!("Fix failed payment to {branded}"),
        Some(NeedsYou::DeliveryException) => format!("Check delivery from {branded}"),
        None => clip(&format!("Check {branded}: {text}"), 120),
    };
    Fact {
        template_key: template_key(input.subject),
        source_key: key,
        source_name: name,
        text,
        fact_source,
        numbers,
        base_signal,
        needs_you,
        window,
        tracked,
        link,
        todo_title,
        sender_host,
    }
}

/// The host part of an address, lowercased: "accounts.google.com".
pub fn sender_host(from_email: &str) -> String {
    from_email
        .trim()
        .rsplit_once('@')
        .map(|(_, host)| host.trim_end_matches(['.', '>']).to_ascii_lowercase())
        .unwrap_or_default()
}

/// "Google (accounts.google.com)": a display name anyone can choose,
/// with the host the mail really came from beside it.
pub fn branded_name(name: &str, host: &str) -> String {
    if host.is_empty() || name.eq_ignore_ascii_case(host) {
        name.to_string()
    } else {
        format!("{name} ({host})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, FixedOffset};

    fn input<'a>(
        from: &'a str,
        name: Option<&'a str>,
        subject: &'a str,
        body: Option<&'a str>,
    ) -> FactInput<'a> {
        FactInput {
            subject,
            body,
            snippet: "",
            from_email: from,
            from_name: name,
            list_id: None,
            date: DateTime::UNIX_EPOCH + Duration::days(20_000),
        }
    }

    fn derive_utc(input: &FactInput<'_>) -> Fact {
        derive(input, &FixedOffset::east_opt(0).unwrap())
    }

    #[test]
    fn a_generic_subject_takes_the_body_line_and_its_numbers() {
        let fact = derive_utc(&input(
            "no-reply@strava.com",
            Some("Strava"),
            "Your weekly update is here",
            Some("Hi Bhekani,\n\nYou ran 3 times this week: 3 runs, 21.3 km.\n\nView in browser"),
        ));
        assert_eq!(fact.fact_source, FactSource::Body);
        assert_eq!(fact.text, "You ran 3 times this week: 3 runs, 21.3 km.");
        assert_eq!(
            fact.numbers
                .iter()
                .map(|n| n.raw.as_str())
                .collect::<Vec<_>>(),
            vec!["3 runs", "21.3 km"]
        );
        assert_eq!(fact.source_key, "strava.com");
        assert_eq!(fact.base_signal, None);
    }

    #[test]
    fn sign_ins_and_failed_payments_need_you_and_lose_their_link() {
        let sign_in = derive_utc(&input(
            "no-reply@accounts.google.com",
            Some("Google"),
            "Security alert: New sign-in from Chrome on Windows",
            Some("Check activity https://myaccount.google.com/notifications"),
        ));
        assert_eq!(sign_in.needs_you, Some(NeedsYou::SignIn));
        assert_eq!(sign_in.base_signal, Some(Signal::NeedsYou));
        assert_eq!(sign_in.link, None);
        assert_eq!(
            sign_in.todo_title,
            "Check new sign-in to Google (accounts.google.com)"
        );
        assert_eq!(sign_in.window.map(|w| w.kind), Some(WindowKind::SignIn));

        let payout = derive_utc(&input(
            "notifications@stripe.com",
            Some("Stripe"),
            "Payout of R 4,210.00 failed: bank declined",
            None,
        ));
        assert_eq!(payout.needs_you, Some(NeedsYou::PaymentFailed));
        assert_eq!(payout.numbers[0].raw, "R 4,210.00");
    }

    #[test]
    fn builds_and_incidents_are_tracked_by_their_state() {
        let failing = derive_utc(&input(
            "notifications@github.com",
            Some("GitHub"),
            "[acme/api] Run failed: CI - main (a1b2c3d)",
            Some("View workflow run https://github.com/acme/api/actions/runs/9"),
        ));
        let fixed = derive_utc(&input(
            "notifications@github.com",
            Some("GitHub"),
            "[acme/api] Run succeeded: CI - main (9f8e7d6)",
            None,
        ));
        let (a, b) = (failing.tracked.unwrap(), fixed.tracked.unwrap());
        assert_eq!(a.kind, TrackedKind::Build);
        assert_eq!(a.key, b.key);
        assert_eq!(a.outcome, TrackedOutcome::Bad);
        assert_eq!(b.outcome, TrackedOutcome::Good);
        assert_eq!(failing.base_signal, Some(Signal::Anomaly));
        assert_eq!(fixed.base_signal, None);
        assert_eq!(failing.source_key, "github.com/acme/api");
        assert_eq!(failing.source_name, "GitHub acme/api");
        assert_eq!(
            failing.link.as_deref(),
            Some("https://github.com/acme/api/actions/runs/9")
        );

        let incident = derive_utc(&input(
            "status@statuspage.io",
            Some("Linear Status"),
            "[Resolved] Degraded performance on the API",
            None,
        ));
        let tracked = incident.tracked.unwrap();
        assert_eq!(tracked.kind, TrackedKind::Incident);
        assert_eq!(tracked.outcome, TrackedOutcome::Good);
        assert_eq!(incident.base_signal, None);
        assert_eq!(incident.text, "Resolved: Degraded performance on the API");
    }

    #[test]
    fn a_borrowed_brand_shows_the_host_it_came_from() {
        let fake = derive_utc(&input(
            "alerts@g00gle-security.example",
            Some("Google"),
            "Security alert: New sign-in from Chrome on Windows",
            None,
        ));
        assert_eq!(
            fake.todo_title,
            "Check new sign-in to Google (g00gle-security.example)"
        );
        assert_eq!(fake.sender_host, "g00gle-security.example");
    }

    #[test]
    fn money_links_are_never_kept() {
        let fact = derive_utc(&input(
            "billing@example.com",
            None,
            "Your receipt",
            Some("View invoice https://example.com/billing/invoice/12"),
        ));
        assert_eq!(fact.link, None);
    }
}
