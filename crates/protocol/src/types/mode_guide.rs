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

use super::{archive_copy, messages_copy, now_copy, reading_copy, todo_copy, updates_copy};

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
    /// `{key}` stands for this hint's key and `{key:<action id>}` for
    /// another's, filled in from the key definition when served.
    pub text: &'static str,
    /// The key definition's action id, and the verb it does here.
    pub key: (&'static str, &'static str),
}

impl HintCopy {
    /// `key_of` gives the key for a key-definition action id
    /// (`mxr_keymap::guide_key`); the protocol crate can't read the
    /// definition itself.
    pub fn to_data(
        &self,
        seen_at: Option<chrono::DateTime<chrono::Utc>>,
        key_of: &dyn Fn(&str) -> Option<String>,
    ) -> HintData {
        let key = key_of(self.key.0).unwrap_or_default();
        HintData {
            id: self.id.to_string(),
            anchor: self.anchor.to_string(),
            text: fill_keys(self.text, &key, key_of),
            key: ModeKeyData {
                key,
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
    /// The key definition's action ids, with the verb each does here, in
    /// the order help lists them.
    pub keys: &'static [(&'static str, &'static str)],
    pub first_run_line: &'static str,
    pub hints: &'static [HintCopy],
}

impl ModeGuideCopy {
    /// `seen_at` answers when a hint id was dismissed, or `None`.
    /// `key_of` gives the key for a key-definition action id
    /// (`mxr_keymap::guide_key`). An action with no key is left out.
    pub fn to_data(
        &self,
        seen_at: impl Fn(&str) -> Option<chrono::DateTime<chrono::Utc>>,
        key_of: &dyn Fn(&str) -> Option<String>,
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
                .filter_map(|(id, verb)| {
                    Some(ModeKeyData {
                        key: key_of(id)?,
                        verb: (*verb).to_string(),
                    })
                })
                .collect(),
            first_run_line: self.first_run_line.to_string(),
            hints: self
                .hints
                .iter()
                .map(|hint| hint.to_data(seen_at(hint.id), key_of))
                .collect(),
        }
    }
}

/// A hint's text with `{key}` replaced by its key and `{key:<id>}` by that
/// action's key.
fn fill_keys(text: &str, own: &str, key_of: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{key") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        let inner = &rest[start + 1..start + end];
        match inner.strip_prefix("key:") {
            Some(id) => out.push_str(&key_of(id).unwrap_or_default()),
            None => out.push_str(own),
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Done here on a conversation, in the toast after the first `e`: the
/// moment a Gmail habit (archive means gone) meets the mode rule.
/// Each mode passes its own done action (`now.done`, `messages.done`), so
/// the hint names the key that does it there; the id, and so the seen
/// state, is the same in every mode.
pub const fn done_here_hint(action: &'static str) -> HintCopy {
    HintCopy {
        id: "done_here",
        anchor: "The toast after the first e (done here) on a conversation",
        text: "Done here ({key}) only clears this mode; it stays in To do until done there.",
        key: (action, "done here"),
    }
}

/// The digest's let go, wherever its button is: Now's Updates card today.
/// Each mode passes its own let-go action (`now.let-go-digest`,
/// `updates.let-go-all`); the id is shared, as for the done-here hint.
pub const fn let_go_digest_hint(action: &'static str) -> HintCopy {
    HintCopy {
        id: "updates.let_go",
        anchor: "The first \"Let go of this digest\" button",
        text: "{key} lets go of this digest only; new mail arrives in the next one.",
        key: (action, "let go of digest"),
    }
}

/// To do's keys, in the order help lists them.
const TODO_KEYS: &[(&str, &str)] = &[
    ("todo.primary", "do it"),
    ("todo.done", "tick off"),
    ("todo.schedule", "schedule"),
    ("todo.edit", "edit"),
    ("todo.dismiss", "correct to-do"),
    ("todo.open-email", "open the email"),
    ("mail.undo", "undo"),
    ("todo.catchup", "catch up"),
    ("shell.help", "what is this"),
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
            anchor: "The first row",
            text: "Press {key} to tick this off, or {key:todo.open-email} to open the email it came from.",
            key: ("todo.done", "tick off"),
        },
        HintCopy {
            id: "todo.catchup",
            anchor: "The first catch-up line",
            text: "These came in before mxr sorted your mail. {key} goes through them: keep or let go of each.",
            key: ("todo.catchup", "catch up"),
        },
    ],
};

/// Now's keys: acting on a row does it in that row's own mode.
const NOW_KEYS: &[(&str, &str)] = &[
    ("now.open", "open in its mode"),
    ("now.done", "done here"),
    ("now.to-do", "make it a to-do"),
    ("now.reply", "reply"),
    ("now.open-email", "open the email"),
    ("now.let-go-digest", "let go of digest"),
    ("mail.undo", "undo"),
    ("shell.help", "what is this"),
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
    why_template: "Here because: {evidence}.",
    keys: NOW_KEYS,
    first_run_line: "",
    hints: &[
        HintCopy {
            id: "now.from_mode",
            anchor: "The first row, and its reason in the status bar",
            text: "Each row comes from a mode; {key} opens it there.",
            key: ("now.open", "open in its mode"),
        },
        done_here_hint("now.done"),
        let_go_digest_hint("now.let-go-digest"),
    ],
};

/// Messages' keys, in the order help lists them.
const MESSAGES_KEYS: &[(&str, &str)] = &[
    ("messages.cycle-filter", "cycle views"),
    ("messages.open", "open the person"),
    ("messages.reply", "reply"),
    ("messages.reply-all", "reply all"),
    ("messages.got-it", "got it"),
    ("messages.done", "done here"),
    ("messages.to-do", "make it a to-do"),
    ("messages.reply-later", "not now"),
    ("messages.pin", "pin"),
    ("messages.new-topic", "new topic"),
    ("messages.prev-topic", "previous topic"),
    ("messages.next-topic", "next topic"),
    ("messages.person-page", "person page"),
    ("messages.as-sent", "show as sent"),
    ("mail.undo", "undo"),
    ("shell.help", "what is this"),
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
            text: "Every conversation with this person, yours to answer first; {key} and {key:messages.prev-topic} step through them.",
            key: ("messages.next-topic", "next topic"),
        },
        HintCopy {
            id: "messages.got_it",
            anchor: "Got it, the first time it has focus",
            text: "Got it ({key}) sends a short note that you've seen it and takes them off Your turn.",
            key: ("messages.got-it", "got it"),
        },
        done_here_hint("messages.done"),
    ],
};

/// Archive's keys. Archive has no done: records stay, so `e` opens the
/// email the record came from, as `o` does.
const ARCHIVE_KEYS: &[(&str, &str)] = &[
    ("archive.ask", "ask"),
    ("archive.copy-reference", "copy reference"),
    ("archive.copy-amount", "copy amount"),
    ("archive.open-document", "open document"),
    ("archive.open-email", "the email"),
    ("archive.issuer", "issuer page"),
    ("archive.prev-year", "previous year"),
    ("archive.next-year", "next year"),
    ("archive.edit", "fix a field or mark it checked"),
    ("archive.dismiss", "not a record"),
    ("archive.make-todo", "make a to-do"),
    ("shell.help", "what is this"),
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
                "Each row is one order, trip or bill, not an email; {key} opens the email it came from.",
            key: ("archive.open-email", "the email"),
        },
        HintCopy {
            id: "archive.answer",
            anchor: "The first answer to a question",
            text: "{key} copies what this answer found; {key:archive.open-document} opens the document.",
            key: ("archive.copy-reference", "copy reference"),
        },
    ],
};

