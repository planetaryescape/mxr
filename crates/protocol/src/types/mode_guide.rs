//! How each mode explains itself in place (D118, amended 2026-10-07): the
//! header line, both empty states, the why template, the keys with their
//! verbs, the explanation `?` leads with, and the hints. A hint is one
//! sentence attached to one element, shown the first time that element is
//! needed and never again once dismissed. There is no page-top card. One
//! table, served by `Request::GetModeGuide`, so the web app, the TUI, the
//! CLI and agents say the same words.
//!
//! A mode joins the table in the phase that ships it; user-facing copy
//! never describes a mode before it exists.

use serde::{Deserialize, Serialize};

use super::{archive_copy, messages_copy, now_copy, todo_copy};

/// One key and the verb it does in this mode: "e tick off".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModeKeyData {
    /// As the clients print it: "Enter", "e", ",".
    pub key: String,
    pub verb: String,
}

/// One hint as a client shows it: the copy, the key it names, and whether
/// it was dismissed on this profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HintData {
    /// Stable across releases, since the seen state is keyed on it:
    /// "now.from_mode".
    pub id: String,
    /// The element it attaches to, for agents and the docs: "the first
    /// row's why line".
    pub anchor: String,
    /// One sentence that names its key.
    pub text: String,
    /// The key the hint names, with its verb in this mode.
    pub key: ModeKeyData,
    /// Dismissed by Esc or by acting on the element, in any client.
    pub seen: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Returned in `ResponseData::ModeGuides`: one mode's teaching copy and
/// its hints with their seen state on this profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModeGuideData {
    /// The id clients and `mxr modes explain` use: "todo".
    pub mode: String,
    /// "To do".
    pub name: String,
    /// Under the mode's name, always visible: the job and its verb.
    pub header: String,
    /// The empty state before anything has ever landed here.
    pub never_had_any: String,
    /// How to add one by hand from a conversation, for the never-had-any
    /// state.
    pub add_one: String,
    /// The empty state once everything is handled. The runway appends when
    /// the next thing shows up.
    pub clear_for_now: String,
    /// One line on what lands here, for `?`.
    pub lands_here: String,
    /// How the mode works in one or two sentences. Only `?` and `mxr modes
    /// explain` show it: nothing teaches at the top of the page.
    pub about: String,
    /// How every row's why line reads: `{evidence}` and `{source}` are
    /// filled per row.
    pub why_template: String,
    /// Every key the mode answers to, with its verb, for `?` and the footer.
    pub keys: Vec<ModeKeyData>,
    /// The first-run summary's line for this mode.
    pub first_run_line: String,
    /// Each attached to one element in this mode.
    pub hints: Vec<HintData>,
}

/// One hint, as compiled in. `HintData` adds the seen state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintCopy {
    pub id: &'static str,
    pub anchor: &'static str,
    pub text: &'static str,
    pub key: (&'static str, &'static str),
}

impl HintCopy {
    pub fn to_data(&self, seen_at: Option<chrono::DateTime<chrono::Utc>>) -> HintData {
        HintData {
            id: self.id.to_string(),
            anchor: self.anchor.to_string(),
            text: self.text.to_string(),
            key: ModeKeyData {
                key: self.key.0.to_string(),
                verb: self.key.1.to_string(),
            },
            seen: seen_at.is_some(),
            seen_at,
        }
    }
}

/// One mode's copy, as compiled in. `ModeGuideData` adds the hints' seen
/// state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeGuideCopy {
    pub mode: &'static str,
    pub name: &'static str,
    pub header: &'static str,
    pub never_had_any: &'static str,
    pub add_one: &'static str,
    pub clear_for_now: &'static str,
    pub lands_here: &'static str,
    pub about: &'static str,
    pub why_template: &'static str,
    pub keys: &'static [(&'static str, &'static str)],
    pub first_run_line: &'static str,
    pub hints: &'static [HintCopy],
}

