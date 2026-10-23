use crate::action::{Action, PatternKind};
use mxr_protocol::CalendarInviteActionData;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Parsed keybinding configuration.
#[derive(Debug, Clone)]
pub struct KeybindingConfig {
    pub mail_list: HashMap<KeyBinding, String>,
    pub message_view: HashMap<KeyBinding, String>,
    pub thread_view: HashMap<KeyBinding, String>,
    // Explicit keys and retired defaults must not reach hardcoded fallbacks.
    overrides: [HashSet<KeyBinding>; 3],
    /// The `[now]`, `[todo]` and other mode sections, keyed by the key
    /// definition's action ids.
    pub modes: mxr_keymap::ModeRemaps,
}

impl ViewContext {
    fn index(self) -> usize {
        match self {
            Self::MailList => 0,
            Self::MessageView => 1,
            Self::ThreadView => 2,
        }
    }
}

impl KeybindingConfig {
    pub(crate) fn bindings(&self, context: ViewContext) -> &HashMap<KeyBinding, String> {
        match context {
            ViewContext::MailList => &self.mail_list,
            ViewContext::MessageView => &self.message_view,
            ViewContext::ThreadView => &self.thread_view,
        }
    }

    pub(crate) fn overrides_prefix(&self, context: ViewContext, first: &KeyPress) -> bool {
        self.overrides[context.index()]
            .iter()
            .any(|binding| binding.keys.first() == Some(first))
    }

    pub(crate) fn retired_sequence(&self, context: ViewContext, sequence: &[KeyPress]) -> bool {
        self.overrides[context.index()]
            .iter()
            .any(|binding| binding.keys == sequence)
    }

    pub(crate) fn overrides_legacy_alias(
        &self,
        context: ViewContext,
        press: &KeyPress,
        legacy_action: impl FnOnce() -> Option<Action>,
    ) -> bool {
        let mut aliases = self.overrides[context.index()]
            .iter()
            .filter(|binding| {
                binding.keys.len() == 1
                    && binding.keys[0].code == press.code
                    && !binding.keys[0]
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            })
            .peekable();
        if aliases.peek().is_none() {
            return false;
        }
        // The mail router accepts #/! with any modifier. Other aliases must
        // resolve to the retired action, not merely share its character.
        if matches!(press.code, KeyCode::Char('#' | '!')) {
            return true;
        }
        let defaults = shown_keybindings();
        let registered_action = resolve_action(defaults, context, std::slice::from_ref(press))
            .and_then(|name| action_from_name(&name));
        let Some(incoming_action) = registered_action.or_else(legacy_action) else {
            return false;
        };
        aliases.any(|binding| {
            defaults
                .bindings(context)
                .get(binding)
                .and_then(|name| action_from_name(name))
                .as_ref()
                == Some(&incoming_action)
        })
    }

    pub(crate) fn primary_key_display(
        &self,
        context: ViewContext,
        action: &Action,
    ) -> Option<String> {
        let mut matches: Vec<_> = self
            .bindings(context)
            .iter()
            .filter(|(_, name)| action_from_name(name).as_ref() == Some(action))
            .map(|(binding, _)| {
                (
                    !self.overrides[context.index()].contains(binding),
                    format_keybinding(binding),
                )
            })
            .collect();
        matches.sort_by(|(a_default, a), (b_default, b)| {
            a_default
                .cmp(b_default)
                .then_with(|| a.len().cmp(&b.len()))
                .then_with(|| a.cmp(b))
        });
        matches.into_iter().next().map(|(_, key)| key)
    }

    pub(crate) fn has_action_override(&self, context: ViewContext, action: &Action) -> bool {
        self.overrides[context.index()].iter().any(|binding| {
            self.bindings(context)
                .get(binding)
                .and_then(|name| action_from_name(name))
                .as_ref()
                == Some(action)
                || shown_keybindings()
                    .bindings(context)
                    .get(binding)
                    .and_then(|name| action_from_name(name))
                    .as_ref()
                    == Some(action)
        })
    }

