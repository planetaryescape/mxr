//! Whether an alert earns its own to-do on arrival (blueprint 22, D193).
//!
//! Pure: the daemon reads the store and the clock and hands plain data in.
//! Every input is the provider's, the store's or the clock's; nothing here
//! reads the sender's `Date`, display name or subject, which an attacker
//! writes. The same function decides at arrival, when it may create a
//! to-do, and in the digest, when it only explains why one was not made.

use crate::NeedsYou;
use chrono::{DateTime, Duration, TimeZone, Utc};

pub const MIN_HISTORY_MESSAGES: u32 = 3;
/// How long before the alert the host's oldest earlier message must be.
pub const MIN_HISTORY_DAYS: i64 = 30;
/// An alert older than this (by the receiver's authenticated handling time) never creates.
pub const FRESH_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct History {
    pub messages: u32,
    pub oldest: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct AlertInput<'a> {
    pub kind: NeedsYou,
    pub enabled: bool,
    /// The provider vouched for this host (a DMARC pass for the From host).
    pub verified_host: Option<&'a str>,
    /// Why not, when it did not: an `UnverifiedReason` code.
    pub unverified_code: Option<&'a str>,
    /// The receiver's authenticated handling time time, never the `Date` header.
    pub received_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
    pub in_inbox: bool,
    pub is_live: bool,
    pub unread: bool,
    pub list_mail: bool,
    pub window_until: Option<DateTime<Utc>>,
    pub history: History,
    pub established: &'a [(String, u32)],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub host: String,
    pub kind: NeedsYou,
    pub key: String,
}

/// Why no to-do was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    SwitchedOff,
    /// The provider did not vouch for the sender; carries the reason code.
    SenderNotVerified(String),
    ParcelNotTied,
    Seen,
    TooOld,
    Backfill,
    MailingList,
    ThinHistory,
    Lookalike(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Create(Claim),
    Hold(Held),
}

impl Held {
    pub fn code(&self) -> String {
        match self {
            Self::SwitchedOff => "switched_off".into(),
            Self::SenderNotVerified(code) => code.clone(),
            Self::ParcelNotTied => "parcel_not_tied".into(),
            Self::Seen => "seen".into(),
            Self::TooOld => "too_old".into(),
            Self::Backfill => "backfill".into(),
            Self::MailingList => "mailing_list".into(),
            Self::ThinHistory => "thin_history".into(),
            Self::Lookalike(_) => "lookalike".into(),
        }
    }

    /// The sentence for the suggestion row. `outlook`: the account is an
    /// Outlook.com or Microsoft 365 one, whose server gives no verdict.
    /// None when the row needs no explanation (the user already dealt with it).
    pub fn text(&self, host: &str, outlook: bool) -> Option<String> {
        let because = match self {
            Self::Seen => return None,
            Self::SwitchedOff => "automatic alert to-dos are switched off".to_string(),
            Self::SenderNotVerified(code) => match code.as_str() {
                "imap_gives_no_verdict" if outlook => {
                    "Outlook doesn't give mxr a verified sender".to_string()
                }
                "imap_gives_no_verdict" => {
                    "your mail server doesn't give mxr a verified sender".to_string()
                }
                "gmail_verdict_unproven" => "mxr could not verify Gmail’s receiver evidence for this message".to_string(),
                "not_recorded" => "it arrived before mxr kept sender checks".to_string(),
                "no_verdict_header" => "your provider didn't say who sent it".to_string(),
                "ambiguous_verdict" => "your provider's sender check was ambiguous".to_string(),
                "no_receipt_time" => "your provider gave no authenticated handling time".to_string(),
                _ => "your provider did not confirm the sender".to_string(),
            },
            Self::ParcelNotTied => "mxr can't tie this parcel to one verified email yet".into(),
            Self::TooOld => "its verified receiver handling time is outside the day-long window".into(),
            Self::Backfill => "it was fetched as history, not a new arrival".into(),
            Self::MailingList => "it came through a mailing list".into(),
            Self::ThinHistory => format!(
                "mxr has fewer than {MIN_HISTORY_MESSAGES} earlier emails from {host} over {MIN_HISTORY_DAYS} days"
            ),
            Self::Lookalike(known) => format!("{host} looks like {known}"),
        };
        Some(format!("Not created automatically: {because}."))
    }
}

