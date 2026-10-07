#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "unit tests unwrap to keep fixture failures direct"
    )
)]
//! Updates as a briefing by source (blueprint 22, phase 4).
//!
//! Everything here is code, no model: a message becomes a source, a
//! template, one fact line, the numbers it quotes, a signal and a
//! relevancy window. Deltas between two messages of one template are
//! computed here and only when both numbers share a unit. Digest cuts turn
//! the clock into stable sets. The daemon reads the store and the clock and
//! hands plain data in.

pub mod cuts;
pub mod fact;
pub mod numbers;
pub mod source;
pub mod template;
pub mod text;
pub mod window;

pub use cuts::{digest_title, time_label, CutError, CutWindow, Cuts};
pub use fact::{
    derive, Fact, FactInput, FactSource, NeedsYou, Tracked, TrackedKind, TrackedOutcome,
};
pub use numbers::{delta, extract_numbers, Delta, Quoted};
pub use source::{email_domain, source_key, source_name};
pub use template::{clean_subject, template_key};
pub use window::{Window, WindowKind};

use serde::{Deserialize, Serialize};

/// Bumped when a rule changes what a message derives to, so cached facts
/// are recomputed.
pub const RULES_VERSION: i64 = 1;

/// How much a message asks of you, strongest last. The digest puts a
/// source in the section of its strongest signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    Routine,
    Changed,
    NewSource,
    Anomaly,
    NeedsYou,
}

impl Signal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Routine => "routine",
            Self::Changed => "changed",
            Self::NewSource => "new_source",
            Self::Anomaly => "anomaly",
            Self::NeedsYou => "needs_you",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "routine" => Self::Routine,
            "changed" => Self::Changed,
            "new_source" => Self::NewSource,
            "anomaly" => Self::Anomaly,
            "needs_you" => Self::NeedsYou,
            _ => return None,
        })
    }

    /// Needs a look: something to check, not just to know.
    pub const fn needs_a_look(self) -> bool {
        matches!(self, Self::Anomaly | Self::NeedsYou)
    }
}

/// What the history of a source says about one message: whether the
/// source and the template were seen before and whether a quoted number
/// moved. Read from earlier facts by the daemon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct History {
    /// An earlier message from this source exists.
    pub source_seen: bool,
    /// An earlier message of this template exists.
    pub template_seen: bool,
    /// A quoted number moved against the previous message of the template.
    pub number_moved: bool,
}

/// The signal a message carries once its source's history is known. A
/// rule's needs-you or anomaly wins; otherwise a first message from a
/// source is new, a new kind of message or a moved number is a change,
/// and the rest is routine.
pub const fn signal(base: Option<Signal>, history: History) -> Signal {
    if let Some(base) = base {
        return base;
    }
    if !history.source_seen {
        Signal::NewSource
    } else if !history.template_seen || history.number_moved {
        Signal::Changed
    } else {
        Signal::Routine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_signal_wins_and_history_decides_the_rest() {
        let seen = History {
            source_seen: true,
            template_seen: true,
            number_moved: false,
        };
        assert_eq!(
            signal(Some(Signal::NeedsYou), History::default()),
            Signal::NeedsYou
        );
        assert_eq!(signal(None, History::default()), Signal::NewSource);
        assert_eq!(signal(None, seen), Signal::Routine);
        assert_eq!(
            signal(
                None,
                History {
                    template_seen: false,
                    ..seen
                }
            ),
            Signal::Changed
        );
        assert_eq!(
            signal(
                None,
                History {
                    number_moved: true,
                    ..seen
                }
            ),
            Signal::Changed
        );
        for value in ["routine", "changed", "new_source", "anomaly", "needs_you"] {
            assert_eq!(Signal::parse(value).map(Signal::as_str), Some(value));
        }
        assert!(Signal::Anomaly.needs_a_look() && !Signal::Changed.needs_a_look());
    }
}
