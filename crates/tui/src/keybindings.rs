use crate::mxr_tui::action::Action;
use crossterm::event::{KeyCode, KeyModifiers};
use serde::Deserialize;
use std::collections::HashMap;

/// Parsed keybinding configuration.
#[derive(Debug, Clone)]
pub struct KeybindingConfig {
    pub mail_list: HashMap<KeyBinding, String>,
    pub message_view: HashMap<KeyBinding, String>,
    pub thread_view: HashMap<KeyBinding, String>,
}

/// A single key or key combination.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub keys: Vec<KeyPress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPress {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// View context for resolving keybindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewContext {
    MailList,
    MessageView,
    ThreadView,
}

/// Parse a key string like "Ctrl-p", "gg", "G", "/", "Enter" into a KeyBinding.
pub fn parse_key_string(s: &str) -> Result<KeyBinding, String> {
    let mut keys = Vec::new();

    if let Some(rest) = s.strip_prefix("Ctrl-") {
        let ch = rest.chars().next().ok_or("Missing char after Ctrl-")?;
        keys.push(KeyPress {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::CONTROL,
        });
    } else if s == "Enter" {
        keys.push(KeyPress {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
        });
    } else if s == "Escape" || s == "Esc" {
        keys.push(KeyPress {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
        });
    } else if s == "Tab" {
        keys.push(KeyPress {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
        });
    } else {
        for ch in s.chars() {
            let modifiers = if ch.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            keys.push(KeyPress {
                code: KeyCode::Char(ch),
                modifiers,
            });
        }
    }

    Ok(KeyBinding { keys })
}

/// Resolve a key sequence to an action name.
pub fn resolve_action(
    config: &KeybindingConfig,
    context: ViewContext,
    key_sequence: &[KeyPress],
) -> Option<String> {
    let map = match context {
        ViewContext::MailList => &config.mail_list,
        ViewContext::MessageView => &config.message_view,
        ViewContext::ThreadView => &config.thread_view,
    };

    let binding = KeyBinding {
        keys: key_sequence.to_vec(),
    };
    map.get(&binding).cloned()
}

/// Map action name strings to Action enum variants.
pub fn action_from_name(name: &str) -> Option<Action> {
    match name {
        // Navigation (vim-native)
        "move_down" | "scroll_down" | "next_message" => Some(Action::MoveDown),
        "move_up" | "scroll_up" | "prev_message" => Some(Action::MoveUp),
        "jump_top" => Some(Action::JumpTop),
        "jump_bottom" => Some(Action::JumpBottom),
        "page_down" => Some(Action::PageDown),
        "page_up" => Some(Action::PageUp),
        "visible_top" => Some(Action::ViewportTop),
        "visible_middle" => Some(Action::ViewportMiddle),
        "visible_bottom" => Some(Action::ViewportBottom),
        "center_current" => Some(Action::CenterCurrent),
        "search" | "search_all_mail" => Some(Action::OpenGlobalSearch),
        "mailbox_filter" => Some(Action::OpenMailboxFilter),
        "next_search_result" => Some(Action::NextSearchResult),
        "prev_search_result" => Some(Action::PrevSearchResult),
        "open" => Some(Action::OpenSelected),
        "quit_view" => Some(Action::QuitView),
        "clear_selection" => Some(Action::ClearSelection),
        "help" => Some(Action::Help),
        "toggle_mail_list_mode" => Some(Action::ToggleMailListMode),
        // Email actions (Gmail-native A005)
        "compose" => Some(Action::Compose),
        "reply" => Some(Action::Reply),
        "reply_all" => Some(Action::ReplyAll),
        "forward" => Some(Action::Forward),
        "archive" => Some(Action::Archive),
        "mark_read_archive" => Some(Action::MarkReadAndArchive),
        "trash" => Some(Action::Trash),
        "spam" => Some(Action::Spam),
        "star" => Some(Action::Star),
        "mark_read" => Some(Action::MarkRead),
        "mark_unread" => Some(Action::MarkUnread),
        "apply_label" => Some(Action::ApplyLabel),
        "move_to_label" => Some(Action::MoveToLabel),
        "toggle_select" => Some(Action::ToggleSelect),
        // mxr-specific
        "unsubscribe" => Some(Action::Unsubscribe),
        "snooze" => Some(Action::Snooze),
        "open_in_browser" => Some(Action::OpenInBrowser),
        "toggle_reader_mode" => Some(Action::ToggleReaderMode),
        "export_thread" => Some(Action::ExportThread),
        "command_palette" => Some(Action::OpenCommandPalette),
        "switch_panes" => Some(Action::SwitchPane),
        "toggle_fullscreen" => Some(Action::ToggleFullscreen),
        "visual_line_mode" => Some(Action::VisualLineMode),
        "attachment_list" => Some(Action::AttachmentList),
        "open_links" => Some(Action::OpenLinks),
        "sync" => Some(Action::SyncNow),
        // Go-to navigation (A005)
        "go_inbox" => Some(Action::GoToInbox),
        "go_starred" => Some(Action::GoToStarred),
        "go_sent" => Some(Action::GoToSent),
        "go_drafts" => Some(Action::GoToDrafts),
        "go_all_mail" => Some(Action::GoToAllMail),
        "go_label" => Some(Action::GoToLabel),
        "edit_config" => Some(Action::EditConfig),
        "open_logs" => Some(Action::OpenLogs),
        "open_tab_1" => Some(Action::OpenTab1),
        "open_tab_2" => Some(Action::OpenTab2),
        "open_tab_3" => Some(Action::OpenTab3),
        "open_tab_4" => Some(Action::OpenTab4),
        "open_tab_5" => Some(Action::OpenTab5),
        "toggle_signature" => Some(Action::ToggleSignature),
        "show_onboarding" => Some(Action::ShowOnboarding),
        _ => None,
    }
}

