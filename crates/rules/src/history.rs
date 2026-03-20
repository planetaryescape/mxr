use crate::RuleId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A log entry for a rule execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatchEntry {
    pub rule_id: RuleId,
    pub rule_name: String,
    pub message_id: String,
    pub actions_applied: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub error: Option<String>,
}

/// Factory for rule execution log entries.
/// Stored in SQLite for auditability ("why was this archived?").
pub struct RuleExecutionLog;

impl RuleExecutionLog {
    /// Create a log entry for a rule match.
    /// Caller persists this via the store.
    pub fn entry(
        rule_id: &RuleId,
        rule_name: &str,
        message_id: &str,
        actions: &[String],
        success: bool,
        error: Option<&str>,
    ) -> RuleMatchEntry {
        RuleMatchEntry {
            rule_id: rule_id.clone(),
            rule_name: rule_name.to_string(),
            message_id: message_id.to_string(),
            actions_applied: actions.to_vec(),
            timestamp: Utc::now(),
            success,
            error: error.map(String::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_captures_success() {
        let entry = RuleExecutionLog::entry(
            &RuleId("r1".into()),
            "Archive newsletters",
            "msg_123",
            &["archive".into()],
            true,
            None,
        );

        assert_eq!(entry.rule_id, RuleId("r1".into()));
        assert_eq!(entry.rule_name, "Archive newsletters");
        assert_eq!(entry.message_id, "msg_123");
        assert!(entry.success);
        assert!(entry.error.is_none());
        assert_eq!(entry.actions_applied, vec!["archive"]);
    }

    #[test]
    fn entry_captures_failure() {
        let entry = RuleExecutionLog::entry(
            &RuleId("r1".into()),
            "Shell hook",
            "msg_456",
            &["shell_hook".into()],
            false,
            Some("command not found"),
        );

        assert!(!entry.success);
        assert_eq!(entry.error.as_deref(), Some("command not found"));
    }

    #[test]
    fn entry_timestamp_is_recent() {
        let before = Utc::now();
        let entry = RuleExecutionLog::entry(
            &RuleId("r1".into()),
            "test",
            "msg",
            &[],
            true,
            None,
        );
        let after = Utc::now();

        assert!(entry.timestamp >= before);
        assert!(entry.timestamp <= after);
    }

    #[test]
    fn entry_roundtrips_through_json() {
        let entry = RuleExecutionLog::entry(
            &RuleId("r1".into()),
            "test rule",
            "msg_1",
            &["archive".into(), "mark_read".into()],
            true,
            None,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RuleMatchEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rule_id, entry.rule_id);
        assert_eq!(parsed.actions_applied.len(), 2);
    }
}
