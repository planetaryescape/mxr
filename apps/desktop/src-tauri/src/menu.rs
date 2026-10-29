//! The app menu, built from the key definition (`crates/keymap/keymap.toml`,
//! blueprint 35, D169) so its commands are the web app's actions.
//!
//! A command's title is its action's label and its id is the action id.
//! Choosing it sends the id to the web app, which runs it through the action
//! registry as a key press would (`apps/web/src/hooks/useDesktopMenu.ts`).
//! macOS App Shortcuts rebinds a menu command by its title, so titles stay
//! the definition's labels and never carry a key.
//!
//! Only commands whose key is already a modifier chord get an accelerator.
//! A menu can't take a bare letter or a `g` sequence, and giving every verb
//! Command plus its letter would take keys the web composer and text fields
//! need (⌘B bold, ⌘Z undo typing). The single keys keep working in the page.

use serde::Deserialize;
use std::sync::OnceLock;
use tauri::menu::{Menu, MenuItem, Submenu, SubmenuBuilder, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Runtime, Url};

const DEFINITION: &str = include_str!("../../../../crates/keymap/keymap.toml");

/// The Message menu: the mail verbs, grouped as Mail and Gmail group them.
const MESSAGE: &[&[&str]] = &[
    &["mail.reply", "mail.reply-all", "mail.forward"],
    &["mail.archive", "mail.snooze", "modes.move", "mail.move"],
    &["mail.star", "mail.mark-read", "mail.mark-unread"],
    &["mail.trash"],
];

/// The View menu's commands: ways in to the rest of the app.
const VIEW: &[&[&str]] = &[
    &["shell.command-palette", "assistant.toggle"],
    &["shell.help"],
];

/// Command-key chords the standard menus or macOS already own (the
/// blueprint's ⌘W, ⌘N, ⌘Q and ⌘, plus the Edit, Hide and Minimize keys).
const RESERVED: &[&str] = &["a", "c", "v", "x", "z", "w", "n", "q", ",", "h", "m"];

#[derive(Debug, Deserialize)]
struct Definition {
    action: Vec<DefinedAction>,
}

#[derive(Debug, Deserialize)]
struct DefinedAction {
    id: String,
    label: String,
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    clients: Option<Vec<String>>,
}

impl DefinedAction {
    fn on_web(&self) -> bool {
        self.clients
            .as_ref()
            .is_none_or(|clients| clients.iter().any(|client| client == "web"))
    }

    fn global(&self) -> bool {
        self.scopes.is_empty() || self.scopes.iter().any(|scope| scope == "global")
    }

    /// The key after `g` when this action's primary key goes somewhere.
    fn place_key(&self) -> Option<&str> {
        self.keys.first()?.strip_prefix("g ")
    }

    fn accelerator(&self) -> Option<String> {
        self.keys.iter().find_map(|key| {
            let rest = key.strip_prefix("Mod+")?;
            let (shift, key) = match rest.strip_prefix("Shift+") {
                Some(key) => ("Shift+", key),
                None => ("", rest),
            };
            if key.chars().count() != 1 || RESERVED.contains(&key.to_lowercase().as_str()) {
                return None;
            }
            Some(format!("CmdOrCtrl+{shift}{}", key.to_uppercase()))
        })
    }
}

fn definition() -> &'static Definition {
    static PARSED: OnceLock<Definition> = OnceLock::new();
    PARSED.get_or_init(|| toml::from_str(DEFINITION).expect("crates/keymap/keymap.toml must parse"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub title: String,
    pub accelerator: Option<String>,
}

/// A menu of commands, split into groups by separators.
#[derive(Debug)]
pub struct Section {
    pub title: &'static str,
    pub groups: Vec<Vec<Command>>,
}

fn command(action: &DefinedAction) -> Command {
    Command {
        id: action.id.clone(),
        title: action.label.clone(),
        accelerator: action.accelerator(),
    }
}

fn listed(groups: &[&[&str]]) -> Vec<Vec<Command>> {
    groups
        .iter()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| {
                    definition()
                        .action
                        .iter()
                        .find(|action| action.id == *id && action.on_web())
                })
                .map(command)
                .collect()
        })
        .collect()
}

