use crate::action::UiContext;
use crate::keybindings::{display_bindings_for_actions, ViewContext};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

/// Maximum hints surfaced in the bar at once. Past this, the user is
/// better served by Cmd+K (palette) or `?` (help). Five fits a single
/// 80-col line and matches the "top-5 contextual" design intent.
pub const HINT_BAR_MAX_HINTS: usize = 5;

pub struct HintBarState<'a> {
    pub ui_context: UiContext,
    pub search_active: bool,
    pub help_modal_open: bool,
    pub selected_count: usize,
    pub bulk_confirm_open: bool,
    pub sync_status: Option<String>,
    /// True when the focused message carries a calendar invite that the
    /// user can RSVP to. Surfaces accept/decline keys in the hint bar so
    /// the action is discoverable without opening the palette.
    pub viewing_invite: bool,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: HintBarState<'_>, theme: &Theme) {
    let lines = if state.bulk_confirm_open {
        vec![hint_line(
            &[("Enter", "Confirm"), ("y", "Confirm"), ("Esc", "Cancel")],
            theme,
        )]
    } else if state.help_modal_open {
        vec![hint_line(
            &[
                ("Type", "Search"),
                ("j/k ↑↓", "Navigate"),
                ("Ctrl-u/d", "Page"),
                ("Esc", "Close"),
            ],
            theme,
        )]
    } else if state.search_active && matches!(state.ui_context, UiContext::MailboxList) {
        vec![hint_line(
            &[
                ("Enter", "Apply Filter"),
                ("Esc", "Clear Filter"),
                ("Ctrl-f", "Mailbox Filter"),
            ],
            theme,
        )]
    } else {
        build_lines(
            &hints_for_context(state.ui_context, state.selected_count, state.viewing_invite),
            theme,
        )
    };

    // Split area: hints on left, sync status on right
    let sync_text = state.sync_status.as_deref().unwrap_or("not synced");
    let sync_color = if sync_text == "syncing" {
        theme.accent
    } else if sync_text.starts_with("synced") {
        theme.success
    } else if sync_text == "degraded" {
        theme.warning
    } else {
        theme.error
    };
    let sync_width = (sync_text.len() + 4) as u16; // "● " + text + padding

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(sync_width)])
        .split(area);

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.hint_bar_bg)),
        chunks[0],
    );

    // Sync status indicator
    let sync_line = Line::from(vec![
        Span::styled("● ", Style::default().fg(sync_color)),
        Span::styled(sync_text.to_string(), Style::default().fg(theme.text_muted)),
    ]);
    frame.render_widget(
        Paragraph::new(sync_line).style(Style::default().bg(theme.hint_bar_bg)),
        chunks[1],
    );
}

