use super::centered_rect;
use crate::action::UiContext;
use crate::keybindings::ViewContext;
use crate::ui::command_palette::commands_for_context;
use crate::ui::help_keys::{places_for, KeyRows, ALWAYS_LISTED};
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
    pub keybindings: Option<&'a crate::keybindings::KeybindingConfig>,
    pub binding_context: Option<ViewContext>,
    pub open: bool,
    pub ui_context: UiContext,
    pub selected_count: usize,
    pub scroll_offset: u16,
    pub query: &'a str,
    pub selected: usize,
    pub mode_guide: Option<&'a mxr_protocol::ModeGuideData>,
    pub key_places: Vec<&'static str>,
    pub mode_common: Option<Vec<(String, String)>>,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: HelpModalState<'_>, theme: &crate::theme::Theme) {
    if !state.open {
        return;
    }

    let popup = centered_rect(88, 88, area);
    frame.render_widget(Clear, popup);

    let single_keys = state.keybindings.is_none_or(|config| config.single_keys);
    let block = Block::bordered()
        .title(if single_keys {
            " Help "
        } else {
            " Help: single-key shortcuts are off; Ctrl-k opens the palette "
        })
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

fn mode_section(guide: &mxr_protocol::ModeGuideData) -> HelpSection {
    let mut entries = vec![
        ("What it's for".to_string(), guide.header.clone()),
        ("What lands here".to_string(), guide.lands_here.clone()),
        ("How it works".to_string(), guide.about.clone()),
    ];
    entries.extend(
        guide
            .keys
            .iter()
            .map(|key| (key.key.clone(), key.verb.clone())),
    );
    HelpSection {
        title: guide.name.clone(),
        entries,
    }
}

fn key_rows<'a>(state: &'a HelpModalState<'_>) -> KeyRows<'a> {
    KeyRows {
        config: state
            .keybindings
            .unwrap_or_else(|| crate::keybindings::shown_keybindings()),
        view: binding_context(state),
    }
}

