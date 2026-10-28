//! The command palette (`:`, `Ctrl-k`). Its commands come from the key
//! definition (`crates/keymap/keymap.toml`, blueprint 35): every TUI action
//! live where the keys are going, with all of its keys there after the
//! person's remaps, and every palette-only action. Nothing here names a key.

use crate::action::{action_allowed_in_context, Action, UiContext};
use crate::keybindings::{KeybindingConfig, ViewContext};
use crate::keymap_names::action_named;
use mxr_keymap::{Client, KeyAction, TuiBinding, MODE_SECTIONS};
use ratatui::prelude::*;
use ratatui::widgets::*;

/// Maximum number of recently-used commands the palette tracks for
/// adaptive ranking. Older entries are evicted FIFO.
pub const RECENT_COMMAND_CAPACITY: usize = 8;

#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// The definition's action id; `None` for archive recovery commands,
    /// which come from the local journal.
    pub id: Option<&'static str>,
    pub label: String,
    /// Every key that runs it here, `" / "` between them; empty for none.
    pub shortcut: String,
    pub action: Action,
    pub category: String,
}

impl PaletteCommand {
    /// The key to name for it here, if it has one.
    pub fn first_key(&self) -> Option<&str> {
        self.shortcut
            .split(" / ")
            .next()
            .filter(|key| !key.is_empty())
    }
}

pub struct CommandPalette {
    pub(crate) keybindings: KeybindingConfig,
    pub(crate) binding_context: Option<ViewContext>,
    /// Where keys are going (`App::key_place`): a mode, the list, the
    /// reader, the sidebar; `None` off the mailbox.
    pub(crate) key_place: Option<&'static str>,
    pub visible: bool,
    pub input: String,
    pub commands: Vec<PaletteCommand>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub context: UiContext,
    /// Most-recent-first list of actions the user has confirmed. Used to
    /// bias filtered results toward muscle-memory commands.
    pub recent_actions: Vec<Action>,
    /// Rebuild the commands from the definition on each open. False for a
    /// palette given its own commands (`with_commands`).
    pub(crate) from_definition: bool,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            from_definition: true,
            ..Self::with_commands(default_commands())
        }
    }
}

impl CommandPalette {
    pub fn with_commands(commands: Vec<PaletteCommand>) -> Self {
        let context = UiContext::MailboxList;
        let recent_actions: Vec<Action> = Vec::new();
        let filtered = filtered_indices(&commands, context, "", &recent_actions);
        Self {
            keybindings: crate::keybindings::default_keybindings(),
            binding_context: None,
            key_place: None,
            visible: false,
            input: String::new(),
            commands,
            filtered,
            selected: 0,
            context,
            recent_actions,
            from_definition: false,
        }
    }

    pub fn toggle(&mut self, context: UiContext) {
        let operations = if self.visible {
            Vec::new()
        } else {
            archive_operations(&mxr_config::data_dir())
        };
        self.toggle_with_archive_operations(context, &operations);
    }

    pub fn toggle_with_archive_operations(
        &mut self,
        context: UiContext,
        operations: &[mxr_protocol::ArchiveOperationData],
    ) {
        self.visible = !self.visible;
        if self.visible {
            self.context = context;
            // Rebuilt on each open, so each row shows the keys that work
            // where the palette was opened, with the person's remaps.
            if self.from_definition {
                let place = self.key_place.or_else(|| place_for(context));
                let view = self
                    .binding_context
                    .unwrap_or_else(|| view_context_for(context));
                self.commands = defined_commands(place, &self.keybindings, view);
            }
            add_archive_recovery_commands(&mut self.commands, operations);
            self.input.clear();
            self.selected = 0;
            self.update_filtered(None);
        }
    }

    pub fn on_char(&mut self, c: char) {
        let preferred = self.selected_action().cloned();
        self.input.push(c);
        self.update_filtered(preferred);
    }

    pub fn on_backspace(&mut self) {
        let preferred = self.selected_action().cloned();
        self.input.pop();
        self.update_filtered(preferred);
    }

    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn confirm(&mut self) -> Option<Action> {
        self.confirm_command().map(|command| command.action)
    }