/// Format a keybinding for display.
pub fn format_keybinding(kb: &KeyBinding) -> String {
    kb.keys
        .iter()
        .map(|kp| {
            let mut s = String::new();
            if kp.modifiers.contains(KeyModifiers::CONTROL) {
                s.push_str("Ctrl-");
            }
            match kp.code {
                KeyCode::Char(c) => s.push(c),
                KeyCode::Enter => s.push_str("Enter"),
                KeyCode::Esc => s.push_str("Esc"),
                KeyCode::Tab => s.push_str("Tab"),
                _ => s.push('?'),
            }
            s
        })
        .collect::<Vec<_>>()
        .join("")
}

pub fn display_bindings_for_actions(
    context: ViewContext,
    actions: &[&str],
) -> Vec<(String, String)> {
    let config = default_keybindings();
    let map = match context {
        ViewContext::MailList => &config.mail_list,
        ViewContext::MessageView => &config.message_view,
        ViewContext::ThreadView => &config.thread_view,
    };

    actions
        .iter()
        .filter_map(|action| {
            let mut bindings: Vec<String> = map
                .iter()
                .filter(|(_, name)| name == action)
                .map(|(binding, _)| format_keybinding(binding))
                .collect();
            bindings.sort();
            bindings.dedup();

            (!bindings.is_empty()).then(|| (bindings.join("/"), action_display_name(action)))
        })
        .collect()
}

pub fn all_bindings_for_context(context: ViewContext) -> Vec<(String, String)> {
    let config = default_keybindings();
    let map = match context {
        ViewContext::MailList => &config.mail_list,
        ViewContext::MessageView => &config.message_view,
        ViewContext::ThreadView => &config.thread_view,
    };

    let mut entries: Vec<(String, String)> = map
        .iter()
        .map(|(binding, action)| (format_keybinding(binding), action_display_name(action)))
        .collect();
    entries.sort_by(|(left_key, left_action), (right_key, right_action)| {
        left_key
            .cmp(right_key)
            .then_with(|| left_action.cmp(right_action))
    });
    entries
}

