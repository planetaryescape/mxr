use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Conditions {
    And { conditions: Vec<Self> },
    Or { conditions: Vec<Self> },
    Not { condition: Box<Self> },
    Field(FieldCondition),
}

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkDensityMatch {
    Any,
    /// Only `Heavy` — newsletter-shaped mail.
    Heavy,
    /// Only `None` — no external links at all.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StringMatch {
    Exact(String),
    Contains(String),
    Regex(String),
    Glob(String),
}

const WHOLE_WORD_PREFIX: &str = r"(?i)(?:^|[^\p{Alphabetic}\p{N}])";
const WHOLE_WORD_SUFFIX: &str = r"(?:$|[^\p{Alphabetic}\p{N}])";

/// Inverts `regex::escape` for the literal part of a `whole_word` pattern.
/// Callers rebuild the pattern from the result and compare, so input that
/// was never produced by `regex::escape` simply fails that check.
fn unescape_regex_literal(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Patterns kept compiled. Rules are few and fixed, so the cap is only a bound
/// on a caller that feeds in many distinct patterns.
const REGEX_CACHE_CAPACITY: usize = 256;

type RegexCache = std::sync::Mutex<std::collections::HashMap<String, Option<Arc<regex::Regex>>>>;

/// The compiled form of a regex pattern, or `None` when it does not compile.
/// Rules run this for every message, so a pattern is compiled once and reused.
/// A pattern that does not compile is remembered as `None`, so it is not
/// recompiled either.
fn compiled_regex(pattern: &str) -> Option<Arc<regex::Regex>> {
    static CACHE: std::sync::OnceLock<RegexCache> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(entry) = cache.get(pattern) {
        return entry.clone();
    }
    if cache.len() >= REGEX_CACHE_CAPACITY {
        cache.clear();
    }
    let compiled = regex::Regex::new(pattern).ok().map(Arc::new);
    cache.insert(pattern.to_string(), compiled.clone());
    compiled
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
    /// The message's matchable body parts (plain and HTML, each its own
    /// string), or `None` when there is no body text to match.
    fn body_parts(&self) -> Option<Vec<&str>>;
    /// Computed link count and body word count for the message body. Default
    /// returns `(0, 0)` so existing implementations stay compatible — only
    /// MessageView impls that actually feed link-density rules need to
    /// override.
    fn link_density_inputs(&self) -> (u32, u32) {
        (0, 0)
    }
}

impl StringMatch {
    /// Case-insensitive whole-word match, the rules form of a search `+word`.
    ///
    /// Word edges are "start or end of text" or a character that the search
    /// tokenizer also splits on (anything not alphabetic or numeric), so
    /// `invoice` matches "Your invoice." but not "invoices". The word is
    /// escaped, so punctuation like `c++` or `a.b` matches literally.
    pub fn whole_word(word: &str) -> Self {
        Self::Regex(format!(
            "{WHOLE_WORD_PREFIX}{}{WHOLE_WORD_SUFFIX}",
            regex::escape(word)
        ))
    }

    /// The word a `whole_word` pattern was built from, so the rules form can
    /// show it as `+word` again. Any other pattern returns `None`.
    pub fn whole_word_term(&self) -> Option<String> {
        let Self::Regex(pattern) = self else {
            return None;
        };
        let escaped = pattern
            .strip_prefix(WHOLE_WORD_PREFIX)?
            .strip_suffix(WHOLE_WORD_SUFFIX)?;
        let word = unescape_regex_literal(escaped);
        matches!(Self::whole_word(&word), Self::Regex(rebuilt) if rebuilt == *pattern)
            .then_some(word)
    }

    pub fn matches(&self, haystack: &str) -> bool {
        match self {
            Self::Exact(s) => haystack == s,
            Self::Contains(s) => haystack.to_lowercase().contains(&s.to_lowercase()),
            Self::Regex(pattern) => compiled_regex(pattern).is_some_and(|re| re.is_match(haystack)),
            Self::Glob(pattern) => glob_match::glob_match(pattern, haystack),
        }
    }
}

impl Conditions {
    /// Unknown body data stays unknown under negation; it cannot prove a match.
    pub fn evaluate_known(&self, msg: &dyn MessageView) -> Option<bool> {
        match self {
            Self::Not { condition } => condition.evaluate_known(msg).map(|v| !v),
            Self::And { conditions } => {
                let values: Vec<_> = conditions.iter().map(|c| c.evaluate_known(msg)).collect();
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(true)
                }
            }
            Self::Or { conditions } => {
                let values: Vec<_> = conditions.iter().map(|c| c.evaluate_known(msg)).collect();
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            }
            Self::Field(FieldCondition::BodyContains { .. }) if msg.body_parts().is_none() => None,
            _ => Some(self.evaluate(msg)),
        }
    }

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
                // Any part may satisfy the pattern: a rule anchored with `^...$` or an
                // exact match is about one part's whole text.
                msg.body_parts()
                    .is_some_and(|parts| parts.iter().any(|part| pattern.matches(part)))
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

    use super::*;
    use crate::tests::*;

    #[test]
    fn contains_match_is_case_insensitive() {
        let m = StringMatch::Contains("invoice".into());
        assert!(m.matches("Re: Invoice #2847"));
        assert!(m.matches("INVOICE attached"));
        assert!(m.matches("Your invoice is ready"));
        assert!(!m.matches("Receipt attached"));
    }

    #[test]
    fn whole_word_matches_the_word_only() {
        let m = StringMatch::whole_word("invoice");
        assert!(m.matches("Your invoice."));
        assert!(m.matches("INVOICE attached"));
        assert!(m.matches("(invoice)"));
        assert!(m.matches("invoice"));
        assert!(!m.matches("Your invoices are ready"));
        assert!(!m.matches("reinvoiced"));
        assert!(!m.matches("uninvoice"));
    }

    #[test]
    fn nested_conditions_evaluate_correctly() {
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
}
