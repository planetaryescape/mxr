use crate::action::UiContext;
use crate::keybindings::{all_bindings_for_context, ViewContext};
use crate::ui::command_palette::commands_for_context;
use nucleo::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32Str};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct HelpSection {
    title: String,
    entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HelpRow {
    section: String,
    shortcut: String,
    label: String,
    search_text: String,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HelpSearchResult {
    row: HelpRow,
    score: u32,
    priority: u8,
}

pub struct HelpModalState<'a> {
    pub open: bool,
    pub ui_context: UiContext,
    pub selected_count: usize,
    pub scroll_offset: u16,
    pub query: &'a str,
    pub selected: usize,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: HelpModalState<'_>, theme: &crate::theme::Theme) {
    if !state.open {
        return;
    }

    let popup = centered_rect(88, 88, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Help ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if state.query.is_empty() {
        draw_grouped_help(frame, inner, &state, theme);
    } else {
        draw_search_results(frame, inner, &state, theme);
    }
}

fn help_sections(state: &HelpModalState<'_>) -> Vec<HelpSection> {
    let mut sections = vec![
        HelpSection {
            title: "Start Here".into(),
            entries: vec![
                ("Ctrl-p".into(), "Open command palette".into()),
                (
                    "palette: Start Here".into(),
                    "Reopen onboarding walkthrough".into(),
                ),
                ("gc".into(), "Edit config in $EDITOR".into()),
                ("?".into(), "Toggle Help".into()),
                ("Esc".into(), "Back / Close".into()),
                ("q".into(), "Quit".into()),
            ],
        },
        HelpSection {
            title: "Most Common Actions".into(),
            entries: context_entries(state),
        },
    ];

    sections.extend(screen_sections(state.ui_context));
    sections.push(HelpSection {
        title: "Modals".into(),
        entries: vec![
            ("Help: j/k Ctrl-d/u".into(), "Scroll".into()),
            ("Label Picker".into(), "Type, j/k, Enter, Esc".into()),
            ("Compose Picker".into(), "Type, Tab, Enter, Esc".into()),
            ("Attachments".into(), "j/k, Enter/o, d, Esc".into()),
            ("Links".into(), "j/k, Enter/o open, y copy, Esc".into()),
            (
                "Unsubscribe".into(),
                "Enter unsubscribe, a archive sender, Esc cancel".into(),
            ),
            (
                "Bulk Confirm".into(),
                "Enter/y confirm, Esc/n cancel".into(),
            ),
        ],
    });
    sections.extend(command_sections(state.ui_context));
    sections
}

fn context_entries(state: &HelpModalState<'_>) -> Vec<(String, String)> {
    let mut entries = match state.ui_context {
        UiContext::MailboxSidebar | UiContext::MailboxList | UiContext::MailboxMessage => vec![
            ("/".into(), "Search all indexed mail".into()),
            ("Ctrl-f".into(), "Filter current mailbox only".into()),
            ("Ctrl-p".into(), "Open command palette".into()),
        ],
        UiContext::SearchEditor | UiContext::SearchResults | UiContext::SearchPreview => vec![
            ("Enter".into(), "Run search now".into()),
            ("Tab".into(), "Switch results and preview".into()),
            ("Esc".into(), "Preview -> results -> mailbox".into()),
        ],
        UiContext::RulesList | UiContext::RulesForm => vec![
            ("n".into(), "Start a new rule".into()),
            ("D".into(), "Dry run before save".into()),
            ("E".into(), "Edit selected rule".into()),
        ],
        UiContext::Diagnostics => vec![
            ("r".into(), "Refresh diagnostics".into()),
            ("c".into(), "Edit config".into()),
            ("L".into(), "Open log file".into()),
        ],
        UiContext::AccountsList | UiContext::AccountsForm => vec![
            ("n".into(), "Add an account".into()),
            ("t".into(), "Test selected account".into()),
            ("c".into(), "Edit config".into()),
        ],
    };

    entries.insert(0, ("Context".into(), state.ui_context.label().into()));

    if state.selected_count > 0 {
        entries.push((
            "Selection".into(),
            format!(
                "{} selected: archive, delete, label, move, read/unread, star, Esc clears",
                state.selected_count
            ),
        ));
    }

    if matches!(
        state.ui_context,
        UiContext::SearchEditor | UiContext::SearchResults | UiContext::SearchPreview
    ) {
        entries.push((
            "Search".into(),
            "Search tab hits the full local index; Ctrl-f only filters the current mailbox".into(),
        ));
    }

    entries
}

fn screen_sections(context: UiContext) -> Vec<HelpSection> {
    match context {
        UiContext::MailboxSidebar => vec![HelpSection {
            title: "Mailbox Sidebar".into(),
            entries: all_bindings_for_context(ViewContext::MailList),
        }],
        UiContext::MailboxList => vec![HelpSection {
            title: "Mailbox List".into(),
            entries: all_bindings_for_context(ViewContext::MailList),
        }],
        UiContext::MailboxMessage => vec![HelpSection {
            title: "Mailbox Message".into(),
            entries: all_bindings_for_context(ViewContext::ThreadView),
        }],
        UiContext::SearchEditor => {
            vec![HelpSection {
                title: "Search Page".into(),
                entries: vec![
                    ("Enter".into(), "Run search now".into()),
                    (
                        "Tab".into(),
                        "Cycle lexical / hybrid / semantic mode".into(),
                    ),
                    ("Esc".into(), "Stop editing".into()),
                ],
            }]
        }
        UiContext::SearchResults => {
            vec![HelpSection {
                title: "Search Page".into(),
                entries: vec![
                    ("/".into(), "Edit query".into()),
                    ("o / Enter / →".into(), "Open selected message".into()),
                    ("x".into(), "Select result".into()),
                    ("Tab".into(), "Switch results and preview".into()),
                    (
                        "j / k".into(),
                        "Move result cursor / update open message".into(),
                    ),
                    ("Esc".into(), "Return to mailbox".into()),
                ],
            }]
        }
        UiContext::SearchPreview => vec![HelpSection {
            title: "Search Preview".into(),
            entries: vec![
                ("j / k".into(), "Move through messages in the thread".into()),
                ("h / Esc".into(), "Return focus to results".into()),
                ("/".into(), "Edit query".into()),
                ("x".into(), "Select current message".into()),
                ("Tab".into(), "Switch results and preview".into()),
                ("A".into(), "Open attachments".into()),
                ("L".into(), "Open links".into()),
                ("R".into(), "Toggle reading view".into()),
                ("H".into(), "Toggle original HTML".into()),
                ("M".into(), "Toggle remote images".into()),
                ("O".into(), "Open in browser".into()),
                (
                    "r / a / f / e".into(),
                    "Reply, reply all, forward, archive".into(),
                ),
            ],
        }],
        UiContext::RulesList | UiContext::RulesForm => vec![HelpSection {
            title: "Rules Page".into(),
            entries: vec![
                ("j / k".into(), "Move rules".into()),
                ("Enter".into(), "Refresh overview".into()),
                ("n".into(), "New rule".into()),
                ("E".into(), "Edit rule".into()),
                ("D".into(), "Dry run".into()),
                ("H".into(), "History".into()),
                ("Ctrl-s".into(), "Save rule form".into()),
            ],
        }],
        UiContext::Diagnostics => vec![HelpSection {
            title: "Diagnostics Page".into(),
            entries: vec![
                ("j / k".into(), "Change section".into()),
                ("Ctrl-d / Ctrl-u".into(), "Scroll details".into()),
                ("Enter / o".into(), "Toggle fullscreen".into()),
                ("d".into(), "Open selected section details".into()),
                ("r".into(), "Refresh".into()),
                ("c".into(), "Edit config".into()),
                ("b".into(), "Generate bug report".into()),
                ("L".into(), "Open logs".into()),
            ],
        }],
        UiContext::AccountsList | UiContext::AccountsForm => vec![HelpSection {
            title: "Accounts Page".into(),
            entries: vec![
                ("j / k".into(), "Move accounts or fields".into()),
                ("Enter".into(), "Edit selected account".into()),
                ("n".into(), "New account".into()),
                ("t".into(), "Test account".into()),
                ("d".into(), "Set default".into()),
                ("c".into(), "Edit config".into()),
                ("s".into(), "Save account form".into()),
            ],
        }],
    }
}

fn command_sections(context: UiContext) -> Vec<HelpSection> {
    let mut by_category: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for command in commands_for_context(context) {
        let shortcut = if command.shortcut.is_empty() {
            "palette".to_string()
        } else {
            command.shortcut
        };
        by_category
            .entry(command.category)
            .or_default()
            .push((shortcut, command.label));
    }

    by_category
        .into_iter()
        .map(|(category, mut entries)| {
            entries.sort_by(|left, right| left.1.cmp(&right.1));
            HelpSection {
                title: format!("Commands: {category}"),
                entries,
            }
        })
        .collect()
}

fn help_rows(state: &HelpModalState<'_>) -> Vec<HelpRow> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();

    for section in help_sections(state) {
        let title = section.title;
        for (shortcut, label) in section.entries {
            if !seen.insert((shortcut.clone(), label.clone())) {
                continue;
            }
            rows.push(HelpRow {
                section: title.clone(),
                search_text: format!("{shortcut} {label} {title}"),
                shortcut,
                label,
                order: rows.len(),
            });
        }
    }

    rows
}

fn search_results(state: &HelpModalState<'_>) -> Vec<HelpSearchResult> {
    if state.query.is_empty() {
        return Vec::new();
    }

    let query_lower = state.query.to_lowercase();
    let pattern = Pattern::new(
        state.query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut utf32_buf = Vec::new();
    let mut results: Vec<_> = help_rows(state)
        .into_iter()
        .filter_map(|row| {
            pattern
                .score(
                    Utf32Str::new(&row.search_text, &mut utf32_buf),
                    &mut matcher,
                )
                .map(|score| HelpSearchResult {
                    priority: match_priority(&row, &query_lower),
                    row,
                    score,
                })
        })
        .collect();
    results.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.row.order.cmp(&right.row.order))
    });
    if is_short_query(&query_lower) {
        let strict: Vec<_> = results
            .iter()
            .filter(|result| result.priority < 5)
            .cloned()
            .collect();
        if !strict.is_empty() {
            return strict;
        }
    }
    results
}