    /// Close the palette on the highlighted command and return it.
    pub fn confirm_command(&mut self) -> Option<PaletteCommand> {
        let &idx = self.filtered.get(self.selected)?;
        self.visible = false;
        let command = self.commands[idx].clone();
        self.record_recent(command.action.clone());
        Some(command)
    }

    /// Record a confirmed action as most-recent. Existing occurrences are
    /// removed first so the same action can't appear twice; the list is
    /// then capped at [`RECENT_COMMAND_CAPACITY`].
    fn record_recent(&mut self, action: Action) {
        self.recent_actions.retain(|existing| existing != &action);
        self.recent_actions.insert(0, action);
        if self.recent_actions.len() > RECENT_COMMAND_CAPACITY {
            self.recent_actions.truncate(RECENT_COMMAND_CAPACITY);
        }
    }

    pub fn visible_commands(&self) -> impl Iterator<Item = &PaletteCommand> {
        self.filtered
            .iter()
            .filter_map(|&index| self.commands.get(index))
    }

    /// Re-hydrate the recents list from persisted labels. Labels that
    /// don't match a current command (renamed/removed) are silently
    /// skipped. Capped at [`RECENT_COMMAND_CAPACITY`] so a corrupted
    /// state file can't grow the in-memory list unbounded.
    pub fn restore_recents_from_labels(&mut self, labels: &[String]) {
        self.recent_actions.clear();
        for label in labels.iter().take(RECENT_COMMAND_CAPACITY) {
            if let Some(action) = self
                .commands
                .iter()
                .find(|cmd| &cmd.label == label)
                .map(|cmd| cmd.action.clone())
            {
                self.recent_actions.push(action);
            }
        }
        self.update_filtered(None);
    }

    /// Snapshot the current recents list as labels for persistence.
    /// Labels are stable across versions; the `Action` enum is not.
    pub fn recent_labels_snapshot(&self) -> Vec<String> {
        self.recent_actions
            .iter()
            .filter_map(|action| {
                self.commands
                    .iter()
                    .find(|cmd| &cmd.action == action)
                    .map(|cmd| cmd.label.clone())
            })
            .collect()
    }

    fn selected_action(&self) -> Option<&Action> {
        self.filtered
            .get(self.selected)
            .and_then(|&index| self.commands.get(index))
            .map(|command| &command.action)
    }

    fn update_filtered(&mut self, preferred_action: Option<Action>) {
        self.filtered = filtered_indices(
            &self.commands,
            self.context,
            &self.input,
            &self.recent_actions,
        );
        if let Some(action) = preferred_action {
            if let Some(index) = self
                .filtered
                .iter()
                .position(|&command_index| self.commands[command_index].action == action)
            {
                self.selected = index;
                return;
            }
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }
}

/// The commands help lists for `context`, with the default keys.
pub fn commands_for_context(context: UiContext) -> Vec<PaletteCommand> {
    commands_with_keybindings(
        context,
        crate::keybindings::shown_keybindings(),
        view_context_for(context),
    )
}

/// The commands help lists for `context`, with the person's keys.
pub(crate) fn commands_with_keybindings(
    context: UiContext,
    config: &KeybindingConfig,
    view: ViewContext,
) -> Vec<PaletteCommand> {
    defined_commands(place_for(context), config, view)
        .into_iter()
        .filter(|command| action_allowed_in_context(&command.action, context))
        .collect()
}

/// Every command the definition offers, wherever it is live, with its
/// default keys: what recents are restored against before the palette
/// first opens.
pub fn default_commands() -> Vec<PaletteCommand> {
    let mut commands = defined_commands(
        None,
        crate::keybindings::shown_keybindings(),
        ViewContext::MailList,
    );
    add_archive_recovery_commands(&mut commands, &[]);
    commands
}

/// The definition's context for a mailbox pane, when the app didn't say.
fn place_for(context: UiContext) -> Option<&'static str> {
    match context {
        UiContext::MailboxList => Some("list"),
        UiContext::MailboxMessage => Some("reader"),
        UiContext::MailboxSidebar => Some("sidebar"),
        _ => None,
    }
}