/// Decide, in the order a reader would ask: is it on, is the sender
/// vouched for, is it new and unhandled, is it a source you know.
pub fn decide<Tz: TimeZone>(input: &AlertInput<'_>, tz: &Tz) -> Decision {
    if !input.enabled {
        return Decision::Hold(Held::SwitchedOff);
    }
    let Some(host) = input.verified_host else {
        return Decision::Hold(Held::SenderNotVerified(
            input.unverified_code.unwrap_or("not_recorded").to_string(),
        ));
    };
    let Some(received) = input.received_at else {
        return Decision::Hold(Held::SenderNotVerified("no_receipt_time".into()));
    };
    if !input.is_live {
        return Decision::Hold(Held::Backfill);
    }
    if !input.in_inbox || !input.unread {
        return Decision::Hold(Held::Seen);
    }
    if received > input.now
        || input.now - received > Duration::hours(FRESH_HOURS)
        || input.window_until.is_some_and(|until| until <= input.now)
    {
        return Decision::Hold(Held::TooOld);
    }
    if input.list_mail {
        return Decision::Hold(Held::MailingList);
    }
    let known = input.history.messages >= MIN_HISTORY_MESSAGES
        && input
            .history
            .oldest
            .is_some_and(|oldest| received - oldest >= Duration::days(MIN_HISTORY_DAYS));
    if !known {
        return Decision::Hold(Held::ThinHistory);
    }
    if let Some(look) = lookalike_of(host, input.history.messages, input.established) {
        return Decision::Hold(Held::Lookalike(look));
    }
    let day = received.with_timezone(tz).date_naive();
    Decision::Create(Claim {
        host: host.to_string(),
        kind: input.kind,
        key: format!("alert|{host}|{}|{day}", input.kind.as_str()),
    })
}

/// "Check new sign-in · accounts.google.com": the verified host, never the
/// display name.
pub fn todo_title(kind: NeedsYou, host: &str) -> String {
    match kind {
        NeedsYou::SignIn => format!("Check new sign-in · {host}"),
        NeedsYou::PaymentFailed => format!("Fix failed payment · {host}"),
        NeedsYou::DeliveryException => format!("Check delivery · {host}"),
    }
}

/// A known host this one imitates, if it has more history than this host.
/// Internationalised and non-ASCII hosts never count as known: a real
/// alert sender is plain ASCII, and folding Unicode confusables is a
/// larger table than this check earns.
pub fn lookalike_of(
    host: &str,
    own_messages: u32,
    established: &[(String, u32)],
) -> Option<String> {
    if !host.is_ascii() || host.split('.').any(|label| label.starts_with("xn--")) {
        return Some("an internationalised domain".to_string());
    }
    let own = registrable(host);
    let own_skeleton = skeleton(&own);
    established
        .iter()
        .filter(|(other, count)| *count > own_messages && other.as_str() != host)
        .map(|(other, _)| other)
        .find(|other| {
            let theirs = registrable(other);
            theirs != own
                && (skeleton(&theirs) == own_skeleton
                    || (own.len() >= 6 && theirs.len() >= 6 && one_edit(&own, &theirs)))
        })
        .cloned()
}

fn registrable(host: &str) -> String {
    psl::domain_str(host).map_or_else(|| host.to_string(), str::to_string)
}

/// Collapses the pairs a reader's eye merges: `rn` and `m`, `vv` and `w`,
/// `0` and `o`, `1`, `i`, `l`; hyphens vanish.
fn skeleton(domain: &str) -> String {
    domain
        .replace("rn", "m")
        .replace("vv", "w")
        .chars()
        .filter(|c| *c != '-')
        .map(|c| match c {
            '0' => 'o',
            '1' | 'i' | '|' => 'l',
            c => c,
        })
        .collect()
}