fn match_priority(row: &HelpRow, query_lower: &str) -> u8 {
    let shortcut = normalize(&row.shortcut);
    if shortcut == query_lower {
        return 0;
    }
    if !shortcut.is_empty() && shortcut.starts_with(query_lower) {
        return 1;
    }
    if !shortcut.is_empty() && shortcut.contains(query_lower) {
        return 2;
    }
    if has_word_prefix(&row.label, query_lower) || acronym(&row.label).starts_with(query_lower) {
        return 3;
    }
    if has_word_prefix(&row.section, query_lower) || acronym(&row.section).starts_with(query_lower)
    {
        return 4;
    }
    5
}

fn is_short_query(query_lower: &str) -> bool {
    query_lower.chars().count() <= 2
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn has_word_prefix(value: &str, query_lower: &str) -> bool {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .any(|word| !word.is_empty() && normalize(word).starts_with(query_lower))
}

fn acronym(value: &str) -> String {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .filter_map(|word| word.chars().next())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(crate) fn search_result_count(state: &HelpModalState<'_>) -> usize {
    search_results(state).len()
}

fn render_sections(sections: &[HelpSection], theme: &crate::theme::Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            section.title.clone(),
            Style::default().fg(theme.accent).bold(),
        )));
        for (key, action) in &section.entries {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{key:<20}"),
                    Style::default().fg(theme.text_primary).bold(),
                ),
                Span::styled(action.clone(), Style::default().fg(theme.text_secondary)),
            ]));
        }
    }

    lines
}

