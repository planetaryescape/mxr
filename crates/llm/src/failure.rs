//! Telling a failing route from a failing request.
//!
//! A route failure (bad key, outage, rate limit, unknown model) will hit the
//! next request too, so it goes into [`crate::health::RouteHealth`]. A request
//! failure (a refusal, an empty answer, a malformed request) says nothing
//! about the route: the API answered. Marking the route bad for it would
//! send every later request to the fallback over one odd prompt.

use crate::health::{FailureReason, RouteFailure};
use crate::LlmError;
use std::time::Duration;

#[derive(Debug)]
pub(crate) enum ErrorClass {
    Request,
    Route(RouteFailure),
}

/// A failed attempt: what the caller is told, and what it says about the
/// route.
#[derive(Debug)]
pub(crate) struct AttemptError {
    pub error: LlmError,
    pub class: ErrorClass,
}

impl AttemptError {
    pub(crate) fn request(error: LlmError) -> Self {
        Self {
            error,
            class: ErrorClass::Request,
        }
    }

    pub(crate) fn route(error: LlmError, failure: RouteFailure) -> Self {
        Self {
            error,
            class: ErrorClass::Route(failure),
        }
    }

    pub(crate) fn timed_out(after: Duration) -> Self {
        Self::route(
            LlmError::Timeout(after),
            RouteFailure::Cooling {
                reason: FailureReason::plain("timed out"),
                retry_after: None,
            },
        )
    }

    pub(crate) fn unreachable(message: String) -> Self {
        Self::route(
            LlmError::Unreachable(message),
            RouteFailure::Cooling {
                reason: FailureReason::plain("unreachable"),
                retry_after: None,
            },
        )
    }

    pub(crate) fn into_error(self) -> LlmError {
        self.error
    }
}

/// How an HTTP error status reflects on the route. `body` is the reply text,
/// read only to recognise "unknown model"; it never reaches the reason.
pub(crate) fn classify_status(
    status: u16,
    retry_after: Option<Duration>,
    body: &str,
) -> ErrorClass {
    let cooling = |reason| {
        ErrorClass::Route(RouteFailure::Cooling {
            reason,
            retry_after,
        })
    };
    let broken = |reason| ErrorClass::Route(RouteFailure::Broken { reason });
    match status {
        401 => broken(FailureReason::status(401, "invalid key")),
        403 => broken(FailureReason::status(403, "key not permitted")),
        // Both providers answer 402 for an account that is out of credit.
        402 => broken(FailureReason::status(402, "billing problem")),
        404 => broken(FailureReason::status(404, "model not found")),
        400 if names_unknown_model(body) => broken(FailureReason::status(400, "model not found")),
        408 => cooling(FailureReason::status(408, "request timeout")),
        429 => cooling(FailureReason::status(429, "rate limited")),
        529 => cooling(FailureReason::status(529, "overloaded")),
        500..=599 => cooling(FailureReason::status(status, "server error")),
        _ => ErrorClass::Request,
    }
}

/// Anthropic answers an unknown model with 404; OpenRouter often with 400
/// and a message naming the model. Any other 400 is about the request.
fn names_unknown_model(body: &str) -> bool {
    let body = body.to_ascii_lowercase();
    body.contains("not a valid model")
        || body.contains("no endpoints found")
        || body.contains("model not found")
        || (body.contains("model") && body.contains("does not exist"))
}

/// `retry-after` as a delay. Only the seconds form is read; the HTTP-date
/// form falls back to the route's own backoff.
pub(crate) fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value?.trim().parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route_failure(class: ErrorClass) -> RouteFailure {
        match class {
            ErrorClass::Route(failure) => failure,
            ErrorClass::Request => panic!("expected a route failure"),
        }
    }

    #[test]
    fn key_and_account_problems_are_broken_until_something_changes() {
        for (status, text) in [
            (401, "401 invalid key"),
            (403, "403 key not permitted"),
            (402, "402 billing problem"),
            (404, "404 model not found"),
        ] {
            let RouteFailure::Broken { reason } = route_failure(classify_status(status, None, ""))
            else {
                panic!("{status} should be broken");
            };
            assert_eq!(reason.to_string(), text);
        }
    }

    #[test]
    fn a_400_is_a_route_failure_only_when_it_names_an_unknown_model() {
        for body in [
            r#"{"error":{"message":"anthropic/claude-haiku-9.9 is not a valid model ID"}}"#,
            r#"{"error":{"message":"No endpoints found for anthropic/claude-x"}}"#,
        ] {
            assert!(matches!(
                route_failure(classify_status(400, None, body)),
                RouteFailure::Broken { .. }
            ));
        }
        for body in [
            r#"{"error":{"type":"invalid_request_error","message":"max_tokens: must be positive"}}"#,
            "",
        ] {
            assert!(matches!(
                classify_status(400, None, body),
                ErrorClass::Request
            ));
        }
    }

    #[test]
    fn outages_and_limits_cool_and_keep_the_servers_retry_after() {
        let after = Some(Duration::from_secs(12));
        for (status, text) in [
            (429, "429 rate limited"),
            (529, "529 overloaded"),
            (500, "500 server error"),
            (503, "503 server error"),
            (408, "408 request timeout"),
        ] {
            let RouteFailure::Cooling {
                reason,
                retry_after,
            } = route_failure(classify_status(status, after, ""))
            else {
                panic!("{status} should be cooling");
            };
            assert_eq!(reason.to_string(), text);
            assert_eq!(retry_after, after);
        }
    }

    #[test]
    fn other_client_errors_are_about_the_request() {
        for status in [405, 409, 413, 422] {
            assert!(
                matches!(classify_status(status, None, ""), ErrorClass::Request),
                "{status}"
            );
        }
    }

    #[test]
    fn the_failure_reason_never_carries_reply_text() {
        let body = "sk-ant-leaked-key and the user's prompt text";
        let RouteFailure::Broken { reason } = route_failure(classify_status(404, None, body))
        else {
            panic!("expected broken");
        };
        assert!(!reason.to_string().contains("sk-ant"));
    }

    #[test]
    fn retry_after_reads_seconds_only() {
        assert_eq!(
            parse_retry_after(Some(" 30 ")),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(Some("-5")), None);
        assert_eq!(parse_retry_after(None), None);
    }
}