fn places(state: &HelpModalState<'_>) -> Vec<&'static str> {
    if state.key_places.is_empty() {
        places_for(state.ui_context)
    } else {
        state.key_places.clone()
    }
}

fn help_sections(state: &HelpModalState<'_>) -> Vec<HelpSection> {
    let keys = key_rows(state);
    let places = places(state);
    let here = places.first().copied().unwrap_or("list");
    let shell = if mxr_keymap::TUI_CONTEXTS.contains(&here) {
        here
    } else {
        "list"
    };
    let mut sections: Vec<HelpSection> = state.mode_guide.map(mode_section).into_iter().collect();
    sections.push(HelpSection {
        title: "Start Here".into(),
        entries: [
            "shell.command-palette",
            "assistant.toggle",
            "nav.settings",
            "shell.help",
            "list.escape",
            "tui.quit-view",
        ]
        .into_iter()
        .filter_map(|id| defined_row(&keys, id, shell))
        .collect(),
    });
    sections.push(how_do_i(&keys, shell));
    sections.push(HelpSection {
        title: "Most Common Actions".into(),
        entries: common_entries(state, &keys, here),
    });
    for place in places.iter().chain(&ALWAYS_LISTED) {
        let entries = keys.rows(place);
        if !entries.is_empty() {
            sections.push(HelpSection {
                title: format!("{} keys", mxr_keymap::place_title(place).unwrap_or(place)),
                entries,
            });
        }
    }
    for overlay in mxr_keymap::OVERLAY_CONTEXTS {
        let entries = keys.rows(overlay);
        if !entries.is_empty() {
            sections.push(HelpSection {
                title: format!(
                    "{} keys",
                    mxr_keymap::place_title(overlay).unwrap_or(overlay)
                ),
                entries,
            });
        }
    }
    sections.extend(command_sections(state));
    sections
}

fn defined_row(keys: &KeyRows<'_>, id: &str, place: &str) -> Option<(String, String)> {
    let action = mxr_keymap::defaults().get(id)?;
    Some((keys.key(id, place)?, action.label.clone()))
}

fn how_do_i(keys: &KeyRows<'_>, place: &str) -> HelpSection {
    let key = |id: &str, place: &str| keys.key(id, place).unwrap_or_else(|| "palette".into());
    HelpSection {
        title: "How do I…".into(),
        entries: vec![
            (
                "Add an account".into(),
                format!(
                    "{} → {} (or palette: Add account)",
                    key("nav.settings", place),
                    key("tui.accounts.new", "accounts")
                ),
            ),
            (
                "Compose and send".into(),
                format!(
                    "{} → write in $EDITOR → save → s to send",
                    key("shell.compose", "list")
                ),
            ),
            (
                "Search and save a search".into(),
                format!(
                    "{} to query → palette: New Saved Search → {} in sidebar to refine",
                    key("shell.search-palette", place),
                    key("tui.open-saved-search-form-new", "sidebar")
                ),
            ),
            (
                "Manage rules".into(),
                format!(
                    "{} → {} new / {} edit / {} dry-run / palette: Refresh Rules",
                    key("nav.rules", place),
                    key("tui.rules.new", "rules"),
                    key("tui.rules.edit", "rules"),
                    key("tui.rules.dry-run", "rules")
                ),
            ),
            (
                "Reset local state".into(),
                "shell: mxr reset --hard (or mxr burn) — keeps config + creds".into(),
            ),
            (
                "Repair an unhealthy account".into(),
                format!(
                    "{} → select account → {} when [unhealthy] indicator shown",
                    key("nav.settings", place),
                    key("tui.accounts.repair", "accounts")
                ),
            ),
            (
                "Open analytics".into(),
                "palette: Analytics: Storage / Stale Threads / Contact Asymmetry / Response Time"
                    .into(),
            ),
            (
                "Undo a destructive action".into(),
                format!(
                    "{} within 60s of archive/trash/spam/mark-read",
                    key("mail.undo", "list")
                ),
            ),
        ],
    }
}

fn common_entries(
    state: &HelpModalState<'_>,
    keys: &KeyRows<'_>,
    here: &str,
) -> Vec<(String, String)> {
    let mut entries = vec![("Context".to_string(), state.ui_context.label().to_string())];
    if let Some(common) = &state.mode_common {
        entries.extend(common.iter().cloned());
    } else {
        let ids: &[&str] = match state.ui_context {
            UiContext::MailboxSidebar | UiContext::MailboxList | UiContext::MailboxMessage => &[
                "shell.search-palette",
                "list.filter",
                "shell.command-palette",
            ],
            UiContext::SearchEditor => &["tui.search.run", "tui.search.stop-editing"],
            UiContext::SearchResults => &["tui.search.edit", "tui.search.open", "tui.screen.back"],
            UiContext::SearchPreview => &["tui.search.edit", "tui.search-preview.results"],
            UiContext::RulesList | UiContext::RulesForm => {
                &["tui.rules.new", "tui.rules.dry-run", "tui.rules.edit"]
            }
            UiContext::Diagnostics => &[
                "tui.diagnostics.refresh",
                "tui.diagnostics.config",
                "tui.diagnostics.logs",
            ],
            UiContext::AccountsList | UiContext::AccountsForm => {
                &["tui.accounts.new", "tui.accounts.test"]
            }
            UiContext::Analytics => &[
                "tui.analytics.next-view",
                "tui.analytics.previous-view",
                "tui.analytics.refresh",
                "tui.screen.back",
            ],
            UiContext::Deliveries => &[
                "tui.deliveries.resolve",
                "tui.deliveries.dismiss",
                "tui.deliveries.filter",
                "tui.deliveries.refresh",
            ],
        };
        let place = if matches!(
            state.ui_context,
            UiContext::MailboxSidebar | UiContext::MailboxList | UiContext::MailboxMessage
        ) {
            here
        } else {
            places_for(state.ui_context)[0]
        };
        entries.extend(ids.iter().filter_map(|id| defined_row(keys, id, place)));
    }

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

fn binding_context(state: &HelpModalState<'_>) -> ViewContext {
    state.binding_context.unwrap_or(match state.ui_context {
        UiContext::MailboxMessage | UiContext::SearchPreview => ViewContext::ThreadView,
        _ => ViewContext::MailList,
    })
}

fn command_sections(state: &HelpModalState<'_>) -> Vec<HelpSection> {
    let mut by_category: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let commands = match state.keybindings {
        Some(config) => crate::ui::command_palette::commands_with_keybindings(
            state.ui_context,
            config,
            binding_context(state),
        ),
        None => commands_for_context(state.ui_context),
    };
    for command in commands {
        let search_owns_slash = matches!(
            state.ui_context,
            UiContext::SearchResults | UiContext::SearchPreview
        ) && command.action == crate::action::Action::OpenGlobalSearch
            && command.shortcut == "/"
            && !state
                .keybindings
                .unwrap_or_else(|| crate::keybindings::shown_keybindings())
                .overrides_prefix(
                    binding_context(state),
                    &crate::keybindings::KeyPress::from_event(crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Char('/'),
                        crossterm::event::KeyModifiers::NONE,
                    )),
                );
        let shortcut = if command.shortcut.is_empty() || search_owns_slash {
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
        .flat_map(char::to_lowercase)
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
        .flat_map(char::to_lowercase)
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
        let column = section
            .entries
            .iter()
            .map(|(key, _)| key.chars().count() + 1)
            .max()
            .unwrap_or(0)
            .max(20);
        for (key, action) in &section.entries {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{key:<column$}"),
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

    let footer_text = results.get(selected).map_or_else(
        || {
            Line::from(vec![
                Span::styled(
                    "No matching help entries",
                    Style::default().fg(theme.text_muted),
                ),
                Span::raw("   "),
                Span::styled("Esc", Style::default().fg(theme.accent).bold()),
                Span::styled(" close", Style::default().fg(theme.text_secondary)),
            ])
        },
        |result| {
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
        },
    );
    let footer = Paragraph::new(footer_text).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused)),
    );
    frame.render_widget(footer, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::{help_rows, help_sections, HelpModalState};
    use crate::action::UiContext;

    fn help_state(context: UiContext, query: &'static str) -> HelpModalState<'static> {
        HelpModalState {
            keybindings: None,
            binding_context: None,
            open: true,
            ui_context: context,
            selected_count: 2,
            scroll_offset: 0,
            query,
            selected: 0,
            mode_guide: None,
            key_places: Vec::new(),
            mode_common: None,
            _marker: std::marker::PhantomData,
        }
    }

    #[test]
    fn configured_help_and_palette_show_effective_keys_without_old_shortcuts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keys.toml"),
            "[message_view]\n\";\" = \"archive\"\n\"wq\" = \"trash\"\n",
        )
        .unwrap();
        let config = crate::keybindings::load_keybindings(dir.path());
        let mut state = help_state(UiContext::MailboxMessage, "");
        state.keybindings = Some(&config);
        state.binding_context = Some(crate::keybindings::ViewContext::MessageView);
        let rows = help_rows(&state);
        assert!(rows
            .iter()
            .any(|row| row.label == "Archive" && row.shortcut == ";"));
        assert!(rows
            .iter()
            .any(|row| row.label == "Move to Trash" && row.shortcut == "wq"));
        assert!(!rows.iter().any(|row| matches!(
            (row.shortcut.as_str(), row.label.as_str()),
            ("e", "Archive") | ("#", "Move to Trash")
        )));
        let mut palette = crate::ui::command_palette::CommandPalette {
            keybindings: config.clone(),
            binding_context: state.binding_context,
            ..Default::default()
        };
        palette.toggle_with_archive_operations(UiContext::MailboxMessage, &[]);
        for (action, shortcut) in [
            (crate::action::Action::Archive, ";"),
            (crate::action::Action::Trash, "wq"),
        ] {
            let command = palette
                .visible_commands()
                .find(|command| command.action == action)
                .unwrap();
            assert_eq!(command.shortcut, shortcut);
            let selected = palette
                .visible_commands()
                .position(|command| command.action == action)
                .unwrap();
            palette.selected = selected;
            assert_eq!(palette.confirm(), Some(action));
            palette.toggle_with_archive_operations(UiContext::MailboxMessage, &[]);
        }
    }

    #[test]
    fn every_bound_tui_key_shows_in_help_where_it_works() {
        let keymap = mxr_keymap::defaults();
        let places = mxr_keymap::TUI_CONTEXTS
            .iter()
            .chain(mxr_keymap::SCREEN_CONTEXTS.iter())
            .chain(mxr_keymap::OVERLAY_CONTEXTS.iter())
            .copied()
            .filter(|place| {
                keymap
                    .actions
                    .iter()
                    .flat_map(|a| &a.tui)
                    .any(|b| !b.keys.is_empty() && b.contexts.includes(place))
            });
        let mut missing = Vec::new();
        for place in places {
            let mut state = help_state(UiContext::MailboxList, "");
            state.key_places = vec![place];
            let title = format!("{} keys", mxr_keymap::place_title(place).unwrap_or(place));
            let sections = help_sections(&state);
            let Some(section) = sections.iter().find(|section| section.title == title) else {
                missing.push(format!("{place}: no section"));
                continue;
            };
            for action in &keymap.actions {
                for binding in action.tui.iter().filter(|b| b.contexts.includes(place)) {
                    for key in &binding.keys {
                        let shown = crate::ui::command_palette::shown_key(key);
                        let listed = section.entries.iter().any(|(keys, label)| {
                            *label == action.label && keys.split(" / ").any(|k| k == shown)
                        });
                        if !listed {
                            missing.push(format!("{place}: `{shown}` {}", action.label));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }
}