/// Hints for the bar in this context. Capped at [`HINT_BAR_MAX_HINTS`];
/// less-frequent actions live in Cmd+K palette or the `?` help modal.
/// Action lists below are ordered by user-task primacy: open the
/// dominant action first, then escape/help last.
pub fn hints_for_context(
    context: UiContext,
    selected_count: usize,
    viewing_invite: bool,
) -> Vec<(String, String)> {
    let mut hints = match context {
        UiContext::MailboxSidebar => display_bindings_for_actions(
            ViewContext::MailList,
            &["open", "search_all_mail", "command_palette", "help"],
        ),
        UiContext::MailboxList if selected_count > 0 => {
            let mut hints = display_bindings_for_actions(
                ViewContext::MailList,
                &["archive", "mark_read_archive", "trash", "apply_label"],
            );
            hints.insert(0, ("Esc".to_string(), "Clear Sel".to_string()));
            hints
        }
        UiContext::MailboxList => display_bindings_for_actions(
            ViewContext::MailList,
            &[
                "open",
                "reply",
                "archive",
                "search_all_mail",
                "command_palette",
            ],
        ),
        UiContext::MailboxMessage if viewing_invite => {
            // Calendar invite is open — surface RSVP keys first so the
            // action is discoverable without the palette.
            let mut hints = display_bindings_for_actions(
                ViewContext::ThreadView,
                &[
                    "invite_accept",
                    "invite_tentative",
                    "invite_decline",
                    "reply",
                    "archive",
                ],
            );
            // Fall back to defaults if any binding is missing in user config.
            if hints.is_empty() {
                hints = display_bindings_for_actions(
                    ViewContext::ThreadView,
                    &["reply", "summarize_current_thread", "archive"],
                );
            }
            hints
        }
        UiContext::MailboxMessage => display_bindings_for_actions(
            ViewContext::ThreadView,
            &[
                "reply",
                "reply_all",
                "summarize_current_thread",
                "archive",
                "open_links",
            ],
        ),
        UiContext::SearchEditor => vec![
            ("Enter".to_string(), "Run Now".to_string()),
            ("Tab".to_string(), "Mode".to_string()),
            ("Esc".to_string(), "Stop Editing".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::SearchResults => vec![
            ("Enter/o".to_string(), "Open".to_string()),
            ("/".to_string(), "Edit Query".to_string()),
            ("Tab".to_string(), "Switch Pane".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::SearchPreview => {
            let mut hints = display_bindings_for_actions(
                ViewContext::ThreadView,
                &["reply", "archive", "open_links"],
            );
            hints.insert(0, ("/".to_string(), "Edit Query".to_string()));
            hints.push(("?".to_string(), "Help".to_string()));
            hints
        }
        UiContext::RulesList => vec![
            ("n".to_string(), "New Rule".to_string()),
            ("Enter".to_string(), "Refresh".to_string()),
            ("E".to_string(), "Edit Rule".to_string()),
            ("D".to_string(), "Dry Run".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::RulesForm => vec![
            ("Tab".to_string(), "Next Field".to_string()),
            ("Ctrl-s".to_string(), "Save".to_string()),
            ("Esc".to_string(), "Close Form".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::Diagnostics => vec![
            ("Enter/o".to_string(), "Full".to_string()),
            ("r".to_string(), "Refresh".to_string()),
            ("c".to_string(), "Config".to_string()),
            ("L".to_string(), "Logs".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::AccountsList => vec![
            ("n".to_string(), "New".to_string()),
            ("Enter".to_string(), "Edit".to_string()),
            ("t".to_string(), "Test".to_string()),
            ("d".to_string(), "Default".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::AccountsForm => vec![
            ("Tab".to_string(), "Next Field".to_string()),
            ("Enter/i".to_string(), "Edit".to_string()),
            ("s".to_string(), "Save".to_string()),
            ("Esc".to_string(), "Close".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::Analytics => vec![
            ("Tab".to_string(), "Next View".to_string()),
            ("j/k".to_string(), "Move Row".to_string()),
            ("r".to_string(), "Refresh".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
    };
    hints.truncate(HINT_BAR_MAX_HINTS);
    hints
}

fn build_lines(hints: &[(String, String)], theme: &Theme) -> Vec<Line<'static>> {
    let mid = hints.len().div_ceil(2);
    vec![
        hint_line_owned(&hints[..mid], theme),
        hint_line_owned(&hints[mid..], theme),
    ]
}

fn hint_line(hints: &[(&str, &str)], theme: &Theme) -> Line<'static> {
    Line::from(
        hints
            .iter()
            .flat_map(|(key, action)| {
                [
                    Span::styled(
                        format!(" {key}"),
                        Style::default().fg(theme.text_primary).bold(),
                    ),
                    Span::styled(
                        format!(":{action}  "),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]
            })
            .collect::<Vec<_>>(),
    )
}

fn hint_line_owned(hints: &[(String, String)], theme: &Theme) -> Line<'static> {
    Line::from(
        hints
            .iter()
            .flat_map(|(key, action)| {
                [
                    Span::styled(
                        format!(" {key}"),
                        Style::default().fg(theme.text_primary).bold(),
                    ),
                    Span::styled(
                        format!(":{action}  "),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::{hints_for_context, HINT_BAR_MAX_HINTS};
    use crate::action::UiContext;

    /// The hint bar promises a top-N contextual surface; deeper
    /// commands live in Cmd+K and `?`. Test that every context keeps
    /// the bar under the documented cap so the design stays honest.
    #[test]
    fn no_context_exceeds_hint_bar_cap() {
        let contexts = [
            (UiContext::MailboxSidebar, 0usize),
            (UiContext::MailboxList, 0),
            (UiContext::MailboxList, 3),
            (UiContext::MailboxMessage, 0),
            (UiContext::SearchEditor, 0),
            (UiContext::SearchResults, 0),
            (UiContext::SearchPreview, 0),
            (UiContext::RulesList, 0),
            (UiContext::RulesForm, 0),
            (UiContext::Diagnostics, 0),
            (UiContext::AccountsList, 0),
            (UiContext::AccountsForm, 0),
            (UiContext::Analytics, 0),
        ];
        for (context, selected) in contexts {
            let hints = hints_for_context(context, selected, false);
            assert!(
                hints.len() <= HINT_BAR_MAX_HINTS,
                "context {context:?} (selected={selected}) yielded {} hints, cap is {HINT_BAR_MAX_HINTS}",
                hints.len(),
            );
        }
    }

    #[test]
    fn selected_mailbox_hints_lead_with_clear_and_keep_archive() {
        let hints = hints_for_context(UiContext::MailboxList, 3, false);
        let labels: Vec<String> = hints.into_iter().map(|(_, label)| label).collect();
        assert_eq!(
            labels.first().map(String::as_str),
            Some("Clear Sel"),
            "selected mailbox must surface the clear-selection escape first"
        );
        assert!(
            labels.contains(&"Archive".to_string()),
            "Archive must remain in the slim bar; got {labels:?}",
        );
    }

    #[test]
    fn message_view_hints_lead_with_reply() {
        let hints = hints_for_context(UiContext::MailboxMessage, 0, false);
        let labels: Vec<String> = hints.into_iter().map(|(_, label)| label).collect();
        assert_eq!(
            labels.first().map(String::as_str),
            Some("Reply"),
            "message view must surface Reply first; got {labels:?}",
        );
    }

    #[test]
    fn invite_message_view_surfaces_rsvp_keys() {
        let hints = hints_for_context(UiContext::MailboxMessage, 0, true);
        let labels: Vec<String> = hints.into_iter().map(|(_, label)| label).collect();
        assert!(
            labels.iter().any(|label| label == "Accept Invite"),
            "invite view must surface Accept; got {labels:?}",
        );
        assert!(
            labels.iter().any(|label| label == "Decline Invite"),
            "invite view must surface Decline; got {labels:?}",
        );
    }
}