/// Updates' keys: the digest is let go in one key, a source in another.
const UPDATES_KEYS: &[(&str, &str)] = &[
    ("updates.let-go-all", "let go of digest"),
    ("updates.let-go-source", "let go of this source"),
    ("updates.pin", "pin latest email"),
    ("updates.needs-me", "this needs me"),
    ("updates.tune", "tune a source"),
    ("updates.link", "open the link"),
    ("updates.copy-code", "copy the code"),
    ("updates.open-email", "open the email"),
    ("mail.undo", "undo"),
    ("shell.help", "what is this"),
];

pub const UPDATES_GUIDE: ModeGuideCopy = ModeGuideCopy {
    mode: "updates",
    name: "Updates",
    header: updates_copy::HEADER,
    never_had_any: updates_copy::NEVER_HAD_ANY,
    add_one: "",
    clear_for_now: updates_copy::CLEAR_FOR_NOW,
    lands_here: updates_copy::LANDS_HERE,
    about: updates_copy::ABOUT,
    why_template: updates_copy::WHY,
    keys: UPDATES_KEYS,
    first_run_line: updates_copy::FIRST_RUN_LINE,
    hints: &[
        HintCopy {
            id: "updates.suggestion",
            anchor: "The first suggested to-do in Needs a look",
            text: "Nothing becomes a to-do on its own; {key} adds this one to To do.",
            key: ("updates.needs-me", "this needs me"),
        },
        HintCopy {
            id: "updates.source",
            anchor: "The first source line",
            text: "{key} lets go of this source only; the rest of the digest stays.",
            key: ("updates.let-go-source", "let go of this source"),
        },
        let_go_digest_hint("updates.let-go-all"),
    ],
};