impl ModeGuideCopy {
    /// `seen_at` answers when a hint id was dismissed, or `None`.
    pub fn to_data(
        &self,
        seen_at: impl Fn(&str) -> Option<chrono::DateTime<chrono::Utc>>,
    ) -> ModeGuideData {
        ModeGuideData {
            mode: self.mode.to_string(),
            name: self.name.to_string(),
            header: self.header.to_string(),
            never_had_any: self.never_had_any.to_string(),
            add_one: self.add_one.to_string(),
            clear_for_now: self.clear_for_now.to_string(),
            lands_here: self.lands_here.to_string(),
            about: self.about.to_string(),
            why_template: self.why_template.to_string(),
            keys: self
                .keys
                .iter()
                .map(|(key, verb)| ModeKeyData {
                    key: (*key).to_string(),
                    verb: (*verb).to_string(),
                })
                .collect(),
            first_run_line: self.first_run_line.to_string(),
            hints: self
                .hints
                .iter()
                .map(|hint| hint.to_data(seen_at(hint.id)))
                .collect(),
        }
    }
}

/// Done here on a conversation, in the toast after the first `e`: the
/// moment a Gmail habit (archive means gone) meets the mode rule.
pub const DONE_HERE_HINT: HintCopy = HintCopy {
    id: "done_here",
    anchor: "The toast after the first e (done here) on a conversation",
    text: "Done here (e) only clears this mode; it stays in To do until done there.",
    key: ("e", "done here"),
};

/// The digest's let go, wherever its button is: Now's Updates card today.
pub const LET_GO_DIGEST_HINT: HintCopy = HintCopy {
    id: "updates.let_go",
    anchor: "The first \"Let go of this digest\" button",
    text: "A lets go of this digest only; new mail arrives in the next one.",
    key: ("A", "let go of the digest"),
};

/// To do's keys, in the order help lists them.
const TODO_KEYS: &[(&str, &str)] = &[
    ("Enter", "do it"),
    ("e", "tick off"),
    ("Z", "schedule"),
    (",", "edit"),
    ("X", "not a to-do"),
    ("o", "open the email"),
    ("u", "undo"),
    ("t", "make a to-do from a conversation"),
    ("C", "catch up"),
    ("?", "what is this"),
];

pub const TODO_GUIDE: ModeGuideCopy = ModeGuideCopy {
    mode: "todo",
    name: "To do",
    header: todo_copy::HEADER,
    never_had_any: todo_copy::NEVER_HAD_ANY,
    add_one: todo_copy::ADD_ONE_KEYS,
    clear_for_now: todo_copy::CLEAR_FOR_NOW,
    lands_here: "Bills, failed payments, renewals, invites to answer, links to verify, forms to sign and promises you made.",
    about: todo_copy::ABOUT,
    why_template: "Here because: \"{evidence}\" ({source}).",
    keys: TODO_KEYS,
    first_run_line: todo_copy::FIRST_RUN_LINE,
    hints: &[
        HintCopy {
            id: "todo.runway",
            anchor: "The first runway bar",
            text: "The bar fills from when this showed up to when it's due; Enter does what the button says.",
            key: ("Enter", "do it"),
        },
        HintCopy {
            id: "todo.catchup",
            anchor: "The first catch-up line",
            text: "These came in before mxr sorted your mail. C goes through them: keep or let go of each.",
            key: ("C", "catch up"),
        },
    ],
};

/// Now's keys: acting on a row does it in that row's own mode.
const NOW_KEYS: &[(&str, &str)] = &[
    ("Enter", "open in its mode"),
    ("e", "done here"),
    ("t", "make it a to-do"),
    ("r", "reply"),
    ("o", "open the email"),
    ("A", "let go of the digest"),
    ("u", "undo"),
    ("?", "what is this"),
];

pub const NOW_GUIDE: ModeGuideCopy = ModeGuideCopy {
    mode: "now",
    name: "Now",
    header: now_copy::HEADER,
    never_had_any: now_copy::NEVER_HAD_ANY,
    add_one: "",
    clear_for_now: now_copy::CLEAR,
    lands_here: "People waiting on you, things due soon, the latest updates and, after 17:00, one thing to read.",
    about: now_copy::ABOUT,
    why_template: "From {mode}: {evidence}.",
    keys: NOW_KEYS,
    first_run_line: "",
    hints: &[
        HintCopy {
            id: "now.from_mode",
            anchor: "The first row's why line, \"From To do: ...\"",
            text: "Each row comes from a mode; Enter opens it there.",
            key: ("Enter", "open in its mode"),
        },
        DONE_HERE_HINT,
        LET_GO_DIGEST_HINT,
    ],
};