/// Keybinding view map that governs which chord fires for `context`.
/// Message/preview panes resolve through the thread-view map (matching
/// the help modal's mapping); everything else uses the mail-list map.
fn view_context_for(context: UiContext) -> ViewContext {
    match context {
        UiContext::MailboxMessage | UiContext::SearchPreview => ViewContext::ThreadView,
        _ => ViewContext::MailList,
    }
}

/// One command per TUI action the definition lists in `place` (every TUI
/// action off the mailbox, where `action_allowed_in_context` filters), in
/// the definition's order. Motion keys stay out, as in the web palette.
pub(crate) fn defined_commands(
    place: Option<&str>,
    config: &KeybindingConfig,
    view: ViewContext,
) -> Vec<PaletteCommand> {
    let keymap = mxr_keymap::defaults();
    let mut commands: Vec<PaletteCommand> = Vec::new();
    for action in &keymap.actions {
        if !action.is_for(Client::Tui) || action.hide_in_palette {
            continue;
        }
        // The bindings live here; off the mailbox, the list's (or any).
        let live: Vec<&TuiBinding> = match place {
            Some(place) => action
                .tui
                .iter()
                .filter(|b| b.contexts.includes(place))
                .collect(),
            None => {
                let list: Vec<&TuiBinding> = action
                    .tui
                    .iter()
                    .filter(|b| b.contexts.includes("list"))
                    .collect();
                if list.is_empty() {
                    action.tui.iter().take(1).collect()
                } else {
                    list
                }
            }
        };
        // The first binding's action is what the palette runs; the others
        // only add keys (`g 0` is Inbox too).
        let Some(tui_action) = live.first().and_then(|b| action_named(&b.name)) else {
            continue;
        };
        // Two definition actions can run one TUI action in one place (Reply
        // in the list and in Now); the first names it.
        if commands.iter().any(|command| command.action == tui_action) {
            continue;
        }
        let mut keys: Vec<String> = Vec::new();
        for binding in live {
            for key in effective_keys(action, binding, place, config, view, &tui_action) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        commands.push(PaletteCommand {
            id: Some(action.id.as_str()),
            label: action.label.clone(),
            shortcut: keys.join(" / "),
            action: tui_action,
            category: action.group.clone(),
        });
    }
    #[cfg(debug_assertions)]
    commands.push(PaletteCommand {
        id: None,
        label: "Dump Action Trace".into(),
        shortcut: "Ctrl-Alt-d".into(),
        action: Action::DumpActionTrace,
        category: "Diagnostics".into(),
    });
    commands
}

/// The keys that run `action` in `place`: a mode section's remaps, or the
/// list and reader maps' remaps from `keys.toml`, else the definition's.
fn effective_keys(
    action: &KeyAction,
    binding: &TuiBinding,
    place: Option<&str>,
    config: &KeybindingConfig,
    view: ViewContext,
    tui_action: &Action,
) -> Vec<String> {
    match place {
        Some(place) if MODE_SECTIONS.contains(&place) => {
            let remaps = &config.modes;
            let remapped = remaps.keys_for(place, &action.id);
            if !remapped.is_empty() {
                return remapped.into_iter().map(shown_key).collect();
            }
            binding
                .keys
                .iter()
                .filter(|key| remaps.lookup(place, key).is_none())
                .map(|key| shown_key(key))
                .collect()
        }
        // The list and reader maps also govern search and the other screens.
        Some("list" | "reader") | None if config.has_action_override(view, tui_action) => {
            config.key_displays(view, tui_action)
        }
        _ => binding.keys.iter().map(|key| shown_key(key)).collect(),
    }
}

/// A definition key as the TUI spells keys elsewhere (help, the hint bar):
/// `g R` → `gR`, `Ctrl+d` → `Ctrl-d`, `Escape` → `Esc`.
fn shown_key(key: &str) -> String {
    key.split(' ')
        .map(|chord| match chord {
            "Escape" => "Esc".to_string(),
            "ArrowUp" => "Up".to_string(),
            "ArrowDown" => "Down".to_string(),
            "ArrowLeft" => "Left".to_string(),
            "ArrowRight" => "Right".to_string(),
            "+" => "+".to_string(),
            chord => chord.replace('+', "-"),
        })
        .collect()
}

fn filtered_indices(
    commands: &[PaletteCommand],
    context: UiContext,
    input: &str,
    recent_actions: &[Action],
) -> Vec<usize> {
    let query = input.to_lowercase();
    let recent_position = |action: &Action| -> usize {
        recent_actions
            .iter()
            .position(|recent| recent == action)
            .unwrap_or(usize::MAX)
    };
    let mut scored: Vec<(usize, u8, usize)> = commands
        .iter()
        .enumerate()
        .filter(|(_, command)| action_allowed_in_context(&command.action, context))
        .filter_map(|(index, command)| {
            match_score(command, &query)
                .map(|score| (index, score, recent_position(&command.action)))
        })
        .collect();
    // Sort by (match score, recency, registration order). Match strength
    // dominates, so a strong query match still wins over a recent
    // command that doesn't match the query well.
    scored.sort_by_key(|(index, score, recency)| (*score, *recency, *index));
    scored.into_iter().map(|(index, _, _)| index).collect()
}

/// Score a command against a query. Lower scores rank higher. `None`
/// means no match. Empty query matches every command at the same score.
fn match_score(command: &PaletteCommand, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let label = command.label.to_lowercase();
    if label == query {
        return Some(0);
    }
    if label.starts_with(query) {
        return Some(1);
    }
    if label.split_whitespace().any(|word| word.starts_with(query)) {
        return Some(2);
    }
    if label.contains(query) {
        return Some(3);
    }
    let shortcut = command.shortcut.to_lowercase();
    let category = command.category.to_lowercase();
    if shortcut.contains(query) || category.contains(query) {
        return Some(4);
    }
    None
}

fn archive_operations(data_dir: &std::path::Path) -> Vec<mxr_protocol::ArchiveOperationData> {
    mxr_client::ArchiveJournal::open_existing(data_dir)
        .ok()
        .flatten()
        .and_then(|journal| journal.list().ok())
        .unwrap_or_default()
}

fn add_archive_recovery_commands(
    commands: &mut Vec<PaletteCommand>,
    operations: &[mxr_protocol::ArchiveOperationData],
) {
    if !commands
        .iter()
        .any(|command| command.action == Action::PreviewArchiveSelection)
    {
        commands.push(PaletteCommand {
            id: None,
            label: "Preview Archive selection".into(),
            category: "Recovery".into(),
            shortcut: String::new(),
            action: Action::PreviewArchiveSelection,
        });
    }
    commands.retain(|command| {
        !matches!(
            command.action,
            Action::CheckArchiveOperation(_) | Action::SubmitArchiveOperation(_)
        )
    });
    for operation in operations {
        let id = operation.identity.operation_id.clone();
        commands.push(PaletteCommand {
            id: None,
            label: format!("Archive recovery: query {id}"),
            shortcut: String::new(),
            action: Action::CheckArchiveOperation(id.clone()),
            category: "Recovery".into(),
        });
        commands.push(PaletteCommand {
            id: None,
            label: format!("Archive recovery: submit reviewed {id}"),
            shortcut: String::new(),
            action: Action::SubmitArchiveOperation(id),
            category: "Recovery".into(),
        });
    }
}

pub fn draw(frame: &mut Frame, area: Rect, palette: &CommandPalette, theme: &crate::theme::Theme) {
    if !palette.visible {
        return;
    }

    let width = ((area.width as u32 * 62) / 100).clamp(56, 96) as u16;
    let width = width.min(area.width.saturating_sub(4)).max(1);
    let preferred_height = area.height.saturating_sub(6).clamp(14, 22);
    let height = preferred_height.min(area.height.saturating_sub(2).max(1));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = (area.y + 2).min(area.y + area.height.saturating_sub(height));
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Command Palette ")
        .title_style(Style::default().fg(theme.accent).bold())
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(inner);

    let selected_command = palette
        .filtered
        .get(palette.selected)
        .and_then(|&idx| palette.commands.get(idx));

    let query_text = if palette.input.is_empty() {
        "type a command or shortcut".to_string()
    } else {
        palette.input.clone()
    };
    let input_block = Block::bordered()
        .title(format!(
            " Query  {} matches  {} ",
            palette.filtered.len(),
            palette.context.label()
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_unfocused))
        .style(Style::default().bg(theme.hint_bar_bg));
    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent).bold()),
        Span::styled(
            query_text,
            Style::default().fg(if palette.input.is_empty() {
                theme.text_muted
            } else {
                theme.text_primary
            }),
        ),
    ]))
    .block(input_block);
    frame.render_widget(input, chunks[0]);

    let list_area = chunks[1];

    let visible_len = list_area.height.saturating_sub(2) as usize;
    let start = if visible_len == 0 {
        0
    } else {
        palette
            .selected
            .saturating_sub(visible_len.saturating_sub(1) / 2)
    };
    let rows: Vec<Row> = palette
        .filtered
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_len)
        .map(|(i, &cmd_idx)| {
            let cmd = &palette.commands[cmd_idx];
            let style = if i + start == palette.selected {
                theme.highlight_style()
            } else {
                Style::default().fg(theme.text_secondary)
            };
            let (icon, category_color) = category_style(&cmd.category, theme);
            let shortcut = if cmd.shortcut.is_empty() {
                Span::styled("palette", Style::default().fg(theme.text_muted))
            } else {
                Span::styled(
                    cmd.shortcut.clone(),
                    Style::default().fg(theme.text_primary).bold(),
                )
            };
            Row::new(vec![
                Cell::from(Span::styled(
                    icon,
                    Style::default().fg(category_color).bold(),
                )),
                Cell::from(Line::from(vec![
                    Span::styled(
                        format!(" {} ", cmd.category),
                        Style::default().bg(category_color).fg(Color::Black).bold(),
                    ),
                    Span::raw(" "),
                    Span::styled(&cmd.label, Style::default().fg(theme.text_primary)),
                ])),
                Cell::from(shortcut),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(10),
        ],
    )
    .column_spacing(1)
    .block(
        Block::bordered()
            .title(" Commands ")
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused)),
    );
    frame.render_widget(table, list_area);

    let mut scrollbar_state =
        ScrollbarState::new(palette.filtered.len().saturating_sub(visible_len)).position(start);

    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.warning)),
        list_area,
        &mut scrollbar_state,
    );

    let footer_text = selected_command.map_or_else(
        || {
            Line::from(Span::styled(
                "No matching commands",
                Style::default().fg(theme.text_muted),
            ))
        },
        |cmd| {
            let shortcut = if cmd.shortcut.is_empty() {
                "palette".to_string()
            } else {
                cmd.shortcut.clone()
            };
            Line::from(vec![
                Span::styled("enter ", Style::default().fg(theme.accent).bold()),
                Span::styled("run", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("↑↓ ", Style::default().fg(theme.accent).bold()),
                Span::styled("move", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("esc ", Style::default().fg(theme.accent).bold()),
                Span::styled("close", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("selected ", Style::default().fg(theme.text_muted)),
                Span::styled(&cmd.label, Style::default().fg(theme.text_primary).bold()),
                Span::styled(" · ", Style::default().fg(theme.text_muted)),
                Span::styled(shortcut, Style::default().fg(theme.accent)),
                Span::styled(" · ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    palette.context.label(),
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

fn category_style(category: &str, theme: &crate::theme::Theme) -> (&'static str, Color) {
    match category {
        "Mail" => ("@", theme.warning),
        "Navigate" => (">", theme.accent),
        "Search" => ("/", theme.link_fg),
        "Select" => ("+", theme.success),
        "View" => ("~", theme.text_secondary),
        "Rules" => ("#", theme.error),
        "Diagnostics" => ("!", theme.warning),
        "Accounts" => ("=", theme.accent_dim),
        "Sync" => ("*", theme.success),
        _ => ("?", theme.text_muted),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        archive_operations, commands_for_context, defined_commands, shown_key, CommandPalette,
    };
    use crate::action::UiContext;
    use crate::keybindings::{shown_keybindings, ViewContext};
    use crate::keymap_names::action_named;
    use mxr_keymap::{Client, TUI_CONTEXTS};

    /// Every key the TUI binds in a mail context is findable in the palette
    /// opened there, with every key the action has there. Motion the
    /// definition keeps out of palettes (`hide_in_palette`) is the exception.
    #[test]
    fn every_bound_key_is_findable_in_the_palette_where_it_works() {
        let keymap = mxr_keymap::defaults();
        let mut missing = Vec::new();
        for place in TUI_CONTEXTS {
            let commands =
                defined_commands(Some(place), shown_keybindings(), ViewContext::MailList);
            for action in keymap.actions.iter().filter(|a| a.is_for(Client::Tui)) {
                if action.hide_in_palette {
                    continue;
                }
                for binding in action.tui.iter().filter(|b| b.contexts.includes(place)) {
                    let Some(tui_action) = action_named(&binding.name) else {
                        continue;
                    };
                    let Some(command) = commands
                        .iter()
                        .find(|c| c.id == Some(action.id.as_str()))
                        .or_else(|| commands.iter().find(|c| c.action == tui_action))
                    else {
                        missing.push(format!("{place}: {} ({})", action.id, binding.name));
                        continue;
                    };
                    // The command another id names may carry other keys.
                    if command.id != Some(action.id.as_str()) {
                        continue;
                    }
                    for key in &binding.keys {
                        let shown = shown_key(key);
                        if !command.shortcut.split(" / ").any(|k| k == shown) {
                            missing.push(format!("{place}: {} lacks `{shown}`", action.id));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    /// The commands that had no key keep a palette entry: they are in the
    /// definition as palette-only actions now.
    #[test]
    fn palette_only_commands_stay_findable() {
        let labels: Vec<String> =
            defined_commands(None, shown_keybindings(), ViewContext::MailList)
                .into_iter()
                .map(|command| command.label)
                .collect();
        for needle in [
            "Edit config",
            "Export as Markdown",
            "Send to Microsoft To Do",
            "Sync the linked Microsoft To Do task",
            "Export the records in view…",
            "Mark this record checked",
            "Drafts saved on this computer",
            "Analytics: Storage",
            "Semantic: Install Profile (BGE-M3)",
            "Top of screen",
            "Center current",
        ] {
            assert!(
                labels.iter().any(|label| label == needle),
                "{needle} missing from {labels:?}"
            );
        }
    }

    /// Opened in To do, the palette offers To do's verbs with To do's keys,
    /// the tabs included, and a person's `[todo]` remap.
    #[test]
    fn a_modes_palette_shows_its_keys_and_remaps() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keys.toml"),
            "[todo]\n\"w\" = \"todo.done\"\n",
        )
        .unwrap();
        let config = crate::keybindings::load_keybindings(dir.path());
        let mut palette = CommandPalette {
            keybindings: config,
            key_place: Some("todo"),
            ..Default::default()
        };
        palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);
        let shortcut = |id: &str| {
            palette
                .commands
                .iter()
                .find(|command| command.id == Some(id))
                .map(|command| command.shortcut.clone())
        };
        assert_eq!(shortcut("todo.done").as_deref(), Some("w"));
        assert_eq!(shortcut("todo.catchup").as_deref(), Some("2"));
        assert_eq!(shortcut("todo.expired").as_deref(), Some("3"));
        assert_eq!(shortcut("nav.inbox").as_deref(), Some("gi / g0"));
        assert_eq!(shortcut("mail.reply"), None, "Reply is not a To do verb");
    }

    #[test]
    fn toggle_rederives_shortcuts_for_the_focused_context() {
        let mut palette = CommandPalette::default();
        let shortcut_of = |palette: &CommandPalette, id: &str| {
            palette
                .commands
                .iter()
                .find(|cmd| cmd.id == Some(id))
                .map(|cmd| cmd.shortcut.clone())
        };

        // The reader's keys, from the definition's reader context.
        palette.toggle_with_archive_operations(UiContext::MailboxMessage, &[]);
        assert_eq!(shortcut_of(&palette, "mail.links").as_deref(), Some("L"));
        assert_eq!(shortcut_of(&palette, "shell.compose").as_deref(), Some("c"));

        // Back in the mailbox list, the list's keys.
        palette.toggle_with_archive_operations(UiContext::MailboxMessage, &[]); // close
        palette.toggle_with_archive_operations(UiContext::MailboxList, &[]); // reopen
        assert_eq!(shortcut_of(&palette, "shell.compose").as_deref(), Some("c"));
        assert_eq!(shortcut_of(&palette, "mail.archive").as_deref(), Some("e"));
        assert_eq!(
            shortcut_of(&palette, "nav.inbox").as_deref(),
            Some("gi / g0")
        );
    }

    #[test]
    fn rules_context_hides_mail_only_commands() {
        let ids: Vec<&str> = commands_for_context(UiContext::RulesList)
            .into_iter()
            .filter_map(|command| command.id)
            .collect();

        assert!(ids.contains(&"rules.new"));
        assert!(!ids.contains(&"mail.label"));
        assert!(!ids.contains(&"mail.archive"));
    }

    #[test]
    fn restore_recents_round_trip_preserves_order_and_skips_unknown_labels() {
        let mut palette = CommandPalette::default();
        // Confirm three real commands so we have known recents to snapshot.
        palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);
        palette.input = "archive".to_string();
        palette.update_filtered(None);
        palette.confirm().expect("archive command should exist");
        palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);
        palette.input = "reply all".to_string();
        palette.update_filtered(None);
        palette.confirm().expect("reply all command should exist");
        palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);
        palette.input = "star".to_string();
        palette.update_filtered(None);
        palette.confirm().expect("star command should exist");

        let snapshot = palette.recent_labels_snapshot();
        // Most-recent-first: Star, Reply All, Archive
        assert_eq!(
            snapshot,
            vec![
                "Star or unstar".to_string(),
                "Reply all".to_string(),
                "Archive".to_string(),
            ],
            "snapshot order must match record_recent semantics"
        );

        // Restore on a fresh palette, with a label that no longer exists
        // in the middle. The unknown label must be dropped silently while
        // the surrounding entries survive in order.
        let persisted = vec![
            "Star or unstar".to_string(),
            "RemovedInThisVersion".to_string(),
            "Reply all".to_string(),
            "Archive".to_string(),
        ];
        let mut fresh = CommandPalette::default();
        fresh.restore_recents_from_labels(&persisted);

        let restored_snapshot = fresh.recent_labels_snapshot();
        assert_eq!(
            restored_snapshot,
            vec![
                "Star or unstar".to_string(),
                "Reply all".to_string(),
                "Archive".to_string(),
            ],
            "restored recents must skip unknown labels and preserve order"
        );
    }

    #[test]
    fn filtering_preserves_selected_command_identity() {
        let mut palette = CommandPalette::default();
        palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);
        palette.selected = palette
            .filtered
            .iter()
            .position(|&index| palette.commands[index].label == "Search page")
            .unwrap();

        palette.on_char('p');
        palette.on_char('a');

        let selected = palette
            .filtered
            .get(palette.selected)
            .map(|&index| palette.commands[index].label.clone());

        assert_eq!(selected.as_deref(), Some("Search page"));
    }

    #[test]
    fn archive_recovery_discovery_does_not_create_journal_storage() {
        let data_dir = tempfile::tempdir().unwrap();
        let operations = archive_operations(data_dir.path());

        assert!(operations.is_empty());
        assert!(!data_dir.path().join("archive-operations").exists());
    }
}
