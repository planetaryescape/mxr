use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Composable condition tree. Evaluated recursively.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Conditions {
    And { conditions: Vec<Conditions> },
    Or { conditions: Vec<Conditions> },
    Not { condition: Box<Conditions> },
    Field(FieldCondition),
}

/// Leaf-level condition against a single message field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FieldCondition {
    From { pattern: StringMatch },
    To { pattern: StringMatch },
    Subject { pattern: StringMatch },
    HasLabel { label: String },
    HasAttachment,
    SizeGreaterThan { bytes: u64 },
    SizeLessThan { bytes: u64 },
    DateAfter { date: DateTime<Utc> },
    DateBefore { date: DateTime<Utc> },
    IsUnread,
    IsStarred,
    HasUnsubscribe,
    BodyContains { pattern: StringMatch },
}

/// How to match a string field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StringMatch {
    Exact(String),
    Contains(String),
    Regex(String),
    Glob(String),
}

/// A message-like view for condition evaluation.
/// The engine evaluates conditions against this trait so it
/// doesn't depend on mxr-core's concrete Envelope type directly.
pub trait MessageView {
    fn sender_email(&self) -> &str;
    fn to_emails(&self) -> &[String];
    fn subject(&self) -> &str;
    fn labels(&self) -> &[String];
    fn has_attachment(&self) -> bool;
    fn size_bytes(&self) -> u64;
    fn date(&self) -> DateTime<Utc>;
    fn is_unread(&self) -> bool;
    fn is_starred(&self) -> bool;
    fn has_unsubscribe(&self) -> bool;
    fn body_text(&self) -> Option<&str>;
}

impl StringMatch {
    /// Evaluate this match against a haystack string.
    pub fn matches(&self, haystack: &str) -> bool {
        match self {
            StringMatch::Exact(s) => haystack == s,
            StringMatch::Contains(s) => haystack.to_lowercase().contains(&s.to_lowercase()),
            StringMatch::Regex(pattern) => regex::Regex::new(pattern)
                .map(|re| re.is_match(haystack))
                .unwrap_or(false),
            StringMatch::Glob(pattern) => glob_match::glob_match(pattern, haystack),
        }
    }
}

impl Conditions {
    /// Recursively evaluate the condition tree against a message.
    pub fn evaluate(&self, msg: &dyn MessageView) -> bool {
        match self {
            Conditions::And { conditions } => conditions.iter().all(|c| c.evaluate(msg)),
            Conditions::Or { conditions } => conditions.iter().any(|c| c.evaluate(msg)),
            Conditions::Not { condition } => !condition.evaluate(msg),
            Conditions::Field(field) => field.evaluate(msg),
        }
    }
}