/// Reading's keys, per the blueprint's one key map.
const READING_KEYS: &[(&str, &str)] = &[
    ("reading.read", "read"),
    ("reading.later", "later"),
    ("reading.let-go", "let go"),
    ("reading.article", "fetch the linked article"),
    ("reading.unsubscribe", "unsubscribe"),
    ("reading.original", "original layout"),
    ("reading.let-go-all", "let go of everything shown"),
    ("reading.reader-highlight", "highlight, while reading"),
    ("reading.open-email", "open the email"),
    ("mail.undo", "undo"),
    ("shell.help", "what is this"),
];

pub const READING_GUIDE: ModeGuideCopy = ModeGuideCopy {
    mode: "reading",
    name: "Reading",
    header: reading_copy::HEADER,
    never_had_any: reading_copy::NEVER_HAD_ANY,
    add_one: "",
    clear_for_now: reading_copy::CLEAR_FOR_NOW,
    lands_here: reading_copy::LANDS_HERE,
    about: reading_copy::ABOUT,
    why_template: "Here because: {evidence} ({source}). {fades}",
    keys: READING_KEYS,
    first_run_line: reading_copy::FIRST_RUN_LINE,
    hints: &[READING_FADING_HINT, READING_LINK_HINT],
};

/// On the first Fading band: what fading means and the one key that stops it.
pub const READING_FADING_HINT: HintCopy = HintCopy {
    id: "reading.fading",
    anchor: "The first Fading band",
    text: "These go within a day; {key} keeps one on Later, which never fades.",
    key: ("reading.later", "later"),
};

/// On the first digest link: a link is its own item, and its article is
/// fetched only when asked.
pub const READING_LINK_HINT: HintCopy = HintCopy {
    id: "reading.link",
    anchor: "The first link under a digest",
    text: "Each link is its own item; {key} fetches its article, and only then does mxr contact that site.",
    key: ("reading.article", "fetch the linked article"),
};

/// Every mode that has shipped, in rail order. Now leads: it is the front
/// page over the modes.
pub const MODE_GUIDES: &[ModeGuideCopy] = &[
    NOW_GUIDE,
    MESSAGES_GUIDE,
    TODO_GUIDE,
    UPDATES_GUIDE,
    READING_GUIDE,
    ARCHIVE_GUIDE,
];

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
                // Keys are filled in when served; measure with one-letter keys.
                let shown = fill_keys(hint.text, "k", &|_| Some("k".into()));
                plain(&shown, guide.mode);
                assert!(sentences(&shown) <= 2, "{}: {}", hint.id, shown);
                assert!(shown.len() <= 100, "{} is too long", hint.id);
                let (id, verb) = hint.key;
                assert!(
                    hint.text.contains("{key}"),
                    "{}: the text never names its key",
                    hint.id
                );
                assert!(
                    guide.keys.contains(&(id, verb)),
                    "{}: {id} {verb} is not one of {}'s keys",
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
                // The action id is each mode's own; the copy is shared.
                let same =
                    super::hint(hint.id).map(|first| (first.anchor, first.text, first.key.1));
                assert_eq!(
                    same,
                    Some((hint.anchor, hint.text, hint.key.1)),
                    "{} differs",
                    hint.id
                );
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
        assert_eq!(mode_guide("Updates").map(|g| g.mode), Some("updates"));
        assert_eq!(mode_guide("Reading").map(|g| g.mode), Some("reading"));
        assert!(mode_guide("paper trail").is_none());
    }

    #[test]
    fn seen_state_rides_along_per_hint() {
        let unseen = TODO_GUIDE.to_data(|_| None, &|id: &str| Some(id.to_string()));
        assert!(unseen.hints.iter().all(|hint| !hint.seen));
        let at = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(20_000);
        let seen = TODO_GUIDE.to_data(|id| (id == "todo.runway").then_some(at), &|id: &str| {
            Some(id.to_string())
        });
        let runway = seen
            .hints
            .iter()
            .find(|h| h.id == "todo.runway")
            .expect("runway hint");
        assert!(runway.seen);
        assert_eq!(runway.seen_at, Some(at));
        let catchup = seen
            .hints
            .iter()
            .find(|h| h.id == "todo.catchup")
            .expect("catch-up hint");
        assert!(!catchup.seen);
    }
}
