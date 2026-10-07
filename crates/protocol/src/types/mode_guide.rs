//! How each mode explains itself in place (D118): the header line, both
//! empty states, the first-encounter card, the why template and the keys
//! with their verbs. One table, served by `Request::GetModeGuide`, so the
//! web app, the TUI, the CLI and agents say the same words.
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

/// Returned in `ResponseData::ModeGuides`: one mode's teaching copy and
/// whether its first-encounter card has been retired on this profile.
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
    /// The first-encounter card: one or two sentences.
    pub card: String,
    /// The card's line of keys.
    pub card_keys: Vec<ModeKeyData>,
    /// How every row's why line reads: `{evidence}` and `{source}` are
    /// filled per row.
    pub why_template: String,
    /// Every key the mode answers to, with its verb, for `?` and the footer.
    pub keys: Vec<ModeKeyData>,
    /// The first-run summary's line for this mode.
    pub first_run_line: String,
    /// The card was closed, or retired by using the mode's main verb.
    pub card_seen: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_seen_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One mode's copy, as compiled in. `ModeGuideData` adds the seen state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeGuideCopy {
    pub mode: &'static str,
    pub name: &'static str,
    pub header: &'static str,
    pub never_had_any: &'static str,
    pub add_one: &'static str,
    pub clear_for_now: &'static str,
    pub lands_here: &'static str,
    pub card: &'static str,
    pub card_keys: &'static [(&'static str, &'static str)],
    pub why_template: &'static str,
    pub keys: &'static [(&'static str, &'static str)],
    pub first_run_line: &'static str,
}

impl ModeGuideCopy {
    pub fn to_data(&self, card_seen_at: Option<chrono::DateTime<chrono::Utc>>) -> ModeGuideData {
        let keys = |keys: &[(&str, &str)]| {
            keys.iter()
                .map(|(key, verb)| ModeKeyData {
                    key: (*key).to_string(),
                    verb: (*verb).to_string(),
                })
                .collect()
        };
        ModeGuideData {
            mode: self.mode.to_string(),
            name: self.name.to_string(),
            header: self.header.to_string(),
            never_had_any: self.never_had_any.to_string(),
            add_one: self.add_one.to_string(),
            clear_for_now: self.clear_for_now.to_string(),
            lands_here: self.lands_here.to_string(),
            card: self.card.to_string(),
            card_keys: keys(self.card_keys),
            why_template: self.why_template.to_string(),
            keys: keys(self.keys),
            first_run_line: self.first_run_line.to_string(),
            card_seen: card_seen_at.is_some(),
            card_seen_at,
        }
    }
}

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
    card: todo_copy::CARD,
    card_keys: &[
        ("Enter", "do it"),
        ("e", "tick off"),
        ("Z", "schedule"),
        ("X", "not a to-do"),
    ],
    why_template: "Here because: \"{evidence}\" ({source}).",
    keys: TODO_KEYS,
    first_run_line: todo_copy::FIRST_RUN_LINE,
};

/// Now's keys: acting on a row does it in that row's own mode.
const NOW_KEYS: &[(&str, &str)] = &[
    ("Enter", "open in its mode"),
    ("e", "done here"),
    ("t", "make it a to-do"),
    ("r", "reply"),
    ("o", "open the email"),
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
    card: now_copy::CARD,
    card_keys: &[
        ("Enter", "open in its mode"),
        ("e", "done here"),
        ("?", "what is this"),
    ],
    why_template: "From {mode}: {evidence}.",
    keys: NOW_KEYS,
    first_run_line: "",
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
    card: messages_copy::CARD,
    card_keys: &[
        ("r", "reply"),
        (".", "got it"),
        ("e", "done here"),
        ("t", "make it a to-do"),
    ],
    why_template: "Here because: {evidence} ({source}).",
    keys: MESSAGES_KEYS,
    first_run_line: messages_copy::FIRST_RUN_LINE,
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
    ("F", "file an email"),
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
    card: archive_copy::CARD,
    card_keys: &[
        ("/", "ask"),
        ("y", "copy reference"),
        ("Enter", "open document"),
        ("o", "the email"),
    ],
    why_template: "Here because: {evidence} ({source}).",
    keys: ARCHIVE_KEYS,
    first_run_line: archive_copy::FIRST_RUN_LINE,
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

#[cfg(test)]
mod tests {
    use super::*;

    const BANNED: &[&str] = &["AI", "smart", "Smart", "magic", "Magic", "powerful"];

    fn sentences(text: &str) -> usize {
        // A sentence ends at ". " or at the final full stop.
        text.matches(". ").count() + usize::from(text.trim_end().ends_with('.'))
    }

    #[test]
    fn every_guide_follows_the_copy_rules() {
        for guide in MODE_GUIDES {
            let words = guide.header.split_whitespace().count();
            assert!(words < 12, "{} header has {words} words", guide.mode);
            assert!(
                sentences(guide.card) <= 2,
                "{} card is more than two sentences",
                guide.mode
            );
            for text in [
                guide.header,
                guide.never_had_any,
                guide.add_one,
                guide.clear_for_now,
                guide.lands_here,
                guide.card,
                guide.why_template,
            ] {
                assert!(!text.contains('!'), "{}: {text}", guide.mode);
                assert!(!text.contains('\u{2014}'), "em dash in {text}");
                for word in BANNED {
                    assert!(
                        !text
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|w| w == *word),
                        "{}: \"{word}\" in {text}",
                        guide.mode
                    );
                }
            }
        }
    }

    #[test]
    fn every_card_key_does_the_same_verb_in_the_mode() {
        for guide in MODE_GUIDES {
            assert!(!guide.card_keys.is_empty());
            for (key, verb) in guide.card_keys {
                assert!(
                    guide.keys.contains(&(*key, *verb)),
                    "{}: card key {key} {verb} is not in the mode's keys",
                    guide.mode
                );
            }
        }
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
    fn seen_state_rides_along() {
        let unseen = TODO_GUIDE.to_data(None);
        assert!(!unseen.card_seen);
        let at = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(20_000);
        let seen = TODO_GUIDE.to_data(Some(at));
        assert!(seen.card_seen);
        assert_eq!(seen.card_seen_at, Some(at));
    }
}