/// Messages' keys, in the order help lists them.
const MESSAGES_KEYS: &[(&str, &str)] = &[
    ("Enter", "open the person"),
    ("r", "reply"),
    ("a", "reply all"),
    (".", "got it"),
    ("e", "done here"),
    ("t", "make it a to-do"),
    ("b", "reply later"),
    ("s", "pin"),
    ("c", "new topic"),
    ("[", "previous topic"),
    ("]", "next topic"),
    ("p", "person page"),
    ("o", "show as sent"),
    ("u", "undo"),
    ("?", "what is this"),
];

pub const MESSAGES_GUIDE: ModeGuideCopy = ModeGuideCopy {
    mode: "messages",
    name: "Messages",
    header: messages_copy::HEADER,
    never_had_any: messages_copy::NEVER_HAD_ANY,
    add_one: "",
    clear_for_now: messages_copy::CLEAR,
    lands_here: messages_copy::LANDS_HERE,
    about: messages_copy::ABOUT,
    why_template: "Here because: {evidence} ({source}).",
    keys: MESSAGES_KEYS,
    first_run_line: messages_copy::FIRST_RUN_LINE,
    hints: &[
        HintCopy {
            id: "messages.topics",
            anchor: "The first topic list on a person's page",
            text: "Every conversation with this person, yours to answer first; ] and [ step through them.",
            key: ("]", "next topic"),
        },
        HintCopy {
            id: "messages.got_it",
            anchor: "Got it, the first time it has focus",
            text: "Got it (.) sends a short note that you've seen it and takes them off Your turn.",
            key: (".", "got it"),
        },
        DONE_HERE_HINT,
    ],
};

/// Archive's keys. Archive has no done: records stay, so `e` opens the
/// email the record came from, as `o` does.
const ARCHIVE_KEYS: &[(&str, &str)] = &[
    ("/", "ask"),
    ("y", "copy reference"),
    ("Y", "copy amount"),
    ("Enter", "open document"),
    ("o", "the email"),
    ("e", "the email"),
    ("p", "issuer page"),
    ("[", "previous year"),
    ("]", "next year"),
    (",", "fix a field"),
    ("v", "mark checked"),
    ("X", "not a record"),
    ("E", "export"),
    ("t", "make a to-do"),
    ("?", "what is this"),
];

pub const ARCHIVE_GUIDE: ModeGuideCopy = ModeGuideCopy {
    mode: "archive",
    name: "Archive",
    header: archive_copy::HEADER,
    never_had_any: archive_copy::NEVER_HAD_ANY,
    add_one: archive_copy::ADD_ONE_KEYS,
    clear_for_now: archive_copy::CLEAR_FOR_NOW,
    lands_here: archive_copy::LANDS_HERE,
    about: archive_copy::ABOUT,
    why_template: "Here because: {evidence} ({source}).",
    keys: ARCHIVE_KEYS,
    first_run_line: archive_copy::FIRST_RUN_LINE,
    hints: &[
        HintCopy {
            id: "archive.record",
            anchor: "The first record row",
            text:
                "Each row is one order, trip or bill, not an email; o opens the email it came from.",
            key: ("o", "the email"),
        },
        HintCopy {
            id: "archive.answer",
            anchor: "The first answer to a question",
            text: "y copies what this answer found; Enter opens the document.",
            key: ("y", "copy reference"),
        },
    ],
};

/// Every mode that has shipped, in rail order. Now leads: it is the front
/// page over the modes.
pub const MODE_GUIDES: &[ModeGuideCopy] = &[NOW_GUIDE, MESSAGES_GUIDE, TODO_GUIDE, ARCHIVE_GUIDE];

/// The guide for a mode id. Accepts "todo", "to-do" and "to do".
pub fn mode_guide(mode: &str) -> Option<&'static ModeGuideCopy> {
    let wanted: String = mode
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    MODE_GUIDES.iter().find(|guide| guide.mode == wanted)
}