    pub(crate) fn all_bindings_for_context(&self, context: ViewContext) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .bindings(context)
            .iter()
            .map(|(binding, action)| (format_keybinding(binding), action_display_name(action)))
            .collect();
        entries.sort();
        entries
    }

    pub(crate) fn display_bindings_for_actions(
        &self,
        context: ViewContext,
        actions: &[&str],
    ) -> Vec<(String, String)> {
        actions
            .iter()
            .filter_map(|action| {
                let mut bindings: Vec<_> = self
                    .bindings(context)
                    .iter()
                    .filter(|(_, name)| name == action)
                    .map(|(key, _)| format_keybinding(key))
                    .collect();
                bindings.sort();
                (!bindings.is_empty()).then(|| (bindings.join("/"), action_display_name(action)))
            })
            .collect()
    }
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

impl KeyPress {
    pub(crate) fn from_event(key: ratatui::crossterm::event::KeyEvent) -> Self {
        let mut code = key.code;
        let mut modifiers = key.modifiers;
        if code == KeyCode::BackTab {
            code = KeyCode::Tab;
            modifiers.insert(KeyModifiers::SHIFT);
        }
        if let KeyCode::Char(c) = code {
            if c.is_ascii_alphabetic() && modifiers.contains(KeyModifiers::SHIFT) {
                code = KeyCode::Char(c.to_ascii_uppercase());
            }
            if matches!(code, KeyCode::Char(c) if c.is_uppercase()) {
                modifiers.insert(KeyModifiers::SHIFT);
            } else if !c.is_alphabetic() {
                modifiers.remove(KeyModifiers::SHIFT);
            }
        }
        Self { code, modifiers }
    }
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
    if s.is_empty() || s.chars().any(char::is_control) {
        return Err("Empty or control-character binding".into());
    }
    let mut rest = s;
    let mut modifiers = KeyModifiers::NONE;
    loop {
        let (prefix, flag) = if rest.starts_with("Ctrl-") {
            ("Ctrl-", KeyModifiers::CONTROL)
        } else if rest.starts_with("Alt-") {
            ("Alt-", KeyModifiers::ALT)
        } else if rest.starts_with("Shift-") {
            ("Shift-", KeyModifiers::SHIFT)
        } else {
            break;
        };
        if modifiers.contains(flag) {
            return Err("Duplicate modifier".into());
        }
        modifiers.insert(flag);
        rest = &rest[prefix.len()..];
    }
    let named = match rest {
        "Enter" => Some(KeyCode::Enter),
        "Escape" | "Esc" => Some(KeyCode::Esc),
        "Tab" => Some(KeyCode::Tab),
        "Up" => Some(KeyCode::Up),
        "Down" => Some(KeyCode::Down),
        "Left" => Some(KeyCode::Left),
        "Right" => Some(KeyCode::Right),
        "Home" => Some(KeyCode::Home),
        "End" => Some(KeyCode::End),
        "PageUp" => Some(KeyCode::PageUp),
        "PageDown" => Some(KeyCode::PageDown),
        "Backspace" => Some(KeyCode::Backspace),
        "Delete" => Some(KeyCode::Delete),
        "Space" => Some(KeyCode::Char(' ')),
        _ => None,
    };
    let keys = if let Some(code) = named {
        vec![KeyPress::from_event(
            ratatui::crossterm::event::KeyEvent::new(code, modifiers),
        )]
    } else {
        if rest.is_empty()
            || (!modifiers.is_empty() && rest.chars().count() != 1)
            || rest.contains('-') && rest != "-"
            || rest.chars().any(char::is_whitespace)
        {
            return Err("Expected a key or a concatenated character chord".into());
        }
        rest.chars()
            .map(|c| {
                KeyPress::from_event(ratatui::crossterm::event::KeyEvent::new(
                    KeyCode::Char(c),
                    modifiers,
                ))
            })
            .collect()
    };
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
        "thread_briefing" => Some(Action::OpenThreadBriefing),
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
        "pattern_select_all" => Some(Action::PatternSelect(PatternKind::All)),
        "pattern_select_none" => Some(Action::PatternSelect(PatternKind::None)),
        "pattern_select_read" => Some(Action::PatternSelect(PatternKind::Read)),
        "pattern_select_unread" => Some(Action::PatternSelect(PatternKind::Unread)),
        "pattern_select_starred" => Some(Action::PatternSelect(PatternKind::Starred)),
        "pattern_select_thread" => Some(Action::PatternSelect(PatternKind::Thread)),
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
        "toggle_assistant" => Some(Action::ToggleAssistant),
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
        "open_desk" => Some(Action::OpenDesk),
        "open_now" => Some(Action::OpenNow),
        "open_messages" => Some(Action::OpenMessages),
        "open_updates" => Some(Action::OpenUpdates),
        "open_archive" => Some(Action::OpenArchiveMode),
        "open_todo" => Some(Action::OpenTodo),
        "create_todo" => Some(Action::CreateTodoFromMessage),
        "open_reading" => Some(Action::OpenReading),
        "open_paper_trail" => Some(Action::OpenPlace(mxr_protocol::MailPlaceData::PaperTrail)),
        "go_all_mail" => Some(Action::GoToAllMail),
        "go_label" => Some(Action::GoToLabel),
        "edit_config" => Some(Action::EditConfig),
        "open_logs" => Some(Action::OpenLogs),
        "open_analytics" => Some(Action::OpenAnalyticsScreen),
        "open_reply_queue" => Some(Action::OpenReplyQueue),
        "open_owed_replies" => Some(Action::OpenOwedReplies),
        "open_calendar_invites" => Some(Action::OpenCalendarInvites),
        "open_subscriptions" => Some(Action::OpenSubscriptions),
        "open_screener" => Some(Action::OpenScreenerQueue),
        "sender_profile" => Some(Action::OpenSenderView),
        "back" => Some(Action::Back),
        "open_tab_1" => Some(Action::OpenTab1),
        "open_tab_2" => Some(Action::OpenTab2),
        "open_tab_3" => Some(Action::OpenTab3),
        "open_tab_4" => Some(Action::OpenTab4),
        "open_tab_5" => Some(Action::OpenTab5),
        "open_tab_6" => Some(Action::OpenTab6),
        "open_tab_7" => Some(Action::OpenTab7),
        "toggle_signature" => Some(Action::ToggleSignature),
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
            if kp.modifiers.contains(KeyModifiers::SHIFT)
                && !matches!(kp.code, KeyCode::Char(c) if c.is_uppercase())
            {
                s.push_str("Shift-");
            }
            match kp.code {
                KeyCode::Char(' ') => s.push_str("Space"),
                KeyCode::Char(c) => s.push(c),
                KeyCode::Enter => s.push_str("Enter"),
                KeyCode::Esc => s.push_str("Esc"),
                KeyCode::Tab => s.push_str("Tab"),
                KeyCode::Up => s.push_str("Up"),
                KeyCode::Down => s.push_str("Down"),
                KeyCode::Left => s.push_str("Left"),
                KeyCode::Right => s.push_str("Right"),
                KeyCode::Home => s.push_str("Home"),
                KeyCode::End => s.push_str("End"),
                KeyCode::PageUp => s.push_str("PageUp"),
                KeyCode::PageDown => s.push_str("PageDown"),
                KeyCode::Backspace => s.push_str("Backspace"),
                KeyCode::Delete => s.push_str("Delete"),
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
    shown_keybindings().primary_key_display(context, action)
}

pub fn display_bindings_for_actions(
    context: ViewContext,
    actions: &[&str],
) -> Vec<(String, String)> {
    shown_keybindings().display_bindings_for_actions(context, actions)
}

pub fn all_bindings_for_context(context: ViewContext) -> Vec<(String, String)> {
    shown_keybindings().all_bindings_for_context(context)
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
        "thread_briefing" => "Brief".into(),
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
        "pattern_select_all" => "Select All".into(),
        "pattern_select_none" => "Select None".into(),
        "pattern_select_read" => "Select Read".into(),
        "pattern_select_unread" => "Select Unread".into(),
        "pattern_select_starred" => "Select Starred".into(),
        "pattern_select_thread" => "Select Thread".into(),
        "go_inbox" => "Go Inbox".into(),
        "open_desk" => "Desk".into(),
        "open_now" => "Now".into(),
        "open_messages" => "Messages".into(),
        "open_updates" => "Updates".into(),
        "open_archive" => "Archive".into(),
        "open_todo" => "To do".into(),
        "create_todo" => "Make a to-do".into(),
        "open_reading" => "Reading".into(),
        "open_paper_trail" => "Updates".into(),
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
        "open_tab_7" => "Deliveries Page".into(),
        "toggle_assistant" => "Assistant".into(),
        "quit_view" => "Quit".into(),
        "clear_selection" => "Clear Sel".into(),
        "back" => "Back".into(),
        "sender_profile" => "Sender".into(),
        "open_analytics" => "Analytics".into(),
        "open_reply_queue" => "Reply Queue".into(),
        "open_owed_replies" => "Owed Replies".into(),
        "open_calendar_invites" => "Invites".into(),
        "open_screener" => "Screener".into(),
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

/// Everything in `keys.toml` that mxr can't use, as sentences: malformed
/// TOML, unknown sections or actions, keys a terminal can't send, and
/// sections the loader ignores because one key starts another.
pub fn check_keys_toml(text: &str) -> Vec<String> {
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => return vec![format!("keys.toml is not valid TOML: {error}")],
    };
    let mut problems = Vec::new();
    for section in ["mail_list", "message_view", "thread_view"] {
        let Some(value) = table.get(section) else {
            continue;
        };
        let Some(entries) = value.as_table() else {
            problems.push(format!("[{section}] must be a table of key = \"action\""));
            continue;
        };
        let mut parsed = Vec::new();
        for (key, name) in entries {
            let Some(name) = name.as_str() else {
                problems.push(format!("[{section}] `{key}`: the action must be a string"));
                continue;
            };
            let binding = match parse_key_string(key) {
                Ok(binding) => binding,
                Err(error) => {
                    problems.push(format!("[{section}] `{key}`: {error}"));
                    continue;
                }
            };
            if matches!(key.as_str(), "Ctrl-i" | "Ctrl-m" | "Ctrl-h" | "Ctrl-[")
                || key.starts_with("Ctrl-Shift-")
            {
                problems.push(format!(
                    "[{section}] `{key}`: terminals can't send this apart from another key"
                ));
            }
            if !name.is_empty() && action_from_name(name).is_none() {
                problems.push(format!("[{section}] `{key}`: no action {name}"));
                continue;
            }
            parsed.push((binding, name.is_empty()));
        }
        let ambiguous = parsed.iter().enumerate().any(|(i, (key, unbound))| {
            !unbound
                && parsed
                    .iter()
                    .enumerate()
                    .any(|(j, (other, other_unbound))| {
                        i != j && !other_unbound && other.keys.starts_with(&key.keys)
                    })
        });
        if ambiguous {
            problems.push(format!(
                "[{section}]: one key is the start of another, so mxr ignores the whole section"
            ));
        }
    }
    let (_, mode_problems) = mxr_keymap::ModeRemaps::from_table(&table, mxr_keymap::defaults());
    problems.extend(mode_problems);
    problems
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
    let text = std::fs::read_to_string(&keys_path).ok();
    let user_config = text
        .as_deref()
        .and_then(|s| toml::from_str::<KeysToml>(s).ok());

    let mut config = default_keybindings();
    if let Some(table) = text.as_deref().and_then(|s| s.parse::<toml::Table>().ok()) {
        let (modes, problems) = mxr_keymap::ModeRemaps::from_table(&table, mxr_keymap::defaults());
        for problem in problems {
            tracing::warn!(%problem, "Ignoring part of keys.toml; `mxr keys check` lists it");
        }
        config.modes = modes;
    }

    if let Some(user) = user_config {
        for (index, (map, user)) in [
            (&mut config.mail_list, user.mail_list),
            (&mut config.message_view, user.message_view),
            (&mut config.thread_view, user.thread_view),
        ]
        .into_iter()
        .enumerate()
        {
            let mut entries: Vec<_> = user
                .into_iter()
                .filter_map(|(key, name)| {
                    let binding = parse_key_string(&key).ok()?;
                    (name.is_empty() || action_from_name(&name).is_some())
                        .then_some((binding, name))
                })
                .collect();
            entries.sort_by_key(|(key, _)| format_keybinding(key));
            // Reject ambiguous user prefixes rather than choosing by HashMap order.
            if entries.iter().enumerate().any(|(i, (key, name))| {
                !name.is_empty()
                    && entries.iter().enumerate().any(|(j, (other, other_name))| {
                        i != j && !other_name.is_empty() && other.keys.starts_with(&key.keys)
                    })
            }) {
                tracing::warn!(context = index, "Ignoring ambiguous keybinding context");
                continue;
            }
            map.retain(|key, name| {
                // Scroll and next-message names deliberately share an Action,
                // but are distinct reader operations and must retain their keys.
                let retired = entries
                    .iter()
                    .any(|(_, user_name)| same_binding_action(name, user_name))
                    || entries.iter().any(|(user_key, _)| {
                        user_key == key
                            || user_key.keys.starts_with(&key.keys)
                            || key.keys.starts_with(&user_key.keys)
                    });
                if retired {
                    config.overrides[index].insert(key.clone());
                }
                !retired
            });
            for (key, name) in entries {
                config.overrides[index].insert(key.clone());
                if !name.is_empty() {
                    map.insert(key, name);
                }
            }
        }
    }

    config
}

fn same_binding_action(left: &str, right: &str) -> bool {
    if matches!(left, "move_down" | "scroll_down") && matches!(right, "move_down" | "scroll_down")
        || matches!(left, "move_up" | "scroll_up") && matches!(right, "move_up" | "scroll_up")
    {
        return true;
    }
    if matches!(
        left,
        "move_down" | "scroll_down" | "next_message" | "move_up" | "scroll_up" | "prev_message"
    ) || matches!(
        right,
        "move_down" | "scroll_down" | "next_message" | "move_up" | "scroll_up" | "prev_message"
    ) {
        left == right
    } else {
        action_from_name(left).is_some() && action_from_name(left) == action_from_name(right)
    }
}

// Mail list defaults — Gmail-native scheme (A005). Module-level so tests
// can assert the table has no duplicate keys before HashMap insertion
// silently drops one of the colliding entries.
const ML_DEFAULTS: &[(&str, &str)] = &[
    // Navigation (vim-native)
    ("j", "move_down"),
    ("k", "move_up"),
    ("gg", "jump_top"),
    ("G", "jump_bottom"),
    ("Ctrl-d", "page_down"),
    ("Ctrl-u", "page_up"),
    ("L", "open_links"),
    ("/", "search_all_mail"),
    ("Ctrl-f", "mailbox_filter"),
    ("n", "next_search_result"),
    ("N", "prev_search_result"),
    ("Enter", "open"),
    ("o", "open"),
    ("q", "quit_view"),
    ("?", "help"),
    ("Escape", "back"),
    // Email actions (Gmail-native A005)
    ("c", "compose"),
    ("r", "reply"),
    ("a", "reply_all"),
    ("f", "forward"),
    ("B", "thread_briefing"),
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
    // Pattern select (`*` then a letter), Gmail-style
    ("*a", "pattern_select_all"),
    ("*n", "pattern_select_none"),
    ("*r", "pattern_select_read"),
    ("*u", "pattern_select_unread"),
    ("*s", "pattern_select_starred"),
    ("*t", "pattern_select_thread"),
    // mxr-specific
    ("D", "unsubscribe"),
    ("b", "flag_reply_later"),
    ("t", "create_todo"),
    ("Z", "snooze"),
    ("O", "open_in_browser"),
    ("R", "toggle_reader_mode"),
    ("S", "toggle_signature"),
    ("E", "export_thread"),
    ("p", "sender_profile"),
    ("V", "visual_line_mode"),
    (":", "command_palette"),
    ("Ctrl-k", "command_palette"),
    ("Ctrl-p", "command_palette"),
    ("@", "toggle_assistant"),
    ("Tab", "switch_panes"),
    ("F", "toggle_fullscreen"),
    // Gmail go-to (A005)
    ("gi", "go_inbox"),
    ("gs", "go_starred"),
    ("gt", "go_sent"),
    ("gd", "go_drafts"),
    ("gh", "open_now"),
    ("gm", "open_messages"),
    ("ge", "open_archive"),
    ("gx", "open_todo"),
    ("gr", "open_reading"),
    ("ga", "go_all_mail"),
    ("gl", "go_label"),
    ("gL", "open_logs"),
    // Draft assist, draft for sender, commitments and voice profile are
    // palette only. g + a capital opens a tool (blueprint 35, D166).
    ("gA", "open_analytics"),
    ("gR", "open_tab_3"),
    ("gC", "open_tab_4"),
    ("gD", "open_tab_5"),
    ("gP", "open_tab_7"),
    ("gY", "open_activity"),
    ("gq", "open_reply_queue"),
    ("go", "open_owed_replies"),
    ("gv", "open_calendar_invites"),
    ("gu", "open_updates"),
    ("gS", "open_screener"),
];

// Message view defaults
const MV_DEFAULTS: &[(&str, &str)] = &[
    ("Ctrl-d", "page_down"),
    ("Ctrl-u", "page_up"),
    ("G", "jump_bottom"),
    ("gg", "jump_top"),
    ("/", "search_all_mail"),
    ("Ctrl-f", "mailbox_filter"),
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
    ("B", "thread_briefing"),
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
    ("t", "create_todo"),
    ("p", "sender_profile"),
    ("S", "toggle_signature"),
    ("gR", "open_tab_3"),
    ("gC", "open_tab_4"),
    ("gD", "open_tab_5"),
    ("gL", "open_logs"),
    ("gA", "open_analytics"),
];

// Thread view defaults
const TV_DEFAULTS: &[(&str, &str)] = &[
    ("Ctrl-d", "page_down"),
    ("Ctrl-u", "page_up"),
    ("G", "jump_bottom"),
    ("gg", "jump_top"),
    ("/", "search_all_mail"),
    ("Ctrl-f", "mailbox_filter"),
    ("j", "scroll_down"),
    ("k", "scroll_up"),
    ("J", "next_message"),
    ("K", "prev_message"),
    ("F", "toggle_fullscreen"),
    ("r", "reply"),
    ("a", "reply_all"),
    ("f", "forward"),
    ("B", "thread_briefing"),
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
    ("p", "sender_profile"),
    ("S", "toggle_signature"),
    ("gR", "open_tab_3"),
    ("gC", "open_tab_4"),
    ("gD", "open_tab_5"),
    ("gL", "open_logs"),
    ("gA", "open_analytics"),
];

/// The default tables, built once. Help, the hint bar and every palette
/// command read them, so rebuilding per lookup parsed every key string
/// over a hundred times each time the palette was created.
pub(crate) fn shown_keybindings() -> &'static KeybindingConfig {
    static SHOWN: std::sync::LazyLock<KeybindingConfig> =
        std::sync::LazyLock::new(default_keybindings);
    &SHOWN
}

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
        overrides: Default::default(),
        modes: mxr_keymap::ModeRemaps::default(),
        mail_list,
        message_view,
        thread_view,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn check_keys_toml_names_what_mxr_would_ignore() {
        let problems = check_keys_toml(
            "[mail_list]\n\"Ctrl-i\" = \"archive\"\n\"q\" = \"no_such_action\"\n\"g\" = \"archive\"\n\"gx\" = \"trash\"\n[todo]\n\"x\" = \"todo.nope\"\n",
        )
        .join("\n");
        for expected in [
            "`Ctrl-i`: terminals can't send this",
            "no action no_such_action",
            "mxr ignores the whole section",
            "no action todo.nope",
        ] {
            assert!(
                problems.contains(expected),
                "missing {expected:?} in:\n{problems}"
            );
        }
        assert_eq!(
            check_keys_toml("[todo]\n\"x\" = \"todo.done\"\n"),
            Vec::<String>::new()
        );
        assert!(check_keys_toml("[todo").join("").contains("not valid TOML"));
    }

    use super::*;

    #[test]
    fn remap_retires_old_destructive_binding() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keys.toml"),
            "[mail_list]\n\";\" = \"trash\"\n",
        )
        .unwrap();
        let config = load_keybindings(dir.path());
        assert_eq!(
            resolve_action(
                &config,
                ViewContext::MailList,
                &parse_key_string(";").unwrap().keys
            ),
            Some("trash".into())
        );
        assert_eq!(
            resolve_action(
                &config,
                ViewContext::MailList,
                &parse_key_string("#").unwrap().keys
            ),
            None
        );
    }

    #[test]
    fn malformed_unknown_and_ambiguous_overrides_keep_safe_defaults() {
        for source in [
            "[mail_list]\n\"#\" = 42\n",
            "[mail_list]\n\"#\" = \"__proto__\"\n",
            "[mail_list]\n\"Ctrl-ab\" = \"trash\"\n",
            "[mail_list]\n\"\" = \"trash\"\n",
            "[mail_list]\n\"w\" = \"trash\"\n\"wq\" = \"archive\"\n",
            "this is not toml",
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("keys.toml"), source).unwrap();
            let config = load_keybindings(dir.path());
            assert_eq!(
                resolve_action(
                    &config,
                    ViewContext::MailList,
                    &parse_key_string("#").unwrap().keys
                ),
                Some("trash".into()),
                "{source}"
            );
            assert_eq!(
                config.mail_list,
                default_keybindings().mail_list,
                "{source}"
            );
        }
    }

    #[test]
    fn remap_swaps_are_order_independent_and_aliases_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keys.toml"), "[mail_list]\n\"j\" = \"move_up\"\n\"k\" = \"move_down\"\n\";\" = \"archive\"\n\"e\" = \"archive\"\n").unwrap();
        let config = load_keybindings(dir.path());
        for (key, action) in [
            ("j", "move_up"),
            ("k", "move_down"),
            (";", "archive"),
            ("e", "archive"),
        ] {
            assert_eq!(
                resolve_action(
                    &config,
                    ViewContext::MailList,
                    &parse_key_string(key).unwrap().keys
                ),
                Some(action.into())
            );
        }
    }

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
    fn shifted_tab_and_space_use_terminal_key_codes_and_readable_labels() {
        let tab = parse_key_string("Shift-Tab").unwrap();
        assert_eq!(
            tab.keys,
            vec![KeyPress::from_event(
                ratatui::crossterm::event::KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
            )]
        );
        assert_eq!(format_keybinding(&tab), "Shift-Tab");
        assert_eq!(
            format_keybinding(&parse_key_string("Space").unwrap()),
            "Space"
        );
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
        assert!(actions.contains(&"sender_profile"));
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
        assert!(action_from_name("pattern_select_unread").is_some());
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
        assert!(labels.contains(&"Accounts Page".to_string()));
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
        // The HTML toggle exists only in message/thread views.
        assert_eq!(
            primary_key_display(ViewContext::MailList, &Action::ToggleHtmlView),
            None
        );
        assert_eq!(
            primary_key_display(ViewContext::ThreadView, &Action::ToggleHtmlView),
            Some("H".to_string())
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
