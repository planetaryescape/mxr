//! Turns a non-success Gmail response into a typed error. Both the real client
//! and the test double read responses through here, so they cannot drift.

use crate::error::GmailError;
use chrono::{DateTime, Utc};

const DEFAULT_RETRY_AFTER_SECS: u64 = 60;
/// Google's error text is short. The cap only stops a surprising reply from
/// filling a sync status line or a log line.
const TEXT_CAP_CHARS: usize = 200;
/// Reasons Google sends with HTTP 403 and `domain: usageLimits` when a quota
/// is exhausted. Any other 403 (insufficientPermissions and so on) is not a rate limit.
const USAGE_LIMIT_REASONS: [&str; 3] = [
    "rateLimitExceeded",
    "userRateLimitExceeded",
    "dailyLimitExceeded",
];

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RateLimitRefusal {
    pub retry_after_secs: u64,
    pub reason: Option<String>,
    pub message: Option<String>,
}

impl RateLimitRefusal {
    /// `rateLimitExceeded: User-rate limit exceeded`, from whatever Gmail sent.
    pub(crate) fn describe(&self) -> Option<String> {
        match (&self.reason, &self.message) {
            (Some(reason), Some(message)) => Some(format!("{reason}: {message}")),
            (Some(reason), None) => Some(reason.clone()),
            (None, Some(message)) => Some(message.clone()),
            (None, None) => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ErrorResponse {
    AuthExpired,
    NotFound(String),
    RateLimited(RateLimitRefusal),
    Api { status: u16, body: String },
}

impl From<ErrorResponse> for GmailError {
    fn from(response: ErrorResponse) -> Self {
        match response {
            ErrorResponse::AuthExpired => Self::AuthExpired,
            ErrorResponse::NotFound(body) => Self::NotFound(body),
            ErrorResponse::RateLimited(refusal) => Self::RateLimited {
                retry_after_secs: refusal.retry_after_secs,
                detail: refusal.describe(),
            },
            ErrorResponse::Api { status, body } => Self::Api { status, body },
        }
    }
}

pub(crate) async fn read_error_response(resp: reqwest::Response) -> ErrorResponse {
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    match status {
        401 => ErrorResponse::AuthExpired,
        404 => ErrorResponse::NotFound(resp.text().await.unwrap_or_default()),
        _ => {
            let body = resp.text().await.unwrap_or_default();
            match classify_rate_limit(status, retry_after.as_deref(), &body, Utc::now()) {
                Some(refusal) => ErrorResponse::RateLimited(refusal),
                None => ErrorResponse::Api { status, body },
            }
        }
    }
}

/// `None` unless the response is a rate limit: every 429, and a 403 whose
/// error reason is a usage-limit reason. The wait is the Retry-After header,
/// else the "Retry after <RFC 3339>" time in Gmail's message, else 60 s.
pub(crate) fn classify_rate_limit(
    status: u16,
    retry_after_header: Option<&str>,
    body: &str,
    now: DateTime<Utc>,
) -> Option<RateLimitRefusal> {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let error = json.as_ref().and_then(|json| json.get("error"));
    let first = error
        .and_then(|error| error.get("errors"))
        .and_then(|errors| errors.get(0));
    let reason = first
        .and_then(|first| first.get("reason"))
        .and_then(|reason| reason.as_str());
    let domain = first
        .and_then(|first| first.get("domain"))
        .and_then(|domain| domain.as_str());
    let is_rate_limit = match status {
        429 => true,
        403 => {
            domain == Some("usageLimits")
                && reason.is_some_and(|reason| USAGE_LIMIT_REASONS.contains(&reason))
        }
        _ => false,
    };
    if !is_rate_limit {
        return None;
    }

    let raw_message = error
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str());
    let (message, timestamp_wait) = split_retry_time(raw_message, now);
    let retry_after_secs = retry_after_header
        .and_then(|value| value.trim().parse::<u64>().ok())
        .or(timestamp_wait)
        .unwrap_or(DEFAULT_RETRY_AFTER_SECS);

    Some(RateLimitRefusal {
        retry_after_secs,
        reason: reason.and_then(clean_text),
        message: message.as_deref().and_then(clean_text),
    })
}

/// Splits `User-rate limit exceeded.  Retry after 2026-…Z` into the message
/// without the retry clause and the seconds until that time (at least 1).
fn split_retry_time(message: Option<&str>, now: DateTime<Utc>) -> (Option<String>, Option<u64>) {
    let Some(message) = message else {
        return (None, None);
    };
    // to_ascii_lowercase keeps byte offsets, so `at` is valid in `message`.
    let Some(at) = message.to_ascii_lowercase().find("retry after") else {
        return (Some(message.to_string()), None);
    };
    let head = message[..at].to_string();
    let stamp = message[at + "retry after".len()..]
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');
    let wait = DateTime::parse_from_rfc3339(stamp).ok().map(|until| {
        let secs = (until.with_timezone(&Utc) - now).num_seconds().max(1);
        u64::try_from(secs).unwrap_or(1)
    });
    (Some(head), wait)
}

/// Collapses whitespace and caps the length. None when nothing is left.
fn clean_text(text: &str) -> Option<String> {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .to_string();
    (!collapsed.is_empty()).then(|| collapsed.chars().take(TEXT_CAP_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 21, 19, 0, 0).unwrap()
    }

    const BODY_429_STAMP: &str = r#"{"error":{"code":429,"message":"User-rate limit exceeded.  Retry after 2026-10-21T19:05:31.123Z","errors":[{"message":"User-rate limit exceeded.  Retry after 2026-10-21T19:05:31.123Z","domain":"usageLimits","reason":"rateLimitExceeded"}],"status":"RESOURCE_EXHAUSTED"}}"#;

    #[test]
    fn a_429_with_retry_after_header_uses_the_header() {
        let refusal = classify_rate_limit(429, Some("30"), BODY_429_STAMP, now()).unwrap();

        assert_eq!(refusal.retry_after_secs, 30);
        assert_eq!(refusal.reason.as_deref(), Some("rateLimitExceeded"));
        assert_eq!(
            refusal.message.as_deref(),
            Some("User-rate limit exceeded"),
            "the retry clause is reported as seconds, not repeated in the message"
        );
    }

    #[test]
    fn a_429_without_header_waits_until_the_time_in_the_message() {
        let refusal = classify_rate_limit(429, None, BODY_429_STAMP, now()).unwrap();

        assert_eq!(refusal.retry_after_secs, 5 * 60 + 31);
    }

    #[test]
    fn a_429_with_neither_header_nor_timestamp_waits_sixty_seconds() {
        let body = r#"{"error":{"code":429,"message":"Too many concurrent requests for user"}}"#;

        let refusal = classify_rate_limit(429, None, body, now()).unwrap();

        assert_eq!(refusal.retry_after_secs, 60);
        assert_eq!(refusal.reason, None);
        assert_eq!(
            refusal.describe().as_deref(),
            Some("Too many concurrent requests for user")
        );
    }

    #[test]
    fn a_non_json_429_is_still_a_rate_limit_waiting_sixty_seconds() {
        let refusal = classify_rate_limit(429, None, "<html>slow down</html>", now()).unwrap();

        assert_eq!(refusal.retry_after_secs, 60);
        assert_eq!(refusal.describe(), None);
    }

    #[test]
    fn a_timestamp_already_past_waits_one_second() {
        let body =
            r#"{"error":{"message":"User-rate limit exceeded. Retry after 2026-10-21T18:00:00Z"}}"#;

        let refusal = classify_rate_limit(429, None, body, now()).unwrap();

        assert_eq!(refusal.retry_after_secs, 1);
    }

    #[test]
    fn a_403_usage_limit_is_a_rate_limit() {
        let body = r#"{"error":{"code":403,"message":"Rate Limit Exceeded","errors":[{"domain":"usageLimits","reason":"rateLimitExceeded"}]}}"#;

        let refusal = classify_rate_limit(403, None, body, now()).unwrap();

        assert_eq!(refusal.retry_after_secs, 60);
        assert_eq!(refusal.reason.as_deref(), Some("rateLimitExceeded"));
    }

    #[test]
    fn a_403_daily_limit_is_a_rate_limit() {
        let body = r#"{"error":{"code":403,"message":"Daily Limit Exceeded","errors":[{"domain":"usageLimits","reason":"dailyLimitExceeded"}]}}"#;

        assert!(classify_rate_limit(403, None, body, now()).is_some());
    }

    #[test]
    fn a_403_that_is_not_a_usage_limit_stays_an_api_error() {
        let permissions = r#"{"error":{"code":403,"message":"Insufficient Permission","errors":[{"domain":"global","reason":"insufficientPermissions"}]}}"#;
        let other_domain = r#"{"error":{"code":403,"message":"x","errors":[{"domain":"global","reason":"rateLimitExceeded"}]}}"#;

        assert_eq!(classify_rate_limit(403, None, permissions, now()), None);
        assert_eq!(classify_rate_limit(403, None, other_domain, now()), None);
        assert_eq!(classify_rate_limit(403, None, "not json", now()), None);
    }

    #[test]
    fn other_statuses_are_never_rate_limits() {
        assert_eq!(
            classify_rate_limit(500, Some("30"), BODY_429_STAMP, now()),
            None
        );
    }

    #[test]
    fn a_header_that_is_not_seconds_falls_back_to_the_message() {
        let refusal = classify_rate_limit(
            429,
            Some("Wed, 21 Oct 2026 19:05:00 GMT"),
            BODY_429_STAMP,
            now(),
        )
        .unwrap();

        assert_eq!(refusal.retry_after_secs, 5 * 60 + 31);
    }

    #[test]
    fn long_or_multiline_text_is_capped_and_flattened() {
        let message = format!("line one\n{}", "x".repeat(500));
        let body = serde_json::json!({
            "error": {"message": message, "errors": [{"domain": "usageLimits", "reason": "rateLimitExceeded"}]}
        })
        .to_string();

        let refusal = classify_rate_limit(403, None, &body, now()).unwrap();
        let text = refusal.message.unwrap();

        assert_eq!(text.chars().count(), TEXT_CAP_CHARS);
        assert!(text.starts_with("line one xxx"));
    }

    #[test]
    fn describe_names_reason_and_message_together() {
        let refusal = RateLimitRefusal {
            retry_after_secs: 1,
            reason: Some("rateLimitExceeded".into()),
            message: Some("User-rate limit exceeded".into()),
        };

        assert_eq!(
            refusal.describe().as_deref(),
            Some("rateLimitExceeded: User-rate limit exceeded")
        );
    }
}
