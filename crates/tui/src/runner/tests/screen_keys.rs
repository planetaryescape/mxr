//! The tool screens, Search, the calendar-invites lens and the assistant
//! pane read keys in their own handlers. Their keys are listed in the key
//! definition so help and the keybindings page can show them; this probe
//! holds each list to what its handler does. A key the screen leaves to the
//! shared input handler needs no entry.

use super::*;
use crate::action::{action_allowed_in_context, UiContext};
use crate::app::{AccountFormMode, AnalyticsView, AssistantFocus, RulesPanel, StorageMode};
use std::collections::BTreeMap;

/// What a key a screen doesn't list does there.
#[derive(Clone, Copy)]
enum Unlisted {
    /// The shared input handler's action, where the screen allows it.
    Shared,
    /// Nothing: a text field or the assistant pane owns every key.
    Nothing,
}

/// A screen context, the state that shows it, the place whose moved keys
/// it announces, and what its unlisted keys do.
type ScreenProbe = (&'static str, fn(&mut App), &'static str, Unlisted);

const SCREENS: &[ScreenProbe] = &[
    ("rules", rules, "rules", Unlisted::Shared),
    ("rules-form", rules_form, "rules", Unlisted::Nothing),
    ("accounts", accounts, "accounts", Unlisted::Shared),
    (
        "accounts-form",
        accounts_form,
        "accounts",
        Unlisted::Nothing,
    ),
    ("diagnostics", diagnostics, "diagnostics", Unlisted::Shared),
    (
        "analytics",
        |app| analytics(app, AnalyticsView::CadenceDrift),
        "analytics",
        Unlisted::Shared,
    ),
    (
        "analytics-storage",
        |app| analytics(app, AnalyticsView::Storage),
        "analytics",
        Unlisted::Shared,
    ),
    (
        "analytics-stale",
        |app| analytics(app, AnalyticsView::StaleThreads),
        "analytics",
        Unlisted::Shared,
    ),
    (
        "analytics-contacts",
        |app| analytics(app, AnalyticsView::Contacts),
        "analytics",
        Unlisted::Shared,
    ),
    (
        "analytics-response-time",
        |app| analytics(app, AnalyticsView::ResponseTime),
        "analytics",
        Unlisted::Shared,
    ),
    (
        "analytics-subscriptions",
        |app| analytics(app, AnalyticsView::Subscriptions),
        "analytics",
        Unlisted::Shared,
    ),
    (
        "analytics-wrapped",
        |app| analytics(app, AnalyticsView::Wrapped),
        "analytics",
        Unlisted::Shared,
    ),
    ("deliveries", deliveries, "deliveries", Unlisted::Shared),
    (
        "search",
        |app| search(app, SearchPane::Results),
        "search",
        Unlisted::Shared,
    ),
    (
        "search-preview",
        |app| search(app, SearchPane::Preview),
        "reader",
        Unlisted::Shared,
    ),
    ("search-editor", search_editor, "search", Unlisted::Nothing),
    ("invites", invites, "invites", Unlisted::Shared),
    (
        "assistant",
        |app| assistant(app, AssistantFocus::Pane),
        "assistant",
        Unlisted::Nothing,
    ),
    (
        "assistant-input",
        |app| assistant(app, AssistantFocus::Input),
        "assistant",
        Unlisted::Nothing,
    ),
];

fn rules(app: &mut App) {
    app.screen = Screen::Rules;
    // The dry-run panel pages with PageUp and PageDown.
    app.rules.page.panel = RulesPanel::DryRun;
}

fn rules_form(app: &mut App) {
    app.screen = Screen::Rules;
    app.rules.page.form.visible = true;
}

fn accounts(app: &mut App) {
    app.screen = Screen::Accounts;
    app.accounts.page.onboarding_required = false;
}

fn accounts_form(app: &mut App) {
    accounts(app);
    app.accounts.page.form.visible = true;
    // Gmail's form adds `r` to sign in again.
    app.accounts.page.form.mode = AccountFormMode::Gmail;
}

fn diagnostics(app: &mut App) {
    app.screen = Screen::Diagnostics;
}

fn analytics(app: &mut App, view: AnalyticsView) {
    app.screen = Screen::Analytics;
    app.analytics.view = view;
    // Storage groups its breakdown with `b`.
    app.analytics.storage_mode = StorageMode::Breakdown;
}

fn deliveries(app: &mut App) {
    app.screen = Screen::Deliveries;
    // The split preview adds Esc and the scroll keys.
    app.deliveries.preview_active = true;
}

fn search(app: &mut App, pane: SearchPane) {
    app.screen = Screen::Search;
    app.search.page.editing = false;
    app.search.page.active_pane = pane;
}

