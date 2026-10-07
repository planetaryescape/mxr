//! Relevancy windows for updates (blueprint 22, "Items have a relevancy
//! window"). The end comes from words the message quotes, else the
//! table's default; never from a model. An update past its window never
//! enters a cut and never breaks through to To do.

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc, Weekday};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// The kinds of update that carry a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    /// A one-time code: its stated lifetime, else 10 minutes (NIST SP
    /// 800-63B's maximum).
    Code,
    /// A verify or confirm link: its stated expiry, else 3 days.
    VerifyLink,
    /// A sign-in or security alert: 2 days.
    SignIn,
    /// A sale or offer: its stated end, else 7 days.
    Offer,
}

impl WindowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::VerifyLink => "verify_link",
            Self::SignIn => "sign_in",
            Self::Offer => "offer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "code" => Self::Code,
            "verify_link" => Self::VerifyLink,
            "sign_in" => Self::SignIn,
            "offer" => Self::Offer,
            _ => return None,
        })
    }

    /// "one-time code", for the expired list.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Code => "one-time code",
            Self::VerifyLink => "verify link",
            Self::SignIn => "sign-in alert",
            Self::Offer => "offer",
        }
    }

    const fn default_length(self) -> Duration {
        match self {
            Self::Code => Duration::minutes(10),
            Self::VerifyLink => Duration::days(3),
            Self::SignIn => Duration::days(2),
            Self::Offer => Duration::days(7),
        }
    }
}

/// When an update stops mattering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub kind: WindowKind,
    pub until: DateTime<Utc>,
    /// `rule` when the message stated it, `default` from the table.
    pub source: String,
    /// The words it came from, or the default it fell back to.
    pub evidence: String,
}

static CODE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b((verification|security|login|log-?in|sign-?in|one-?time|confirmation|access|authentication|auth|2fa|otp)\s+(code|pin|passcode)|your\s+(code|passcode|otp)\s+(is|:)|\botp\b|one-?time\s+pass(word|code))")
        .expect("valid code regex")
});

static VERIFY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(verify|confirm)\s+(your\s+)?(email|e-mail|account|address|sign-?up|registration|subscription)\b")
        .expect("valid verify regex")
});

static SIGN_IN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(new\s+(sign-?in|log-?in|login|device)|sign-?in\s+(attempt|alert|from|on|to)|unusual\s+(sign-?in|log-?in|login|activity)|suspicious\s+(activity|sign-?in|log-?in|login)|security\s+alert|someone\s+(signed|logged)\s+in|was\s+(this|that|it)\s+you\??|access(ed)?\s+from\s+a\s+new)")
        .expect("valid sign-in regex")
});

static OFFER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\b(sale|offer|deal|discount|promo(tion)?|coupon|voucher|flash\s+sale|black\s+friday)\b|\d+\s?%\s+off\b)")
        .expect("valid offer regex")
});

static LIFETIME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:valid|expires?|good|active|usable)\s+(?:for|in|within)\s+(?:the\s+next\s+)?(\d{1,3})\s+(minutes?|mins?|hours?|hrs?|days?)\b")
        .expect("valid lifetime regex")
});

static ENDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:ends?|ending|closes?|expires?|last\s+day|until)\s+(?:on\s+|at\s+)?(today|tonight|midnight|tomorrow|monday|tuesday|wednesday|thursday|friday|saturday|sunday|in\s+(\d{1,2})\s+(hours?|days?))\b")
        .expect("valid ends regex")
});

/// What kind of windowed update `text` (subject first, then the first
/// lines of the body) is, if any. A code wins over a sign-in alert, so a
/// "sign-in code" expires in minutes.
pub fn kind_of(text: &str) -> Option<WindowKind> {
    if CODE.is_match(text) {
        Some(WindowKind::Code)
    } else if VERIFY.is_match(text) {
        Some(WindowKind::VerifyLink)
    } else if SIGN_IN.is_match(text) {
        Some(WindowKind::SignIn)
    } else if OFFER.is_match(text) {
        Some(WindowKind::Offer)
    } else {
        None
    }
}

/// The window `text` implies for mail that arrived at `arrived`, in the
/// user's zone `tz` for "ends Sunday".
pub fn window<Tz: TimeZone>(text: &str, arrived: DateTime<Utc>, tz: &Tz) -> Option<Window> {
    let kind = kind_of(text)?;
    let stated = match kind {
        WindowKind::Code | WindowKind::VerifyLink => lifetime(text, arrived),
        WindowKind::Offer => ends(text, arrived, tz).or_else(|| lifetime(text, arrived)),
        WindowKind::SignIn => None,
    };
    Some(match stated {
        Some((until, evidence)) => Window {
            kind,
            until,
            source: "rule".to_string(),
            evidence,
        },
        None => Window {
            kind,
            until: arrived + kind.default_length(),
            source: "default".to_string(),
            evidence: format!("{} default", kind.label()),
        },
    })
}

