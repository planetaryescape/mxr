//! A person's remaps for the modes: the `[now]`, `[todo]` and other mode
//! sections of `keys.toml` (blueprint chapter 35, D170). Keys are spelled
//! as `mxr keys` prints them and name an action id from the definition;
//! an empty string unbinds the key.

use crate::{is_well_formed, Keymap};
use std::collections::BTreeMap;

/// The `keys.toml` sections that remap a mode's keys.
pub const MODE_SECTIONS: [&str; 8] = [
    "now", "messages", "todo", "catchup", "expired", "updates", "reading", "archive",
];

/// Keys a terminal cannot tell apart from another key, so a binding on
/// them never fires.
const TERMINAL_ALIASES: [(&str, &str); 4] = [
    ("Ctrl+i", "Tab"),
    ("Ctrl+m", "Enter"),
    ("Ctrl+h", "Backspace"),
    ("Ctrl+[", "Escape"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModeRemaps {
    /// context → key → action id, or `None` where the key is unbound.
    by_context: BTreeMap<String, BTreeMap<String, Option<String>>>,
}

impl ModeRemaps {
    /// The mode sections of a parsed `keys.toml`, and what in them can't be
    /// used. Unusable entries are left out; other sections are ignored.
    pub fn from_table(table: &toml::Table, keymap: &Keymap) -> (Self, Vec<String>) {
        let mut remaps = Self::default();
        let mut problems = Vec::new();
        for section in MODE_SECTIONS {
            let Some(value) = table.get(section) else {
                continue;
            };
            let Some(entries) = value.as_table() else {
                problems.push(format!("[{section}] must be a table of key = \"action\""));
                continue;
            };
            let mut keys = BTreeMap::new();
            for (key, action) in entries {
                let Some(action) = action.as_str() else {
                    problems.push(format!("[{section}] `{key}`: the action must be a string"));
                    continue;
                };
                if !is_well_formed(key) {
                    problems.push(format!(
                        "[{section}] `{key}`: not a key; spell it as `mxr keys` prints it, e.g. `x`, `g x`, `Ctrl+d`"
                    ));
                    continue;
                }
                if let Some((_, same)) = TERMINAL_ALIASES.iter().find(|(alias, _)| alias == key) {
                    problems.push(format!(
                        "[{section}] `{key}`: terminals send this as {same}, so it can't be bound on its own"
                    ));
                    continue;
                }
                if key.contains("Ctrl+") && key.contains("Shift+") {
                    problems.push(format!(
                        "[{section}] `{key}`: most terminals send Ctrl+Shift+ as plain Ctrl+"
                    ));
                    continue;
                }
                if action.is_empty() {
                    keys.insert(key.clone(), None);
                    continue;
                }
                match keymap.get(action) {
                    Some(defined) if !defined.tui.is_empty() => {
                        keys.insert(key.clone(), Some(action.to_string()));
                    }
                    Some(_) => problems.push(format!(
                        "[{section}] `{key}`: {action} is not a TUI action"
                    )),
                    None => problems.push(format!(
                        "[{section}] `{key}`: no action {action}; `mxr keys --client tui --context {section}` lists them"
                    )),
                }
            }
            for key in keys.keys() {
                let start = format!("{key} ");
                if let Some(longer) = keys.keys().find(|other| other.starts_with(&start)) {
                    problems.push(format!(
                        "[{section}] `{key}` is also the start of `{longer}`, so `{longer}` can never complete"
                    ));
                }
                if keys.get(key).is_some_and(Option::is_some)
                    && keymap.tui_starts_sequence(section, key)
                {
                    problems.push(format!(
                        "[{section}] `{key}` starts default sequences such as `{key} …`; binding it alone hides them"
                    ));
                }
            }
            if !keys.is_empty() {
                remaps.by_context.insert(section.to_string(), keys);
            }
        }
        for name in table.keys() {
            if !MODE_SECTIONS.contains(&name.as_str())
                && !["mail_list", "message_view", "thread_view"].contains(&name.as_str())
            {
                problems.push(format!(
                    "[{name}] is not a keys.toml section; use mail_list, message_view, thread_view or a mode: {}",
                    MODE_SECTIONS.join(", ")
                ));
            }
        }
        (remaps, problems)
    }

    pub fn is_empty(&self) -> bool {
        self.by_context.is_empty()
    }

    /// The person's binding for `keys` in `context`: `Some(None)` when they
    /// unbound it, `None` when they left it alone.
    pub fn lookup(&self, context: &str, keys: &str) -> Option<Option<&str>> {
        self.by_context
            .get(context)?
            .get(keys)
            .map(Option::as_deref)
    }

    /// The key the person gave this action in `context`, if any.
    pub fn key_for(&self, context: &str, action_id: &str) -> Option<&str> {
        self.by_context
            .get(context)?
            .iter()
            .find_map(|(key, action)| {
                (action.as_deref() == Some(action_id)).then_some(key.as_str())
            })
    }

    /// True when the person moved this action to another key here, so its
    /// default keys no longer trigger it.
    pub fn retires(&self, context: &str, action_id: &str) -> bool {
        self.key_for(context, action_id).is_some()
    }

    /// True when `prefix` starts a sequence the person bound in `context`.
    pub fn starts_sequence(&self, context: &str, prefix: &str) -> bool {
        let start = format!("{prefix} ");
        self.by_context
            .get(context)
            .is_some_and(|keys| keys.keys().any(|key| key.starts_with(&start)))
    }
}
