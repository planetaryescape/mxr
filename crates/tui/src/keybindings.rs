use crate::action::Action;
use mxr_protocol::CalendarInviteActionData;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
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

    if let Some(rest) = s.strip_prefix("Ctrl-Alt-") {
        let ch = rest.chars().next().ok_or("Missing char after Ctrl-Alt-")?;
        keys.push(KeyPress {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::CONTROL | KeyModifiers::ALT,
        });
    } else if let Some(rest) = s.strip_prefix("Ctrl-") {
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
        "summarize_current_thread" | "summarize_thread" => Some(Action::SummarizeCurrentThread),
        "invite_accept" => Some(Action::RespondInvite(CalendarInviteActionData::Accept)),
        "invite_tentative" | "invite_maybe" => {
            Some(Action::RespondInvite(CalendarInviteActionData::Tentative))
        }
        "invite_decline" => Some(Action::RespondInvite(CalendarInviteActionData::Decline)),
        "archive" => Some(Action::Archive),
        "mark_read_archive" => Some(Action::MarkReadAndArchive),
        "trash" => Some(Action::Trash),
        "spam" => Some(Action::Spam),
        "undo_last_mutation" => Some(Action::UndoLastMutation),
        "star" => Some(Action::Star),
        "mark_read" => Some(Action::MarkRead),
        "mark_unread" => Some(Action::MarkUnread),
        "apply_label" => Some(Action::ApplyLabel),
        "move_to_label" => Some(Action::MoveToLabel),
        "route_to_label" => Some(Action::RouteToLabel),
        "toggle_select" => Some(Action::ToggleSelect),
        // mxr-specific
        "unsubscribe" => Some(Action::Unsubscribe),
        "snooze" => Some(Action::Snooze),
        "flag_reply_later" => Some(Action::FlagReplyLater),
        "cancel_reminder" => Some(Action::CancelAutoReminder),
        "open_in_browser" => Some(Action::OpenInBrowser),
        "toggle_reader_mode" => Some(Action::ToggleReaderMode),
        "toggle_html_view" => Some(Action::ToggleHtmlView),
        "toggle_remote_content" => Some(Action::ToggleRemoteContent),
        "export_thread" => Some(Action::ExportThread),
        "command_palette" => Some(Action::OpenCommandPalette),
        "switch_panes" => Some(Action::SwitchPane),
        "toggle_fullscreen" => Some(Action::ToggleFullscreen),
        "visual_line_mode" => Some(Action::VisualLineMode),
        "attachment_list" => Some(Action::AttachmentList),
        "open_links" => Some(Action::OpenLinks),
        "sync" => Some(Action::SyncNow),
        "enable_semantic" => Some(Action::EnableSemantic),
        "disable_semantic" => Some(Action::DisableSemantic),
        "reindex_semantic" => Some(Action::ReindexSemantic),
        "backfill_semantic" => Some(Action::BackfillSemantic),
        "draft_assist" | "draft_assist_thread" => Some(Action::DraftAssistCurrentThread),
        "draft_new_for_sender" => Some(Action::DraftNewForSender),
        "open_voice_profile" | "voice_profile" => Some(Action::OpenVoiceProfile),
        "rebuild_user_voice" => Some(Action::RebuildUserVoice),
        "open_commitments" | "commitments" => Some(Action::OpenCommitments),
        "open_activity" | "activity" | "open_activity_screen" => Some(Action::OpenActivityScreen),
        "open_stored_drafts" => Some(Action::OpenStoredDrafts),
        "close_activity_modal" => Some(Action::CloseActivityModal),
        "activity_next" => Some(Action::ActivityModalNext),
        "activity_prev" => Some(Action::ActivityModalPrev),
        "activity_toggle_pause" => Some(Action::ActivityTogglePause),
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
        "open_tab_6" => Some(Action::OpenTab6),
        "toggle_signature" => Some(Action::ToggleSignature),
        "show_onboarding" => Some(Action::ShowOnboarding),
        #[cfg(debug_assertions)]
        "dump_action_trace" => Some(Action::DumpActionTrace),
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
            if kp.modifiers.contains(KeyModifiers::ALT) {
                s.push_str("Alt-");
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

/// Shortest formatted mail-list key chord for [`Action`], sourced from the
/// default bindings registry (`default_keybindings` mail list map). Used so
/// the command palette shortcuts stay aligned with canonical keybindings.
pub(crate) fn primary_mail_list_key_display(action: &Action) -> Option<String> {
    primary_key_display(ViewContext::MailList, action)
}

/// Shortest formatted key chord bound to [`Action`] in `context`, sourced
/// from the bindings registry. Context-aware sibling of
/// [`primary_mail_list_key_display`] so the command palette can show the
/// key that actually fires in the focused view.
pub(crate) fn primary_key_display(context: ViewContext, action: &Action) -> Option<String> {
    let cfg = default_keybindings();
    let map = match context {
        ViewContext::MailList => &cfg.mail_list,
        ViewContext::MessageView => &cfg.message_view,
        ViewContext::ThreadView => &cfg.thread_view,
    };
    let mut matches: Vec<String> = map
        .iter()
        .filter_map(|(binding, name)| {
            let mapped = action_from_name(name)?;
            if mapped != *action {
                return None;
            }
            Some(format_keybinding(binding))
        })
        .collect();
    if matches.is_empty() {
        None
    } else {
        matches.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        matches.first().cloned()
    }
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
        "summarize_current_thread" | "summarize_thread" => "Summary".into(),
        "invite_accept" => "Accept Invite".into(),
        "invite_tentative" | "invite_maybe" => "Maybe".into(),
        "invite_decline" => "Decline Invite".into(),
        "archive" => "Archive".into(),
        "mark_read_archive" => "Read + Archive".into(),
        "star" => "Star".into(),
        "mark_read" => "Mark Read".into(),
        "mark_unread" => "Mark Unread".into(),
        "unsubscribe" => "Unsubscribe".into(),
        "snooze" => "Snooze".into(),
        "visual_line_mode" => "Visual Line Mode".into(),
        "toggle_fullscreen" => "Full View".into(),
        "toggle_select" => "Toggle Select".into(),
        "go_inbox" => "Go Inbox".into(),
        "edit_config" => "Edit Config".into(),
        "open_logs" => "Open Logs".into(),
        "switch_panes" => "Switch Pane".into(),
        "next_message" => "Next Msg".into(),
        "prev_message" => "Prev Msg".into(),
        "attachment_list" => "Attachments".into(),
        "open_links" => "Open Links".into(),
        "toggle_reader_mode" => "Reading View".into(),
        "toggle_html_view" => "Original HTML".into(),
        "toggle_remote_content" => "Remote Images".into(),
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
        #[cfg(debug_assertions)]
        "dump_action_trace" => "Dump Trace".into(),
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

// Mail list defaults — Gmail-native scheme (A005). Module-level so tests
// can assert the table has no duplicate keys before HashMap insertion
// silently drops one of the colliding entries.
const ML_DEFAULTS: &[(&str, &str)] = &[
    // Navigation (vim-native)
    ("j", "move_down"),
    ("k", "move_up"),
    ("gg", "jump_top"),
    ("gy", "open_activity"),
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
    ("Escape", "clear_selection"),
    // Email actions (Gmail-native A005)
    ("c", "compose"),
    ("r", "reply"),
    ("a", "reply_all"),
    ("f", "forward"),
    ("y", "summarize_current_thread"),
    ("ia", "invite_accept"),
    ("im", "invite_tentative"),
    ("id", "invite_decline"),
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
    ("b", "flag_reply_later"),
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
    ("gA", "draft_assist"),
    ("gD", "draft_new_for_sender"),
    ("gC", "open_commitments"),
    ("gV", "open_voice_profile"),
    ("gE", "open_stored_drafts"),
];

// Message view defaults
const MV_DEFAULTS: &[(&str, &str)] = &[
    ("j", "scroll_down"),
    ("k", "scroll_up"),
    ("J", "next_message"),
    ("K", "prev_message"),
    ("F", "toggle_fullscreen"),
    ("R", "toggle_reader_mode"),
    ("H", "toggle_html_view"),
    ("M", "toggle_remote_content"),
    ("O", "open_in_browser"),
    ("A", "attachment_list"),
    ("L", "open_links"),
    ("r", "reply"),
    ("a", "reply_all"),
    ("f", "forward"),
    ("y", "summarize_current_thread"),
    ("ia", "invite_accept"),
    ("im", "invite_tentative"),
    ("id", "invite_decline"),
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
    ("gA", "draft_assist"),
    ("gD", "draft_new_for_sender"),
    ("gC", "open_commitments"),
    ("gV", "open_voice_profile"),
];

// Thread view defaults
const TV_DEFAULTS: &[(&str, &str)] = &[
    ("j", "next_message"),
    ("k", "prev_message"),
    ("F", "toggle_fullscreen"),
    ("r", "reply"),
    ("a", "reply_all"),
    ("f", "forward"),
    ("y", "summarize_current_thread"),
    ("ia", "invite_accept"),
    ("im", "invite_tentative"),
    ("id", "invite_decline"),
    ("A", "attachment_list"),
    ("L", "open_links"),
    ("R", "toggle_reader_mode"),
    ("H", "toggle_html_view"),
    ("M", "toggle_remote_content"),
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
    ("gA", "draft_assist"),
    ("gD", "draft_new_for_sender"),
    ("gC", "open_commitments"),
    ("gV", "open_voice_profile"),
];

pub fn default_keybindings() -> KeybindingConfig {
    let mut mail_list = HashMap::new();
    let mut message_view = HashMap::new();
    let mut thread_view = HashMap::new();

    for (key, action) in ML_DEFAULTS {
        if let Ok(kb) = parse_key_string(key) {
            mail_list.insert(kb, action.to_string());
        }
    }
    for (key, action) in MV_DEFAULTS {
        if let Ok(kb) = parse_key_string(key) {
            message_view.insert(kb, action.to_string());
        }
    }
    for (key, action) in TV_DEFAULTS {
        if let Ok(kb) = parse_key_string(key) {
            thread_view.insert(kb, action.to_string());
        }
    }
    #[cfg(debug_assertions)]
    if let Ok(kb) = parse_key_string("Ctrl-Alt-d") {
        mail_list.insert(kb.clone(), "dump_action_trace".to_string());
        message_view.insert(kb.clone(), "dump_action_trace".to_string());
        thread_view.insert(kb, "dump_action_trace".to_string());
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
        let actions: Vec<&str> = config
            .mail_list
            .values()
            .map(std::string::String::as_str)
            .collect();
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
        assert!(actions.contains(&"draft_assist"));
        assert!(actions.contains(&"draft_new_for_sender"));
        assert!(actions.contains(&"open_commitments"));
        assert!(actions.contains(&"open_voice_profile"));
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
        assert!(action_from_name("enable_semantic").is_some());
        assert!(action_from_name("disable_semantic").is_some());
        assert!(action_from_name("reindex_semantic").is_some());
        assert!(action_from_name("draft_assist").is_some());
        assert!(action_from_name("draft_new_for_sender").is_some());
        assert!(action_from_name("open_commitments").is_some());
        assert!(action_from_name("open_voice_profile").is_some());
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
        assert!(labels.contains(&"Full View".to_string()));
        assert!(labels.contains(&"Visual Line Mode".to_string()));
        assert!(labels.contains(&"Go Inbox".to_string()));
        assert!(labels.contains(&"Edit Config".to_string()));
    }

    #[test]
    fn primary_key_display_resolves_per_context() {
        // Compose is bound in the mail list but not the thread view.
        assert_eq!(
            primary_key_display(ViewContext::MailList, &Action::Compose),
            Some("c".to_string())
        );
        assert_eq!(
            primary_key_display(ViewContext::ThreadView, &Action::Compose),
            None
        );
        // Open Links exists only in message/thread views.
        assert_eq!(
            primary_key_display(ViewContext::MailList, &Action::OpenLinks),
            None
        );
        assert_eq!(
            primary_key_display(ViewContext::ThreadView, &Action::OpenLinks),
            Some("L".to_string())
        );
    }

    #[test]
    fn display_bindings_for_actions_joins_aliases_stably() {
        let bindings = display_bindings_for_actions(ViewContext::MailList, &["open"]);
        assert_eq!(bindings, vec![("Enter/o".to_string(), "Open".to_string())]);
    }

    /// HashMap insertion makes a duplicate key in the default tables a
    /// silent last-one-wins bug (the `ga` open_activity/go_all_mail
    /// collision shipped exactly this way). Catch it at the source table.
    #[test]
    fn default_tables_have_no_duplicate_keys() {
        for (name, table) in [
            ("mail_list", ML_DEFAULTS),
            ("message_view", MV_DEFAULTS),
            ("thread_view", TV_DEFAULTS),
        ] {
            let mut seen: HashMap<&str, &str> = HashMap::new();
            for (key, action) in table {
                if let Some(prev) = seen.insert(key, action) {
                    panic!(
                        "duplicate key '{key}' in {name} defaults: bound to both '{prev}' and '{action}'"
                    );
                }
            }
        }
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
