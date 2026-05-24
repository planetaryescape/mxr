use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Composable condition tree. Evaluated recursively.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Conditions {
    And { conditions: Vec<Self> },
    Or { conditions: Vec<Self> },
    Not { condition: Box<Self> },
    Field(FieldCondition),
}

/// Leaf-level condition against a single message field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FieldCondition {
    From {
        pattern: StringMatch,
    },
    To {
        pattern: StringMatch,
    },
    Subject {
        pattern: StringMatch,
    },
    HasLabel {
        label: String,
    },
    HasAttachment,
    SizeGreaterThan {
        bytes: u64,
    },
    SizeLessThan {
        bytes: u64,
    },
    DateAfter {
        date: DateTime<Utc>,
    },
    DateBefore {
        date: DateTime<Utc>,
    },
    IsUnread,
    IsStarred,
    HasUnsubscribe,
    BodyContains {
        pattern: StringMatch,
    },
    /// Match on the tri-state link-density classification computed at sync
    /// time. `match_kind: "any"` covers `Some` and `Heavy`; `"heavy"` covers
    /// only `Heavy`; `"none"` covers the no-links tier. Lets users write
    /// rules like "auto-archive link-heavy mail from unknown senders".
    LinkDensity {
        match_kind: LinkDensityMatch,
    },
}

/// How to match the tri-state `LinkDensity` classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkDensityMatch {
    /// `Some` or `Heavy` — any external link survived the deny-list filter.
    Any,
    /// Only `Heavy` — newsletter-shaped mail.
    Heavy,
    /// Only `None` — no external links at all.
    None,
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
    /// Computed link count and body word count for the message body. Default
    /// returns `(0, 0)` so existing implementations stay compatible — only
    /// MessageView impls that actually feed link-density rules need to
    /// override.
    fn link_density_inputs(&self) -> (u32, u32) {
        (0, 0)
    }
}

impl StringMatch {
    /// Evaluate this match against a haystack string.
    pub fn matches(&self, haystack: &str) -> bool {
        match self {
            Self::Exact(s) => haystack == s,
            Self::Contains(s) => haystack.to_lowercase().contains(&s.to_lowercase()),
            Self::Regex(pattern) => {
                regex::Regex::new(pattern).is_ok_and(|re| re.is_match(haystack))
            }
            Self::Glob(pattern) => glob_match::glob_match(pattern, haystack),
        }
    }
}

impl Conditions {
    /// Recursively evaluate the condition tree against a message.
    pub fn evaluate(&self, msg: &dyn MessageView) -> bool {
        match self {
            Self::And { conditions } => conditions.iter().all(|c| c.evaluate(msg)),
            Self::Or { conditions } => conditions.iter().any(|c| c.evaluate(msg)),
            Self::Not { condition } => !condition.evaluate(msg),
            Self::Field(field) => field.evaluate(msg),
        }
    }
}

impl FieldCondition {
    pub fn evaluate(&self, msg: &dyn MessageView) -> bool {
        match self {
            Self::From { pattern } => pattern.matches(msg.sender_email()),
            Self::To { pattern } => msg.to_emails().iter().any(|e| pattern.matches(e)),
            Self::Subject { pattern } => pattern.matches(msg.subject()),
            Self::HasLabel { label } => msg.labels().iter().any(|l| l == label),
            Self::HasAttachment => msg.has_attachment(),
            Self::SizeGreaterThan { bytes } => msg.size_bytes() > *bytes,
            Self::SizeLessThan { bytes } => msg.size_bytes() < *bytes,
            Self::DateAfter { date } => msg.date() > *date,
            Self::DateBefore { date } => msg.date() < *date,
            Self::IsUnread => msg.is_unread(),
            Self::IsStarred => msg.is_starred(),
            Self::HasUnsubscribe => msg.has_unsubscribe(),
            Self::BodyContains { pattern } => {
                msg.body_text().is_some_and(|body| pattern.matches(body))
            }
            Self::LinkDensity { match_kind } => {
                let (link_count, body_word_count) = msg.link_density_inputs();
                let tier =
                    mxr_core::types::Envelope::classify_link_density(link_count, body_word_count);
                matches!(
                    (match_kind, tier),
                    (
                        LinkDensityMatch::Any,
                        mxr_core::types::LinkDensity::Some | mxr_core::types::LinkDensity::Heavy
                    ) | (LinkDensityMatch::Heavy, mxr_core::types::LinkDensity::Heavy)
                        | (LinkDensityMatch::None, mxr_core::types::LinkDensity::None)
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

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