/// Go is every place `g` reaches from anywhere, in the definition's order:
/// mail places and modes, then the tools behind a capital (D166). The saved
/// searches' digits and `g g` (top of the list) aren't places.
fn go() -> Vec<Vec<Command>> {
    let places = || {
        definition()
            .action
            .iter()
            .filter(|action| action.on_web() && action.global())
            .filter_map(|action| Some((action, action.place_key()?)))
            .filter(|(_, key)| *key != "g" && !key.starts_with(|c: char| c.is_ascii_digit()))
    };
    let tool = |key: &str| key.starts_with(|c: char| c.is_ascii_uppercase());
    vec![
        places()
            .filter(|(_, key)| !tool(key))
            .map(|(action, _)| command(action))
            .collect(),
        places()
            .filter(|(_, key)| tool(key))
            .map(|(action, _)| command(action))
            .collect(),
    ]
}

/// The menus built from the definition, in menu-bar order.
pub fn sections() -> &'static [Section] {
    static SECTIONS: OnceLock<Vec<Section>> = OnceLock::new();
    SECTIONS.get_or_init(|| {
        vec![
            Section {
                title: "View",
                groups: listed(VIEW),
            },
            Section {
                title: "Go",
                groups: go(),
            },
            Section {
                title: "Message",
                groups: listed(MESSAGE),
            },
        ]
    })
}

/// True for a menu id that names a web action.
pub fn is_command(id: &str) -> bool {
    sections()
        .iter()
        .flat_map(|section| section.groups.iter().flatten())
        .any(|command| command.id == id)
}

/// The text muda needs to show `title` as written. muda reads a lone `&` as a
/// mnemonic marker and drops it (on macOS too, where there are no mnemonics),
/// so "Focus & reply" would show as "Focus  reply"; `&&` shows one `&`.
/// App Shortcuts matches the shown title, so it has to be the label.
fn menu_text(title: &str) -> String {
    title.replace('&', "&&")
}

fn submenu<R: Runtime>(app: &AppHandle<R>, section: &Section) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, section.title);
    for (index, group) in section.groups.iter().enumerate() {
        if index > 0 {
            builder = builder.separator();
        }
        for command in group {
            builder = builder.item(&MenuItem::with_id(
                app,
                &command.id,
                menu_text(&command.title),
                true,
                command.accelerator.as_deref(),
            )?);
        }
    }
    // macOS adds Enter Full Screen to a View menu; keep it where it expects.
    #[cfg(target_os = "macos")]
    if section.title == "View" {
        builder = builder.separator().fullscreen();
    }
    builder.build()
}

/// The standard App, File, Edit, Window and Help menus around the
/// definition's View, Go and Message, as Tauri's default menu lays them out.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    menu.append(
        &SubmenuBuilder::new(app, app.package_info().name.clone())
            .about(None)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?,
    )?;
    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    menu.append(
        &SubmenuBuilder::new(app, "File")
            .close_window()
            .quit()
            .build()?,
    )?;
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    menu.append(&SubmenuBuilder::new(app, "File").quit().build()?)?;
    menu.append(
        &SubmenuBuilder::new(app, "Edit")
            .undo()
            .redo()
            .separator()
            .cut()
            .copy()
            .paste()
            .select_all()
            .build()?,
    )?;
    for section in sections() {
        menu.append(&submenu(app, section)?)?;
    }
    menu.append(
        &SubmenuBuilder::with_id(app, WINDOW_SUBMENU_ID, "Window")
            .minimize()
            .maximize()
            .separator()
            .close_window()
            .build()?,
    )?;
    menu.append(&SubmenuBuilder::with_id(app, HELP_SUBMENU_ID, "Help").build()?)?;
    Ok(menu)
}