/// Optimal string alignment distance of at most one: one insertion,
/// deletion, substitution or adjacent transposition.
fn one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a == b {
        return false;
    }
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let (ra, rb) = (&a[prefix..], &b[prefix..]);
    match ra.len().cmp(&rb.len()) {
        std::cmp::Ordering::Equal => {
            ra[1..] == rb[1..]
                || (ra.len() >= 2 && ra[0] == rb[1] && ra[1] == rb[0] && ra[2..] == rb[2..])
        }
        std::cmp::Ordering::Greater => ra[1..] == *rb,
        std::cmp::Ordering::Less => *ra == rb[1..],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    const NOW: &str = "2026-11-06T10:00:00Z";

    fn base<'a>(established: &'a [(String, u32)]) -> AlertInput<'a> {
        AlertInput {
            kind: NeedsYou::SignIn,
            enabled: true,
            verified_host: Some("accounts.google.com"),
            unverified_code: None,
            received_at: Some(at("2026-11-06T09:30:00Z")),
            now: at(NOW),
            in_inbox: true,
            is_live: true,
            unread: true,
            list_mail: false,
            window_until: Some(at("2026-11-08T09:30:00Z")),
            history: History {
                messages: 12,
                oldest: Some(at("2026-01-02T00:00:00Z")),
            },
            established,
        }
    }

    fn held(input: &AlertInput<'_>) -> Held {
        match decide(input, &utc()) {
            Decision::Hold(held) => held,
            Decision::Create(claim) => std::panic::resume_unwind(Box::new(claim.key)),
        }
    }

    #[test]
    fn a_verified_alert_from_a_known_source_creates_one_claim() {
        let established = [("accounts.google.com".to_string(), 12)];
        let Decision::Create(claim) = decide(&base(&established), &utc()) else {
            std::panic::resume_unwind(Box::new("expected a to-do"));
        };
        assert_eq!(claim.key, "alert|accounts.google.com|sign_in|2026-11-06");
        assert_eq!(
            todo_title(claim.kind, &claim.host),
            "Check new sign-in · accounts.google.com"
        );
    }

    #[test]
    fn an_unverified_sender_never_creates_and_says_why() {
        let mut input = base(&[]);
        input.verified_host = None;
        input.unverified_code = Some("imap_gives_no_verdict");
        let held = held(&input);
        assert_eq!(
            held.text("x", true).unwrap(),
            "Not created automatically: Outlook doesn't give mxr a verified sender."
        );
        assert_eq!(
            held.text("x", false).unwrap(),
            "Not created automatically: your mail server doesn't give mxr a verified sender."
        );
        input.unverified_code = None;
        assert_eq!(
            super::decide(&input, &utc()),
            Decision::Hold(Held::SenderNotVerified("not_recorded".into()))
        );
    }

    #[test]
    fn the_candidate_cannot_be_its_own_history() {
        let mut input = base(&[]);
        input.history = History {
            messages: 0,
            oldest: None,
        };
        assert_eq!(held(&input), Held::ThinHistory);
        input.history = History {
            messages: 2,
            oldest: Some(at("2026-01-02T00:00:00Z")),
        };
        assert_eq!(held(&input), Held::ThinHistory);
        input.history = History {
            messages: 5,
            oldest: Some(at("2026-10-20T00:00:00Z")),
        };
        assert_eq!(held(&input), Held::ThinHistory);
    }

    #[test]
    fn a_lookalike_of_a_better_known_host_is_held() {
        let established = [
            ("accounts.google.com".to_string(), 40),
            ("stripe.com".to_string(), 30),
        ];
        for fake in [
            "accounts.g00gle.com",
            "accounts.goog1e.com",
            "accounts.gogle.com",
            "accounts.goolge.com",
            "stripe.co",
            "str1pe.com",
            "rnail.stripe.com",
        ] {
            let found = lookalike_of(fake, 4, &established);
            let expect = !matches!(fake, "rnail.stripe.com");
            assert_eq!(found.is_some(), expect, "{fake}: {found:?}");
        }
        assert_eq!(lookalike_of("github.com", 4, &established), None);
        assert!(lookalike_of("accounts.xn--ggle-55da.com", 4, &established).is_some());
        assert!(lookalike_of("gооgle.com", 4, &established).is_some());
        assert_eq!(lookalike_of("stripe.co", 31, &established), None);
    }
}