fn action_display_name(action: &str) -> String {
    match action {
        "move_down" => "Down".into(),
        "move_up" => "Up".into(),
        "search" | "search_all_mail" => "Search All Mail".into(),
        "mailbox_filter" => "Filter Mailbox".into(),
        "open" => "Open".into(),
        "apply_label" => "Apply Label".into(),
        "move_to_label" => "Move Label".into(),
        "command_palette" => "Commands".into(),
        "help" => "Help".into(),
        "reply" => "Reply".into(),
        "reply_all" => "Reply All".into(),
        "forward" => "Forward".into(),
        "archive" => "Archive".into(),
        "mark_read_archive" => "Read + Archive".into(),
        "star" => "Star".into(),
        "mark_read" => "Mark Read".into(),
        "mark_unread" => "Mark Unread".into(),
        "unsubscribe" => "Unsubscribe".into(),
        "snooze" => "Snooze".into(),
        "visual_line_mode" => "Visual Line Mode".into(),
        "toggle_fullscreen" => "Toggle Fullscreen".into(),
        "toggle_select" => "Toggle Select".into(),
        "go_inbox" => "Go Inbox".into(),
        "edit_config" => "Edit Config".into(),
        "open_logs" => "Open Logs".into(),
        "switch_panes" => "Switch Pane".into(),
        "next_message" => "Next Msg".into(),
        "prev_message" => "Prev Msg".into(),
        "attachment_list" => "Attachments".into(),
        "open_links" => "Open Links".into(),
        "toggle_reader_mode" => "Reader".into(),
        "toggle_signature" => "Signature".into(),
        "export_thread" => "Export".into(),
        "open_in_browser" => "Browser".into(),
        "open_tab_1" => "Mailbox".into(),
        "open_tab_2" => "Search Page".into(),
        "open_tab_3" => "Rules Page".into(),
        "open_tab_4" => "Accounts Page".into(),
        "open_tab_5" => "Diagnostics Page".into(),
        "show_onboarding" => "Start Here".into(),
        "quit_view" => "Quit".into(),
        "clear_selection" => "Clear Sel".into(),
        _ => action
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Raw TOML structure for keys.toml
#[derive(Debug, Deserialize)]
pub struct KeysToml {
    #[serde(default)]
    pub mail_list: HashMap<String, String>,
    #[serde(default)]
    pub message_view: HashMap<String, String>,
    #[serde(default)]
    pub thread_view: HashMap<String, String>,
}

/// Load keybinding config from keys.toml, falling back to defaults.
pub fn load_keybindings(config_dir: &std::path::Path) -> KeybindingConfig {
    let keys_path = config_dir.join("keys.toml");
    let user_config = if keys_path.exists() {
        std::fs::read_to_string(&keys_path)
            .ok()
            .and_then(|s| toml::from_str::<KeysToml>(&s).ok())
    } else {
        None
    };

    let mut config = default_keybindings();

    if let Some(user) = user_config {
        for (key, action) in &user.mail_list {
            if let Ok(kb) = parse_key_string(key) {
                config.mail_list.insert(kb, action.clone());
            }
        }
        for (key, action) in &user.message_view {
            if let Ok(kb) = parse_key_string(key) {
                config.message_view.insert(kb, action.clone());
            }
        }
        for (key, action) in &user.thread_view {
            if let Ok(kb) = parse_key_string(key) {
                config.thread_view.insert(kb, action.clone());
            }
        }
    }

    config
}

pub fn default_keybindings() -> KeybindingConfig {
    let mut mail_list = HashMap::new();
    let mut message_view = HashMap::new();
    let mut thread_view = HashMap::new();

    // Mail list defaults — Gmail-native scheme (A005)
    let ml_defaults = [
        // Navigation (vim-native)
        ("j", "move_down"),
        ("k", "move_up"),
        ("gg", "jump_top"),
        ("G", "jump_bottom"),
        ("Ctrl-d", "page_down"),
        ("Ctrl-u", "page_up"),
        ("H", "visible_top"),
        ("M", "visible_middle"),
        ("L", "visible_bottom"),
        ("zz", "center_current"),
        ("/", "search_all_mail"),
        ("Ctrl-f", "mailbox_filter"),
        ("n", "next_search_result"),
        ("N", "prev_search_result"),
        ("Enter", "open"),
        ("o", "open"),
        ("q", "quit_view"),
        ("?", "help"),
        // Email actions (Gmail-native A005)
        ("c", "compose"),
        ("r", "reply"),
        ("a", "reply_all"),
        ("f", "forward"),
        ("e", "archive"),
        ("m", "mark_read_archive"),
        ("#", "trash"),
        ("!", "spam"),
        ("s", "star"),
        ("I", "mark_read"),
        ("U", "mark_unread"),
        ("l", "apply_label"),
        ("v", "move_to_label"),
        ("x", "toggle_select"),
        // mxr-specific
        ("D", "unsubscribe"),
        ("Z", "snooze"),
        ("O", "open_in_browser"),
        ("R", "toggle_reader_mode"),
        ("S", "toggle_signature"),
        ("E", "export_thread"),
        ("V", "visual_line_mode"),
        ("Ctrl-p", "command_palette"),
        ("Tab", "switch_panes"),
        ("F", "toggle_fullscreen"),
        ("1", "open_tab_1"),
        ("2", "open_tab_2"),
        ("3", "open_tab_3"),
        ("4", "open_tab_4"),
        ("5", "open_tab_5"),
        // Gmail go-to (A005)
        ("gi", "go_inbox"),
        ("gs", "go_starred"),
        ("gt", "go_sent"),
        ("gd", "go_drafts"),
        ("ga", "go_all_mail"),
        ("gl", "go_label"),
        ("gc", "edit_config"),
        ("gL", "open_logs"),
    ];
    for (key, action) in ml_defaults {
        if let Ok(kb) = parse_key_string(key) {
            mail_list.insert(kb, action.to_string());
        }
    }

    // Message view defaults
    let mv_defaults = [
        ("j", "scroll_down"),
        ("k", "scroll_up"),
        ("R", "toggle_reader_mode"),
        ("O", "open_in_browser"),
        ("A", "attachment_list"),
        ("L", "open_links"),
        ("r", "reply"),
        ("a", "reply_all"),
        ("f", "forward"),
        ("e", "archive"),
        ("m", "mark_read_archive"),
        ("#", "trash"),
        ("!", "spam"),
        ("s", "star"),
        ("I", "mark_read"),
        ("U", "mark_unread"),
        ("D", "unsubscribe"),
        ("S", "toggle_signature"),
        ("1", "open_tab_1"),
        ("2", "open_tab_2"),
        ("3", "open_tab_3"),
        ("4", "open_tab_4"),
        ("5", "open_tab_5"),
        ("gc", "edit_config"),
        ("gL", "open_logs"),
    ];
    for (key, action) in mv_defaults {
        if let Ok(kb) = parse_key_string(key) {
            message_view.insert(kb, action.to_string());
        }
    }

    // Thread view defaults
    let tv_defaults = [
        ("j", "next_message"),
        ("k", "prev_message"),
        ("r", "reply"),
        ("a", "reply_all"),
        ("f", "forward"),
        ("A", "attachment_list"),
        ("L", "open_links"),
        ("R", "toggle_reader_mode"),
        ("E", "export_thread"),
        ("O", "open_in_browser"),
        ("e", "archive"),
        ("m", "mark_read_archive"),
        ("#", "trash"),
        ("!", "spam"),
        ("s", "star"),
        ("I", "mark_read"),
        ("U", "mark_unread"),
        ("D", "unsubscribe"),
        ("S", "toggle_signature"),
        ("1", "open_tab_1"),
        ("2", "open_tab_2"),
        ("3", "open_tab_3"),
        ("4", "open_tab_4"),
        ("5", "open_tab_5"),
        ("gc", "edit_config"),
        ("gL", "open_logs"),
    ];
    for (key, action) in tv_defaults {
        if let Ok(kb) = parse_key_string(key) {
            thread_view.insert(kb, action.to_string());
        }
    }

    KeybindingConfig {
        mail_list,
        message_view,
        thread_view,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_string_single_char() {
        let kb = parse_key_string("j").unwrap();
        assert_eq!(kb.keys.len(), 1);
        assert_eq!(kb.keys[0].code, KeyCode::Char('j'));
        assert_eq!(kb.keys[0].modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn parse_key_string_ctrl_p() {
        let kb = parse_key_string("Ctrl-p").unwrap();
        assert_eq!(kb.keys.len(), 1);
        assert_eq!(kb.keys[0].code, KeyCode::Char('p'));
        assert_eq!(kb.keys[0].modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_key_string_gg() {
        let kb = parse_key_string("gg").unwrap();
        assert_eq!(kb.keys.len(), 2);
        assert_eq!(kb.keys[0].code, KeyCode::Char('g'));
        assert_eq!(kb.keys[1].code, KeyCode::Char('g'));
    }

    #[test]
    fn parse_key_string_enter() {
        let kb = parse_key_string("Enter").unwrap();
        assert_eq!(kb.keys.len(), 1);
        assert_eq!(kb.keys[0].code, KeyCode::Enter);
    }

    #[test]
    fn parse_key_string_shift() {
        let kb = parse_key_string("G").unwrap();
        assert_eq!(kb.keys.len(), 1);
        assert_eq!(kb.keys[0].code, KeyCode::Char('G'));
        assert_eq!(kb.keys[0].modifiers, KeyModifiers::SHIFT);
    }

    #[test]
    fn default_keybindings_contain_gmail_native() {
        let config = default_keybindings();

        // Check that key actions are present
        let actions: Vec<&str> = config.mail_list.values().map(|s| s.as_str()).collect();
        assert!(actions.contains(&"compose"));
        assert!(actions.contains(&"reply"));
        assert!(actions.contains(&"reply_all"));
        assert!(actions.contains(&"archive"));
        assert!(actions.contains(&"mark_read_archive"));
        assert!(actions.contains(&"trash"));
        assert!(actions.contains(&"spam"));
        assert!(actions.contains(&"star"));
        assert!(actions.contains(&"mark_read"));
        assert!(actions.contains(&"mark_unread"));
        assert!(actions.contains(&"toggle_select"));
        assert!(actions.contains(&"unsubscribe"));
        assert!(actions.contains(&"snooze"));
        assert!(actions.contains(&"visual_line_mode"));
    }

    #[test]
    fn action_from_name_coverage() {
        // Test that all important actions are mapped
        assert!(action_from_name("compose").is_some());
        assert!(action_from_name("reply").is_some());
        assert!(action_from_name("reply_all").is_some());
        assert!(action_from_name("forward").is_some());
        assert!(action_from_name("archive").is_some());
        assert!(action_from_name("mark_read_archive").is_some());
        assert!(action_from_name("trash").is_some());
        assert!(action_from_name("spam").is_some());
        assert!(action_from_name("star").is_some());
        assert!(action_from_name("mark_read").is_some());
        assert!(action_from_name("mark_unread").is_some());
        assert!(action_from_name("unsubscribe").is_some());
        assert!(action_from_name("snooze").is_some());
        assert!(action_from_name("toggle_reader_mode").is_some());
        assert!(action_from_name("toggle_select").is_some());
        assert!(action_from_name("visual_line_mode").is_some());
        assert!(action_from_name("go_inbox").is_some());
        assert!(action_from_name("go_starred").is_some());
        assert!(action_from_name("edit_config").is_some());
        assert!(action_from_name("open_logs").is_some());
        assert!(action_from_name("nonexistent").is_none());
    }

    #[test]
    fn resolve_action_finds_match() {
        let config = default_keybindings();
        let j = KeyPress {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
        };
        let result = resolve_action(&config, ViewContext::MailList, &[j]);
        assert_eq!(result, Some("move_down".to_string()));
    }

    #[test]
    fn format_keybinding_basic() {
        let kb = parse_key_string("Ctrl-p").unwrap();
        assert_eq!(format_keybinding(&kb), "Ctrl-p");
    }

    #[test]
    fn all_bindings_for_mail_list_include_full_action_set() {
        let bindings = all_bindings_for_context(ViewContext::MailList);
        let labels: Vec<String> = bindings.into_iter().map(|(_, label)| label).collect();
        assert!(labels.contains(&"Apply Label".to_string()));
        assert!(labels.contains(&"Toggle Fullscreen".to_string()));
        assert!(labels.contains(&"Visual Line Mode".to_string()));
        assert!(labels.contains(&"Go Inbox".to_string()));
        assert!(labels.contains(&"Edit Config".to_string()));
    }

    #[test]
    fn display_bindings_for_actions_joins_aliases_stably() {
        let bindings = display_bindings_for_actions(ViewContext::MailList, &["open"]);
        assert_eq!(bindings, vec![("Enter/o".to_string(), "Open".to_string())]);
    }

    #[test]
    fn user_override_replaces_default() {
        let mut config = default_keybindings();

        // Override 'j' to do something different
        let j_key = parse_key_string("j").unwrap();
        config
            .mail_list
            .insert(j_key.clone(), "page_down".to_string());

        let j_press = KeyPress {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
        };
        let result = resolve_action(&config, ViewContext::MailList, &[j_press]);
        assert_eq!(result, Some("page_down".to_string()));
    }
}