/// Script that runs a menu command in the web app. The app sets
/// `mxrMenuReady` once it listens; until then, or on the down page, the
/// command does nothing rather than navigate away from what is open.
pub fn menu_action_script(bridge: &Url, id: &str) -> Option<String> {
    let origin = bridge.origin().ascii_serialization();
    // JSON strings are valid JS string literals, so neither value can break out.
    let origin = serde_json::to_string(&origin).ok()?;
    let id = serde_json::to_string(id).ok()?;
    Some(format!(
        "(function(o,id){{if(location.origin===o&&window.mxrMenuReady===true)\
         {{window.dispatchEvent(new CustomEvent('mxr:menu-action',{{detail:id}}));}}}})({origin},{id});"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<&'static Command> {
        sections()
            .iter()
            .flat_map(|section| section.groups.iter().flatten())
            .collect()
    }

    fn titles(section: &str) -> Vec<&'static str> {
        sections()
            .iter()
            .find(|s| s.title == section)
            .unwrap()
            .groups
            .iter()
            .flatten()
            .map(|command| command.title.as_str())
            .collect()
    }

    /// The listed menus can't drift from the definition: an id that was
    /// renamed or lost its web handler would drop out of its menu silently.
    #[test]
    fn every_listed_command_is_a_web_action() {
        for ids in MESSAGE.iter().chain(VIEW) {
            for id in *ids {
                assert!(
                    definition()
                        .action
                        .iter()
                        .any(|action| action.id == *id && action.on_web()),
                    "{id} is not a web action in crates/keymap/keymap.toml"
                );
            }
        }
    }

    #[test]
    fn go_lists_the_places_then_the_tools() {
        let go = &sections().iter().find(|s| s.title == "Go").unwrap().groups;
        let ids = |group: &Vec<Command>| -> Vec<String> {
            group.iter().map(|command| command.id.clone()).collect()
        };
        for place in [
            "nav.now",
            "nav.messages",
            "nav.todo",
            "nav.inbox",
            "nav.trash",
        ] {
            assert!(ids(&go[0]).contains(&place.to_string()), "{place}");
        }
        for tool in ["nav.rules", "nav.settings", "nav.diagnostics"] {
            assert!(ids(&go[1]).contains(&tool.to_string()), "{tool}");
        }
        assert!(!ids(&go[0])
            .iter()
            .any(|id| id.starts_with("nav.saved-search")));
    }

    #[test]
    fn the_message_menu_has_the_mail_verbs() {
        assert_eq!(
            titles("Message"),
            [
                "Reply",
                "Reply all",
                "Forward",
                "Archive",
                "Not now…",
                "Move to…",
                "Move to label…",
                "Star or unstar",
                "Mark read",
                "Mark unread",
                "Move to Trash",
            ]
        );
    }

    /// App Shortcuts finds a command by its title, so two commands may not
    /// share one, and a title never carries a key.
    #[test]
    fn titles_are_unique_and_name_no_key() {
        let all = commands();
        for command in &all {
            let same = all.iter().filter(|c| c.title == command.title).count();
            assert_eq!(same, 1, "two commands are titled {:?}", command.title);
            assert!(!command.title.contains('('), "{:?}", command.title);
        }
    }

    /// muda drops a lone `&` as a mnemonic marker, so a label's `&` is
    /// doubled to show as written.
    #[test]
    fn an_ampersand_in_a_label_is_escaped() {
        assert_eq!(menu_text("Focus & reply"), "Focus && reply");
        assert_eq!(menu_text("Go to Inbox"), "Go to Inbox");
        let focus = commands()
            .into_iter()
            .find(|command| command.id == "nav.focus")
            .unwrap();
        assert_eq!(focus.title, "Focus & reply");
        assert_eq!(menu_text(&focus.title), "Focus && reply");
    }

    #[test]
    fn only_modifier_chords_become_accelerators() {
        let accelerator = |id: &str| {
            commands()
                .into_iter()
                .find(|command| command.id == id)
                .unwrap()
                .accelerator
                .clone()
        };
        assert_eq!(
            accelerator("shell.command-palette").as_deref(),
            Some("CmdOrCtrl+K")
        );
        assert_eq!(
            accelerator("assistant.toggle").as_deref(),
            Some("CmdOrCtrl+.")
        );
        for single in ["mail.archive", "mail.reply", "nav.inbox", "shell.help"] {
            assert_eq!(accelerator(single), None, "{single}");
        }
        for command in commands() {
            if let Some(key) = &command.accelerator {
                let letter = key.rsplit('+').next().unwrap().to_lowercase();
                assert!(!RESERVED.contains(&letter.as_str()), "{key}");
            }
        }
    }

    #[test]
    fn the_menu_script_sends_the_id_only_to_the_ready_app() {
        let bridge = Url::parse("http://mxr.localhost:42829/#token=s3cret").unwrap();
        let script = menu_action_script(&bridge, "mail.archive").unwrap();
        assert!(script.contains("\"http://mxr.localhost:42829\""));
        assert!(script.contains("\"mail.archive\""));
        assert!(script.contains("mxrMenuReady===true"));
        assert!(script.contains("mxr:menu-action"));
        assert!(!script.contains("s3cret"));
        let quoted = menu_action_script(&bridge, "x\"),alert(1)//").unwrap();
        assert!(quoted.contains(r#""x\"),alert(1)//""#));
    }
}
