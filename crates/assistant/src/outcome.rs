use mxr_llm::{UNTRUSTED_MAIL_BEGIN, UNTRUSTED_MAIL_END};
use serde_json::Value;

pub const APPROVED_PREFIX: &str = "The user approved this call.\n";
pub const AUTO_APPROVED_PREFIX: &str =
    "This reversible change ran without asking: the user lets this conversation approve those.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    Done,
    Failed,
    Approved,
    AutoApproved,
    Declined,
    Answered,
    Cancelled,
}

pub fn call_state(content: &str, is_error: bool) -> CallState {
    if content.starts_with(APPROVED_PREFIX) {
        return if is_error || refused(content) {
            CallState::Failed
        } else {
            CallState::Approved
        };
    }
    if content.starts_with(AUTO_APPROVED_PREFIX) {
        return if is_error || refused(content) {
            CallState::Failed
        } else {
            CallState::AutoApproved
        };
    }
    if let Ok(value) = serde_json::from_str::<Value>(content) {
        if value["source"] == "user" {
            if value.get("answers").is_some() {
                return CallState::Answered;
            }
            match value["status"].as_str() {
                Some("rejected" | "not_answered") => return CallState::Declined,
                Some("cancelled") => return CallState::Cancelled,
                _ => {}
            }
        }
    }
    if is_error || refused(content) {
        CallState::Failed
    } else {
        CallState::Done
    }
}

/// mxr's own guard refused the call, such as a change sent without its
/// confirm flag. Nothing ran, so it reads as a failure, not a finished step
/// the model could report as done.
pub fn refused(content: &str) -> bool {
    serde_json::from_str::<Value>(&readable_result(content, usize::MAX))
        .is_ok_and(|value| value.get("blocked").and_then(Value::as_bool) == Some(true))
}

pub fn readable_result(content: &str, max_chars: usize) -> String {
    let content = content
        .strip_prefix(APPROVED_PREFIX)
        .or_else(|| content.strip_prefix(AUTO_APPROVED_PREFIX))
        .unwrap_or(content);
    let content = content
        .strip_prefix(UNTRUSTED_MAIL_BEGIN)
        .map_or(content, |rest| rest.strip_prefix('\n').unwrap_or(rest));
    let content = content
        .strip_suffix(UNTRUSTED_MAIL_END)
        .map_or(content, |rest| rest.strip_suffix('\n').unwrap_or(rest));
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let mut kept: String = content.chars().take(max_chars).collect();
    kept.push('…');
    kept
}
