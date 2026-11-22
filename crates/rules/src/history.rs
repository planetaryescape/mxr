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
