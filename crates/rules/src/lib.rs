pub mod action;
pub mod condition;
pub mod engine;
pub mod history;
pub mod shell_hook;

pub use action::{RuleAction, SnoozeDuration};
pub use condition::{Conditions, FieldCondition, MessageView, StringMatch};
pub use engine::{DryRunMatch, DryRunResult, EvaluationResult, RuleEngine};
pub use history::{RuleExecutionLog, RuleMatchEntry};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleId(pub String);

impl RuleId {
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }
}

impl Default for RuleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A declarative mail rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: RuleId,
    pub name: String,
    pub enabled: bool,
    /// Lower number = runs first.
    pub priority: i32,
    pub conditions: Conditions,
    pub actions: Vec<RuleAction>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::condition::MessageView;
    use chrono::{DateTime, Utc};

    /// Test message implementation for condition evaluation.
    pub(crate) struct TestMessage {
        pub from: String,
        pub to: Vec<String>,
        pub subject: String,
        pub labels: Vec<String>,
        pub has_attachment: bool,
        pub size: u64,
        pub date: DateTime<Utc>,
        pub is_unread: bool,
        pub is_starred: bool,
        pub has_unsub: bool,
        pub body: Option<String>,
    }

    impl MessageView for TestMessage {
        fn sender_email(&self) -> &str {
            &self.from
        }
        fn to_emails(&self) -> &[String] {
            &self.to
        }
        fn subject(&self) -> &str {
            &self.subject
        }
        fn labels(&self) -> &[String] {
            &self.labels
        }
        fn has_attachment(&self) -> bool {
            self.has_attachment
        }
        fn size_bytes(&self) -> u64 {
            self.size
        }
        fn date(&self) -> DateTime<Utc> {
            self.date
        }
        fn is_unread(&self) -> bool {
            self.is_unread
        }
        fn is_starred(&self) -> bool {
            self.is_starred
        }
        fn has_unsubscribe(&self) -> bool {
            self.has_unsub
        }
        fn body_text(&self) -> Option<&str> {
            self.body.as_deref()
        }
    }

    pub(crate) fn newsletter_msg() -> TestMessage {
        TestMessage {
            from: "newsletter@substack.com".into(),
            to: vec!["user@example.com".into()],
            subject: "This Week in Rust #580".into(),
            labels: vec!["INBOX".into(), "newsletters".into()],
            has_attachment: false,
            size: 15000,
            date: Utc::now(),
            is_unread: false,
            is_starred: false,
            has_unsub: true,
            body: Some("Here's your weekly Rust digest...".into()),
        }
    }

    pub(crate) fn work_email_msg() -> TestMessage {
        TestMessage {
            from: "boss@company.com".into(),
            to: vec!["user@example.com".into()],
            subject: "Q1 Review".into(),
            labels: vec!["INBOX".into(), "work".into()],
            has_attachment: true,
            size: 52000,
            date: Utc::now(),
            is_unread: true,
            is_starred: true,
            has_unsub: false,
            body: Some("Please review the attached Q1 report.".into()),
        }
    }
}