fn draw_grouped_help(
    frame: &mut Frame,
    area: Rect,
    state: &HelpModalState<'_>,
    theme: &crate::theme::Theme,
) {
    let lines = render_sections(&help_sections(state), theme);
    let content_height = lines.len();
    let paragraph = Paragraph::new(lines)
        .scroll((state.scroll_offset, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    let mut scrollbar_state =
        ScrollbarState::new(content_height.saturating_sub(area.height as usize))
            .position(state.scroll_offset as usize);
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.warning)),
        area,
        &mut scrollbar_state,
    );
}

fn draw_search_results(
    frame: &mut Frame,
    area: Rect,
    state: &HelpModalState<'_>,
    theme: &crate::theme::Theme,
) {
    if area.height < 7 {
        return;
    }

    let results = search_results(state);
    let selected = state.selected.min(results.len().saturating_sub(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    let query = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent).bold()),
        Span::styled(state.query, Style::default().fg(theme.text_primary)),
    ]))
    .block(
        Block::bordered()
            .title(format!(
                " Query  {} matches  {} ",
                results.len(),
                state.ui_context.label()
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused))
            .style(Style::default().bg(theme.hint_bar_bg)),
    );
    frame.render_widget(query, chunks[0]);

    let list_area = chunks[1];
    let visible_len = list_area.height.saturating_sub(2) as usize;
    let start = if visible_len == 0 {
        0
    } else {
        selected.saturating_sub(visible_len.saturating_sub(1) / 2)
    };
    let rows: Vec<Row> = results
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_len)
        .map(|(index, result)| {
            let style = if index == selected {
                theme.highlight_style()
            } else {
                Style::default().fg(theme.text_secondary)
            };
            Row::new(vec![
                Cell::from(Span::styled(
                    &result.row.shortcut,
                    Style::default().fg(theme.text_primary).bold(),
                )),
                Cell::from(Span::styled(
                    &result.row.label,
                    Style::default().fg(theme.text_primary),
                )),
                Cell::from(Span::styled(
                    &result.row.section,
                    Style::default().fg(theme.text_muted),
                )),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Fill(1),
            Constraint::Length(24),
        ],
    )
    .column_spacing(1)
    .block(
        Block::bordered()
            .title(" Matches ")
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused)),
    );
    frame.render_widget(table, list_area);

    let mut scrollbar_state =
        ScrollbarState::new(results.len().saturating_sub(visible_len)).position(start);
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.warning)),
        list_area,
        &mut scrollbar_state,
    );

    let footer_text = results
        .get(selected)
        .map(|result| {
            Line::from(vec![
                Span::styled("type ", Style::default().fg(theme.accent).bold()),
                Span::styled("search", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("↑↓ ", Style::default().fg(theme.accent).bold()),
                Span::styled("move", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("enter ", Style::default().fg(theme.accent).bold()),
                Span::styled("close", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled(
                    result.row.label.clone(),
                    Style::default().fg(theme.text_primary).bold(),
                ),
                Span::styled(" · ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    result.row.shortcut.clone(),
                    Style::default().fg(theme.accent),
                ),
                Span::styled(" · ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    result.row.section.clone(),
                    Style::default().fg(theme.text_muted),
                ),
            ])
        })
        .unwrap_or_else(|| {
            Line::from(vec![
                Span::styled(
                    "No matching help entries",
                    Style::default().fg(theme.text_muted),
                ),
                Span::raw("   "),
                Span::styled("Esc", Style::default().fg(theme.accent).bold()),
                Span::styled(" close", Style::default().fg(theme.text_secondary)),
            ])
        });
    let footer = Paragraph::new(footer_text).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused)),
    );
    frame.render_widget(footer, chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::{draw, help_rows, help_sections, search_results, HelpModalState};
    use crate::action::UiContext;
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn help_state(context: UiContext, query: &'static str) -> HelpModalState<'static> {
        HelpModalState {
            open: true,
            ui_context: context,
            selected_count: 2,
            scroll_offset: 0,
            query,
            selected: 0,
            _marker: std::marker::PhantomData,
        }
    }

    #[test]
    fn help_sections_cover_accounts_and_commands() {
        let state = help_state(UiContext::AccountsList, "");
        let titles: Vec<String> = help_sections(&state)
            .into_iter()
            .map(|section| section.title)
            .collect();
        assert!(titles.contains(&"Accounts Page".to_string()));
        assert!(titles
            .iter()
            .any(|title| title.starts_with("Commands: Accounts")));
        assert!(!titles
            .iter()
            .any(|title| title.starts_with("Commands: Mail")));
    }

    #[test]
    fn help_search_fuzzy_matches_descriptions() {
        let state = help_state(UiContext::MailboxList, "fcmb");
        let first = search_results(&state).into_iter().next().unwrap();
        assert_eq!(first.row.shortcut, "Ctrl-f");
        assert_eq!(first.row.label, "Filter current mailbox only");
    }

    #[test]
    fn help_search_fuzzy_matches_shortcuts() {
        let state = help_state(UiContext::MailboxList, "gc");
        let results = search_results(&state);
        assert_eq!(results[0].row.shortcut, "gc");
        assert!(results[0].row.label.contains("Edit config"));
    }

    #[test]
    fn help_short_queries_drop_weak_fuzzy_tail() {
        let state = help_state(UiContext::MailboxList, "gc");
        let labels: Vec<String> = search_results(&state)
            .into_iter()
            .map(|result| result.row.label)
            .collect();
        assert!(labels.iter().all(|label| !label.contains("Go to Sent")));
        assert!(labels
            .iter()
            .all(|label| !label.contains("Generate Bug Report")));
    }

    #[test]
    fn help_rows_deduplicate_duplicate_bindings() {
        let state = help_state(UiContext::SearchEditor, "");
        let duplicates = help_rows(&state)
            .into_iter()
            .filter(|row| row.shortcut == "Enter" && row.label == "Run search now")
            .count();
        assert_eq!(duplicates, 1);
    }

    #[test]
    fn help_modal_grouped_snapshot() {
        let snapshot = render_to_string(100, 28, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 28),
                help_state(UiContext::MailboxList, ""),
                &crate::theme::Theme::default(),
            );
        });
        insta::assert_snapshot!("help_modal_grouped_snapshot", snapshot);
    }

    #[test]
    fn help_modal_filtered_snapshot() {
        let snapshot = render_to_string(100, 28, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 28),
                help_state(UiContext::MailboxList, "gc"),
                &crate::theme::Theme::default(),
            );
        });
        insta::assert_snapshot!("help_modal_filtered_snapshot", snapshot);
    }
}