fn lifetime(text: &str, arrived: DateTime<Utc>) -> Option<(DateTime<Utc>, String)> {
    let caps = LIFETIME.captures(text)?;
    let amount: i64 = caps.get(1)?.as_str().parse().ok()?;
    let unit = caps.get(2)?.as_str().to_ascii_lowercase();
    let length = match unit.chars().next()? {
        'm' => Duration::minutes(amount),
        'h' => Duration::hours(amount),
        _ => Duration::days(amount),
    };
    Some((arrived + length, caps.get(0)?.as_str().to_string()))
}

fn ends<Tz: TimeZone>(
    text: &str,
    arrived: DateTime<Utc>,
    tz: &Tz,
) -> Option<(DateTime<Utc>, String)> {
    let caps = ENDS.captures(text)?;
    let word = caps.get(1)?.as_str().to_ascii_lowercase();
    let evidence = caps.get(0)?.as_str().to_string();
    let local = arrived.with_timezone(tz);
    let end_of = |date: chrono::NaiveDate| {
        let end = date.and_time(NaiveTime::from_hms_opt(23, 59, 59)?);
        tz.from_local_datetime(&end)
            .latest()
            .map(|at| at.with_timezone(&Utc))
    };
    let today = local.date_naive();
    let until = match word.as_str() {
        "today" | "tonight" | "midnight" => end_of(today)?,
        "tomorrow" => end_of(today.succ_opt()?)?,
        _ if word.starts_with("in ") => {
            let amount: i64 = caps.get(2)?.as_str().parse().ok()?;
            match caps.get(3)?.as_str().chars().next()? {
                'h' | 'H' => arrived + Duration::hours(amount),
                _ => arrived + Duration::days(amount),
            }
        }
        day => {
            let wanted: Weekday = day.parse().ok()?;
            let ahead =
                (7 + wanted.num_days_from_monday() - today.weekday().num_days_from_monday()) % 7;
            end_of(today + Duration::days(i64::from(ahead)))?
        }
    };
    Some((until, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at() -> DateTime<Utc> {
        // Wednesday 7 October 2026, 10:00 UTC.
        Utc.with_ymd_and_hms(2026, 10, 7, 10, 0, 0).unwrap()
    }

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    #[test]
    fn codes_last_their_stated_lifetime_else_ten_minutes() {
        let code = window("Your verification code is 482913", at(), &utc()).unwrap();
        assert_eq!(code.kind, WindowKind::Code);
        assert_eq!(code.until, at() + Duration::minutes(10));
        assert_eq!(code.source, "default");
        let stated = window(
            "Your login code: 1234. It expires in 15 minutes.",
            at(),
            &utc(),
        )
        .unwrap();
        assert_eq!(stated.until, at() + Duration::minutes(15));
        assert_eq!(stated.source, "rule");
        // A sign-in code is a code, not a sign-in alert.
        assert_eq!(kind_of("Your sign-in code"), Some(WindowKind::Code));
    }

    #[test]
    fn sign_in_alerts_last_two_days_and_offers_their_end() {
        let alert = window("New sign-in from Chrome on Windows", at(), &utc()).unwrap();
        assert_eq!(alert.kind, WindowKind::SignIn);
        assert_eq!(alert.until, at() + Duration::days(2));
        let sale = window("Our autumn sale ends Sunday", at(), &utc()).unwrap();
        assert_eq!(sale.kind, WindowKind::Offer);
        assert_eq!(
            sale.until,
            Utc.with_ymd_and_hms(2026, 10, 11, 23, 59, 59).unwrap()
        );
        let tonight = window("20% off ends tonight", at(), &utc()).unwrap();
        assert_eq!(
            tonight.until,
            Utc.with_ymd_and_hms(2026, 10, 7, 23, 59, 59).unwrap()
        );
        let plain = window("Flash sale on now", at(), &utc()).unwrap();
        assert_eq!(plain.until, at() + Duration::days(7));
        let verify = window("Please confirm your email address", at(), &utc()).unwrap();
        assert_eq!(verify.until, at() + Duration::days(3));
        assert!(window("Your week: 3 runs", at(), &utc()).is_none());
        for kind in [
            WindowKind::Code,
            WindowKind::VerifyLink,
            WindowKind::SignIn,
            WindowKind::Offer,
        ] {
            assert_eq!(WindowKind::parse(kind.as_str()), Some(kind));
        }
    }
}