impl FieldCondition {
    pub fn evaluate(&self, msg: &dyn MessageView) -> bool {
        match self {
            FieldCondition::From { pattern } => pattern.matches(msg.sender_email()),
            FieldCondition::To { pattern } => msg.to_emails().iter().any(|e| pattern.matches(e)),
            FieldCondition::Subject { pattern } => pattern.matches(msg.subject()),
            FieldCondition::HasLabel { label } => msg.labels().iter().any(|l| l == label),
            FieldCondition::HasAttachment => msg.has_attachment(),
            FieldCondition::SizeGreaterThan { bytes } => msg.size_bytes() > *bytes,
            FieldCondition::SizeLessThan { bytes } => msg.size_bytes() < *bytes,
            FieldCondition::DateAfter { date } => msg.date() > *date,
            FieldCondition::DateBefore { date } => msg.date() < *date,
            FieldCondition::IsUnread => msg.is_unread(),
            FieldCondition::IsStarred => msg.is_starred(),
            FieldCondition::HasUnsubscribe => msg.has_unsubscribe(),
            FieldCondition::BodyContains { pattern } => {
                msg.body_text().is_some_and(|body| pattern.matches(body))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::tests::*;

    // -- StringMatch tests --

    #[test]
    fn exact_match_requires_full_equality() {
        let m = StringMatch::Exact("alice@example.com".into());
        assert!(m.matches("alice@example.com"));
        assert!(!m.matches("ALICE@example.com"));
        assert!(!m.matches("alice@example.com "));
    }

    #[test]
    fn contains_match_is_case_insensitive() {
        let m = StringMatch::Contains("invoice".into());
        assert!(m.matches("Re: Invoice #2847"));
        assert!(m.matches("INVOICE attached"));
        assert!(m.matches("Your invoice is ready"));
        assert!(!m.matches("Receipt attached"));
    }

    #[test]
    fn contains_match_handles_unicode() {
        let m = StringMatch::Contains("café".into());
        assert!(m.matches("Meeting at Café Luna"));
    }

    #[test]
    fn glob_match_with_wildcards() {
        let m = StringMatch::Glob("*@substack.com".into());
        assert!(m.matches("newsletter@substack.com"));
        assert!(m.matches("alice@substack.com"));
        assert!(!m.matches("newsletter@gmail.com"));
    }

    #[test]
    fn glob_match_with_question_mark() {
        let m = StringMatch::Glob("user?@example.com".into());
        assert!(m.matches("user1@example.com"));
        assert!(m.matches("userA@example.com"));
        assert!(!m.matches("user12@example.com"));
    }

    #[test]
    fn regex_match_complex_pattern() {
        let m = StringMatch::Regex(r"(?i)invoice\s*#\d+".into());
        assert!(m.matches("Re: Invoice #2847"));
        assert!(m.matches("invoice#100"));
        assert!(!m.matches("Receipt attached"));
    }

    #[test]
    fn regex_invalid_pattern_returns_false() {
        let m = StringMatch::Regex(r"[invalid".into());
        assert!(!m.matches("anything"));
    }

    // -- Condition composition tests --

    #[test]
    fn and_condition_requires_all_true() {
        let cond = Conditions::And {
            conditions: vec![
                Conditions::Field(FieldCondition::HasLabel {
                    label: "newsletters".into(),
                }),
                Conditions::Field(FieldCondition::HasUnsubscribe),
            ],
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn and_condition_short_circuits_on_false() {
        let cond = Conditions::And {
            conditions: vec![
                Conditions::Field(FieldCondition::IsStarred),
                Conditions::Field(FieldCondition::HasUnsubscribe),
            ],
        };
        assert!(!cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn or_condition_succeeds_on_any_true() {
        let cond = Conditions::Or {
            conditions: vec![
                Conditions::Field(FieldCondition::IsStarred),
                Conditions::Field(FieldCondition::HasUnsubscribe),
            ],
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn or_condition_fails_when_all_false() {
        let cond = Conditions::Or {
            conditions: vec![
                Conditions::Field(FieldCondition::IsStarred),
                Conditions::Field(FieldCondition::HasAttachment),
            ],
        };
        assert!(!cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn not_condition_inverts() {
        let cond = Conditions::Not {
            condition: Box::new(Conditions::Field(FieldCondition::IsStarred)),
        };
        assert!(cond.evaluate(&newsletter_msg())); // not starred
    }

    #[test]
    fn nested_conditions_evaluate_correctly() {
        // (from matches *@substack.com AND has_unsub) OR is_starred
        let cond = Conditions::Or {
            conditions: vec![
                Conditions::And {
                    conditions: vec![
                        Conditions::Field(FieldCondition::From {
                            pattern: StringMatch::Glob("*@substack.com".into()),
                        }),
                        Conditions::Field(FieldCondition::HasUnsubscribe),
                    ],
                },
                Conditions::Field(FieldCondition::IsStarred),
            ],
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn empty_and_is_vacuously_true() {
        let cond = Conditions::And { conditions: vec![] };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn empty_or_is_false() {
        let cond = Conditions::Or { conditions: vec![] };
        assert!(!cond.evaluate(&newsletter_msg()));
    }

    // -- Field condition tests --

    #[test]
    fn from_field_matches_email() {
        let cond = FieldCondition::From {
            pattern: StringMatch::Contains("substack".into()),
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn to_field_matches_any_recipient() {
        let cond = FieldCondition::To {
            pattern: StringMatch::Exact("user@example.com".into()),
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn subject_field_matches() {
        let cond = FieldCondition::Subject {
            pattern: StringMatch::Contains("Rust".into()),
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn has_label_checks_label_list() {
        let cond = FieldCondition::HasLabel {
            label: "newsletters".into(),
        };
        assert!(cond.evaluate(&newsletter_msg()));

        let cond = FieldCondition::HasLabel {
            label: "work".into(),
        };
        assert!(!cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn size_conditions_work() {
        let msg = newsletter_msg(); // size 15000

        let cond = FieldCondition::SizeGreaterThan { bytes: 10000 };
        assert!(cond.evaluate(&msg));

        let cond = FieldCondition::SizeGreaterThan { bytes: 20000 };
        assert!(!cond.evaluate(&msg));

        let cond = FieldCondition::SizeLessThan { bytes: 20000 };
        assert!(cond.evaluate(&msg));
    }

    #[test]
    fn date_conditions_work() {
        let msg = newsletter_msg(); // date is Utc::now()
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let future = chrono::Utc::now() + chrono::Duration::hours(1);

        let cond = FieldCondition::DateAfter { date: past };
        assert!(cond.evaluate(&msg));

        let cond = FieldCondition::DateBefore { date: future };
        assert!(cond.evaluate(&msg));
    }

    #[test]
    fn body_contains_searches_text() {
        let cond = FieldCondition::BodyContains {
            pattern: StringMatch::Contains("weekly Rust digest".into()),
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn body_contains_returns_false_when_no_body() {
        let mut msg = newsletter_msg();
        msg.body = None;
        let cond = FieldCondition::BodyContains {
            pattern: StringMatch::Contains("anything".into()),
        };
        assert!(!cond.evaluate(&msg));
    }

    #[test]
    fn has_attachment_checks_flag() {
        assert!(!FieldCondition::HasAttachment.evaluate(&newsletter_msg()));

        let mut msg = newsletter_msg();
        msg.has_attachment = true;
        assert!(FieldCondition::HasAttachment.evaluate(&msg));
    }

    // -- Serialization tests --

    #[test]
    fn conditions_roundtrip_through_json() {
        let cond = Conditions::And {
            conditions: vec![
                Conditions::Field(FieldCondition::From {
                    pattern: StringMatch::Glob("*@substack.com".into()),
                }),
                Conditions::Not {
                    condition: Box::new(Conditions::Field(FieldCondition::IsStarred)),
                },
            ],
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: Conditions = serde_json::from_str(&json).unwrap();
        // Verify it still evaluates the same
        assert!(parsed.evaluate(&newsletter_msg()));
    }
}
