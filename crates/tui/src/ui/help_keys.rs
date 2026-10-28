//! Help's key tables, read from the key definition
//! (`crates/keymap/keymap.toml`, blueprint 35) for the place help is
//! opened in, with the person's `keys.toml` remaps. Nothing here names a
//! key: rows, titles and labels all come from the definition.

use crate::action::UiContext;
use crate::keybindings::{parse_key_string, KeybindingConfig, ViewContext};
use crate::keymap_names::action_named;
use crate::ui::command_palette::shown_key;
use mxr_keymap::{KeyAction, TuiBinding, MODE_SECTIONS};

/// The places whose keys help lists for a screen, when the app didn't
/// name one: Analytics lists every view's keys under its own.
pub(crate) fn places_for(context: UiContext) -> Vec<&'static str> {
    match context {
        UiContext::MailboxSidebar => vec!["sidebar"],
        UiContext::MailboxList => vec!["list"],
        UiContext::MailboxMessage => vec!["reader"],
        UiContext::SearchEditor => vec!["search-editor"],
        UiContext::SearchResults => vec!["search"],
        UiContext::SearchPreview => vec!["search-preview"],
        UiContext::RulesList => vec!["rules"],
        UiContext::RulesForm => vec!["rules-form"],
        UiContext::Diagnostics => vec!["diagnostics"],
        UiContext::AccountsList => vec!["accounts"],
        UiContext::AccountsForm => vec!["accounts-form"],
        UiContext::Analytics => vec![
            "analytics",
            "analytics-storage",
            "analytics-stale",
            "analytics-contacts",
            "analytics-response-time",
            "analytics-subscriptions",
            "analytics-wrapped",
        ],
        UiContext::Deliveries => vec!["deliveries"],
    }
}

/// Places help lists wherever it is opened: the assistant pane takes the
/// keys while it is open, so help can't be opened from it, and the writing
/// proof is an overlay. Help lists every overlay after these, the
/// assistant's proof, question and history among them.
pub(crate) const ALWAYS_LISTED: [&str; 3] =
    ["assistant", "assistant-input", "compose-assist-proof"];

/// The keys that run the definition's actions, as the person has them.
pub(crate) struct KeyRows<'a> {
    pub config: &'a KeybindingConfig,
    /// The list or reader map that governs `keys.toml` remaps here.
    pub view: ViewContext,
}

impl KeyRows<'_> {
    /// One row per action with keys in `place`, in the definition's order:
    /// every key that runs it there, `" / "` between them, and its label.
    pub(crate) fn rows(&self, place: &str) -> Vec<(String, String)> {
        mxr_keymap::defaults()
            .actions
            .iter()
            .filter_map(|action| {
                let keys = self.keys(action, place);
                (!keys.is_empty()).then(|| (keys.join(" / "), action.label.clone()))
            })
            .collect()
    }

    /// The first key that runs action `id` in `place`, if any.
    pub(crate) fn key(&self, id: &str, place: &str) -> Option<String> {
        let action = mxr_keymap::defaults().get(id)?;
        self.keys(action, place).into_iter().next()
    }

    /// Every key that runs `action` in `place`, as help spells keys.
    pub(crate) fn keys(&self, action: &KeyAction, place: &str) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for binding in action.tui.iter().filter(|b| b.contexts.includes(place)) {
            for key in self.binding_keys(action, binding, place) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        keys
    }

    fn binding_keys(&self, action: &KeyAction, binding: &TuiBinding, place: &str) -> Vec<String> {
        if MODE_SECTIONS.contains(&place) {
            let remaps = &self.config.modes;
            let remapped = remaps.keys_for(place, &action.id);
            if !remapped.is_empty() {
                return remapped.into_iter().map(shown_key).collect();
            }
            return binding
                .keys
                .iter()
                .filter(|key| remaps.lookup(place, key).is_none())
                .map(|key| shown_key(key))
                .collect();
        }
        // The list and reader maps also govern Search and the sidebar.
        let mapped = matches!(
            place,
            "list" | "reader" | "sidebar" | "place" | "search" | "search-preview"
        );
        if !mapped {
            return binding.keys.iter().map(|key| shown_key(key)).collect();
        }
        if let Some(tui_action) = action_named(&binding.name) {
            if self.config.has_action_override(self.view, &tui_action) {
                return self.config.key_displays(self.view, &tui_action);
            }
        }
        // A default the person gave to another action no longer runs this.
        binding
            .keys
            .iter()
            .map(|key| shown_key(key))
            .filter(|key| {
                parse_key_string(key).map_or(true, |parsed| {
                    !self.config.retired_sequence(self.view, &parsed.keys)
                })
            })
            .collect()
    }
}
