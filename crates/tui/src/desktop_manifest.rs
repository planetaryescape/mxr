use crate::action::Action;
use crate::keybindings::{
    action_from_name, all_bindings_for_context, default_keybindings, format_keybinding, ViewContext,
};
use crate::ui::command_palette::default_commands;
use ratatui::crossterm::event::KeyCode;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopManifest {
    pub bindings: DesktopBindings,
    pub commands: Vec<DesktopCommand>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopBindings {
    pub mail_list: Vec<DesktopBinding>,
    pub message_view: Vec<DesktopBinding>,
    pub thread_view: Vec<DesktopBinding>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopBinding {
    pub action: String,
    pub label: String,
    pub display: String,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCommand {
    pub label: String,
    pub shortcut: String,
    pub action: String,
    pub category: String,
}

pub fn desktop_manifest() -> DesktopManifest {
    let config = default_keybindings();

    DesktopManifest {
        bindings: DesktopBindings {
            mail_list: bindings_for_context(&config.mail_list, ViewContext::MailList),
            message_view: bindings_for_context(&config.message_view, ViewContext::MessageView),
            thread_view: bindings_for_context(&config.thread_view, ViewContext::ThreadView),
        },
        commands: default_commands()
            .into_iter()
            .filter_map(|command| {
                action_name(&command.action).map(|action| DesktopCommand {
                    label: command.label,
                    shortcut: command.shortcut,
                    action: action.to_string(),
                    category: command.category,
                })
            })
            .collect(),
    }
}

fn bindings_for_context(
    bindings: &std::collections::HashMap<crate::keybindings::KeyBinding, String>,
    context: ViewContext,
) -> Vec<DesktopBinding> {
    let display_names = all_bindings_for_context(context)
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();

    let mut entries = bindings
        .iter()
        .filter_map(|(binding, action)| {
            action_from_name(action)?;
            Some(DesktopBinding {
                action: action.clone(),
                label: display_names
                    .get(&format_keybinding(binding))
                    .cloned()
                    .unwrap_or_else(|| action.clone()),
                display: format_keybinding(binding),
                tokens: binding.keys.iter().map(key_press_token).collect(),
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left.display
            .cmp(&right.display)
            .then_with(|| left.action.cmp(&right.action))
    });
    entries
}

fn key_press_token(key: &crate::keybindings::KeyPress) -> String {
    if key
        .modifiers
        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
    {
        if let KeyCode::Char(c) = key.code {
            return format!("Ctrl-{}", c.to_ascii_lowercase());
        }
    }

    match key.code {
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Char(c) => c.to_string(),
        _ => "?".to_string(),
    }
}

fn action_name(action: &Action) -> Option<&'static str> {
    match action {
        Action::MoveDown => Some("move_down"),
        Action::MoveUp => Some("move_up"),
        Action::JumpTop => Some("jump_top"),
        Action::JumpBottom => Some("jump_bottom"),
        Action::PageDown => Some("page_down"),
        Action::PageUp => Some("page_up"),
        Action::ViewportTop => Some("visible_top"),
        Action::ViewportMiddle => Some("visible_middle"),
        Action::ViewportBottom => Some("visible_bottom"),
        Action::CenterCurrent => Some("center_current"),
        Action::SwitchPane => Some("switch_panes"),
        Action::OpenSelected => Some("open"),
        Action::Back => Some("back"),
        Action::QuitView => Some("quit_view"),
        Action::ClearSelection => Some("clear_selection"),
        Action::OpenMailboxScreen => Some("open_mailbox_screen"),
        Action::OpenSearchScreen => Some("open_search_screen"),
        Action::OpenRulesScreen => Some("open_rules_screen"),
        Action::OpenDiagnosticsScreen => Some("open_diagnostics_screen"),
        Action::OpenAccountsScreen => Some("open_accounts_screen"),
        Action::OpenTab1 => Some("open_tab_1"),
        Action::OpenTab2 => Some("open_tab_2"),
        Action::OpenTab3 => Some("open_tab_3"),
        Action::OpenTab4 => Some("open_tab_4"),
        Action::OpenTab5 => Some("open_tab_5"),
        Action::OpenGlobalSearch => Some("search"),
        Action::OpenMailboxFilter => Some("mailbox_filter"),
        Action::SubmitSearch => Some("submit_search"),
        Action::CloseSearch => Some("close_search"),
        Action::CycleSearchMode => Some("cycle_search_mode"),
        Action::NextSearchResult => Some("next_search_result"),
        Action::PrevSearchResult => Some("prev_search_result"),
        Action::GoToInbox => Some("go_inbox"),
        Action::GoToStarred => Some("go_starred"),
        Action::GoToSent => Some("go_sent"),
        Action::GoToDrafts => Some("go_drafts"),
        Action::GoToAllMail => Some("go_all_mail"),
        Action::OpenSubscriptions => Some("open_subscriptions"),
        Action::GoToLabel => Some("go_label"),
        Action::OpenCommandPalette => Some("command_palette"),
        Action::CloseCommandPalette => Some("close_command_palette"),
        Action::SyncNow => Some("sync"),
        Action::Compose => Some("compose"),
        Action::Reply => Some("reply"),
        Action::ReplyAll => Some("reply_all"),
        Action::Forward => Some("forward"),
        Action::Archive => Some("archive"),
        Action::MarkReadAndArchive => Some("mark_read_archive"),
        Action::Trash => Some("trash"),
        Action::Spam => Some("spam"),
        Action::Star => Some("star"),
        Action::MarkRead => Some("mark_read"),
        Action::MarkUnread => Some("mark_unread"),
        Action::ApplyLabel => Some("apply_label"),
        Action::MoveToLabel => Some("move_to_label"),
        Action::Unsubscribe => Some("unsubscribe"),
        Action::Snooze => Some("snooze"),
        Action::OpenInBrowser => Some("open_in_browser"),
        Action::ToggleReaderMode => Some("toggle_reader_mode"),
        Action::ToggleHtmlView => Some("toggle_html_view"),
        Action::ToggleRemoteContent => Some("toggle_remote_content"),
        Action::ToggleSignature => Some("toggle_signature"),
        Action::ToggleSelect => Some("toggle_select"),
        Action::VisualLineMode => Some("visual_line_mode"),
        Action::AttachmentList => Some("attachment_list"),
        Action::OpenLinks => Some("open_links"),
        Action::ToggleFullscreen => Some("toggle_fullscreen"),
        Action::ExportThread => Some("export_thread"),
        Action::Help => Some("help"),
        Action::ToggleMailListMode => Some("toggle_mail_list_mode"),
        Action::RefreshRules => Some("refresh_rules"),
        Action::RefreshDiagnostics => Some("refresh_diagnostics"),
        Action::RefreshAccounts => Some("refresh_accounts"),
        Action::GenerateBugReport => Some("generate_bug_report"),
        Action::EditConfig => Some("edit_config"),
        Action::OpenLogs => Some("open_logs"),
        Action::ShowOnboarding => Some("show_onboarding"),
        Action::OpenDiagnosticsPaneDetails => Some("open_diagnostics_pane_details"),
        Action::OpenRuleFormNew => Some("open_rule_form_new"),
        Action::OpenRuleFormEdit => Some("open_rule_form_edit"),
        Action::ToggleRuleEnabled => Some("toggle_rule_enabled"),
        Action::ShowRuleDryRun => Some("show_rule_dry_run"),
        Action::ShowRuleHistory => Some("show_rule_history"),
        Action::DeleteRule => Some("delete_rule"),
        Action::OpenAccountFormNew => Some("open_account_form_new"),
        Action::TestAccountForm => Some("test_account_form"),
        Action::SetDefaultAccount => Some("set_default_account"),
        Action::Noop => Some("noop"),
        Action::OpenMessageView
        | Action::CloseMessageView
        | Action::SelectLabel(_)
        | Action::SelectSavedSearch(_, _)
        | Action::ClearFilter
        | Action::SaveRuleForm
        | Action::SaveAccountForm
        | Action::ReauthorizeAccountForm
        | Action::ConfirmUnsubscribeOnly
        | Action::ConfirmUnsubscribeAndArchiveSender
        | Action::CancelUnsubscribe
        | Action::PatternSelect(_)
        | Action::SwitchAccount(_) => None,
    }
}