/// A hint by id, wherever it attaches. Shared hints, like done here,
/// appear in more than one mode's table with one id and one seen state.
pub fn hint(id: &str) -> Option<&'static HintCopy> {
    MODE_GUIDES
        .iter()
        .flat_map(|guide| guide.hints)
        .find(|hint| hint.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANNED: &[&str] = &["AI", "smart", "Smart", "magic", "Magic", "powerful"];

    fn sentences(text: &str) -> usize {
        // A sentence ends at ". " or at the final full stop.
        text.matches(". ").count() + usize::from(text.trim_end().ends_with('.'))
    }

    fn plain(text: &str, mode: &str) {
        assert!(!text.contains('!'), "{mode}: {text}");
        assert!(!text.contains('\u{2014}'), "em dash in {text}");
        for word in BANNED {
            assert!(
                !text
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|w| w == *word),
                "{mode}: \"{word}\" in {text}"
            );
        }
    }

    #[test]
    fn every_guide_follows_the_copy_rules() {
        for guide in MODE_GUIDES {
            let words = guide.header.split_whitespace().count();
            assert!(words < 12, "{} header has {words} words", guide.mode);
            assert!(
                sentences(guide.about) <= 2,
                "{} about is more than two sentences",
                guide.mode
            );
            for text in [
                guide.header,
                guide.never_had_any,
                guide.add_one,
                guide.clear_for_now,
                guide.lands_here,
                guide.about,
                guide.why_template,
            ] {
                plain(text, guide.mode);
            }
        }
    }

    #[test]
    fn every_hint_is_short_names_its_key_and_the_key_does_that_in_the_mode() {
        for guide in MODE_GUIDES {
            assert!(!guide.hints.is_empty(), "{} has no hints", guide.mode);
            for hint in guide.hints {
                plain(hint.text, guide.mode);
                assert!(sentences(hint.text) <= 2, "{}: {}", hint.id, hint.text);
                assert!(hint.text.len() <= 100, "{} is too long", hint.id);
                let (key, verb) = hint.key;
                // A key is named as its own word: "(e)", "A lets", "] and [".
                let names_key = hint.text.split_whitespace().any(|word| {
                    word.trim_matches(|c: char| matches!(c, '(' | ')' | ',' | ';' | ':')) == key
                });
                assert!(names_key, "{}: the text never names {key}", hint.id);
                assert!(
                    guide.keys.contains(&(key, verb)),
                    "{}: {key} {verb} is not one of {}'s keys",
                    hint.id,
                    guide.mode
                );
            }
        }
    }

    #[test]
    fn a_hint_id_means_one_hint_wherever_it_appears() {
        for guide in MODE_GUIDES {
            for hint in guide.hints {
                assert_eq!(super::hint(hint.id), Some(hint), "{} differs", hint.id);
            }
        }
        assert!(super::hint("now.card").is_none());
    }

    #[test]
    fn mode_ids_resolve_loosely() {
        for id in ["todo", "to-do", "To do", " TODO "] {
            assert_eq!(mode_guide(id).map(|g| g.mode), Some("todo"), "{id}");
        }
        assert_eq!(mode_guide("Now").map(|g| g.mode), Some("now"));
        assert_eq!(mode_guide("messages").map(|g| g.mode), Some("messages"));
        assert_eq!(mode_guide("Archive").map(|g| g.mode), Some("archive"));
        assert!(mode_guide("updates").is_none());
    }

    #[test]
    fn seen_state_rides_along_per_hint() {
        let unseen = TODO_GUIDE.to_data(|_| None);
        assert!(unseen.hints.iter().all(|hint| !hint.seen));
        let at = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(20_000);
        let seen = TODO_GUIDE.to_data(|id| (id == "todo.runway").then_some(at));
        let runway = seen.hints.iter().find(|h| h.id == "todo.runway").unwrap();
        assert!(runway.seen);
        assert_eq!(runway.seen_at, Some(at));
        let catchup = seen.hints.iter().find(|h| h.id == "todo.catchup").unwrap();
        assert!(!catchup.seen);
    }
}