fn search_editor(app: &mut App) {
    app.screen = Screen::Search;
    app.search.page.editing = true;
}

fn invites(app: &mut App) {
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = MailboxView::CalendarInvites;
    app.mailbox.active_pane = ActivePane::MailList;
}

fn assistant(app: &mut App, focus: AssistantFocus) {
    app.screen = Screen::Mailbox;
    app.assistant.open = true;
    app.assistant.focus = focus;
}

/// The definition's places for a screen: an analytics view has the
/// analytics keys too, except Wrapped, which lists its own.
fn places(screen: &str) -> Vec<&str> {
    match screen {
        "analytics-wrapped" => vec![screen],
        view if view.starts_with("analytics-") => vec!["analytics", view],
        _ => vec![screen],
    }
}

/// Key → definition name for a screen; a view's own key wins.
fn defined(screen: &str) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    for place in places(screen) {
        for binding in mxr_keymap::defaults().actions.iter().flat_map(|a| &a.tui) {
            if binding.contexts.includes(place) {
                for key in &binding.keys {
                    keys.insert(key.clone(), binding.name.clone());
                }
            }
        }
    }
    keys
}

/// The probe's keys plus the ones only screens read.
pub(super) fn screen_probe_keys() -> Vec<(KeyEvent, String)> {
    let mut keys = super::keymap::probe_keys();
    keys.push((
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        "Shift+Tab".into(),
    ));
    keys.push((
        KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
        "Alt+Enter".into(),
    ));
    keys
}

#[test]
fn screen_keys_are_what_the_definition_says() {
    let keymap = mxr_keymap::defaults();
    let keys = screen_probe_keys();
    let mut wrong = Vec::new();
    for (screen, setup, moved_place, unlisted) in SCREENS {
        let defined = defined(screen);
        for key in defined.keys() {
            if !keys.iter().any(|(_, name)| name == key) {
                wrong.push(format!("{screen} `{key}`: not a key the probe presses"));
            }
        }
        let press = |event: KeyEvent| {
            let mut app = App::new();
            setup(&mut app);
            app.handle_key(event)
        };
        let context: UiContext = {
            let mut app = App::new();
            setup(&mut app);
            app.current_ui_context()
        };
        for (event, name) in &keys {
            let got = press(*event);
            let unlisted_does = match unlisted {
                Unlisted::Shared => InputHandler::new().handle_key(*event),
                Unlisted::Nothing => None,
            }
            .filter(|action| action_allowed_in_context(action, context));
            // Handlers that ignore Ctrl or Alt make `Ctrl+o` an echo of `o`,
            // and `q` below the root does what `Esc` does there.
            let echo = (event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && got == press(KeyEvent::new(event.code, KeyModifiers::NONE)))
                || (name == "q" && got == press(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
            match defined.get(name) {
                Some(inline) if inline.starts_with("inline:") => {
                    if let Some(action) = got {
                        wrong.push(format!(
                            "{screen} `{name}`: listed as {inline} but runs {action:?}"
                        ));
                    }
                }
                Some(listed) => {
                    let got = got.map(|action| format!("{action:?}"));
                    if got.as_deref() != Some(listed.as_str()) {
                        wrong.push(format!(
                            "{screen} `{name}`: listed as {listed} but runs {got:?}"
                        ));
                    }
                }
                None if echo || keymap.moved_from(moved_place, name).is_some() => {}
                // A text field or the pane may leave a key unanswered.
                None if matches!(unlisted, Unlisted::Nothing) && got.is_none() => {}
                None if got != unlisted_does => {
                    wrong.push(format!(
                        "{screen} `{name}`: runs {got:?} (unlisted: {unlisted_does:?}) but the definition doesn't list it"
                    ));
                }
                None => {}
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Screens the probe leaves out, and why. Every other place outside the
/// mail contexts with keys of its own must be in `SCREENS`.
const NOT_PROBED: &[&str] = &[
    // Dispatched from the definition itself (`compose_assist.rs`).
    "compose-assist-proof",
];

#[test]
fn every_screen_with_keys_is_probed() {
    let keymap = mxr_keymap::defaults();
    let unprobed: Vec<&str> = mxr_keymap::SCREEN_CONTEXTS
        .into_iter()
        .filter(|place| {
            keymap
                .actions
                .iter()
                .flat_map(|a| &a.tui)
                .any(|b| !b.keys.is_empty() && b.contexts.includes(place))
        })
        .filter(|place| {
            !SCREENS.iter().any(|(screen, ..)| screen == place) && !NOT_PROBED.contains(place)
        })
        .collect();
    assert!(unprobed.is_empty(), "add {unprobed:?} to SCREENS");
}
