use super::centered_rect;
use crate::app::{snooze_presets, NotNowTarget, SnoozePanelState, SnoozePreset};
use mxr_config::SnoozeConfig;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub const CUSTOM_ROW_LABEL: &str = "Custom… (e.g. tomorrow 9am, in 2h)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotNowRow {
    Later,
    Preset(SnoozePreset),
    Custom,
}

pub struct NotNowRows {
    later: bool,
    presets: Vec<SnoozePreset>,
}

impl NotNowRows {
    pub fn of(target: &NotNowTarget) -> Self {
        Self {
            later: target.later_label().is_some(),
            presets: snooze_presets().to_vec(),
        }
    }

    pub fn len(&self) -> usize {
        usize::from(self.later) + self.presets.len() + 1
    }

    pub fn preset(&self, nth: usize) -> Option<SnoozePreset> {
        nth.checked_sub(1)
            .and_then(|index| self.presets.get(index))
            .copied()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn row(&self, index: usize) -> NotNowRow {
        let presets = &self.presets;
        let index = if self.later {
            match index.checked_sub(1) {
                None => return NotNowRow::Later,
                Some(index) => index,
            }
        } else {
            index
        };
        presets
            .get(index)
            .copied()
            .map_or(NotNowRow::Custom, NotNowRow::Preset)
    }
}

pub fn rows_of(panel: &SnoozePanelState) -> NotNowRows {
    let mut rows = panel.target.as_ref().map_or(
        NotNowRows {
            later: false,
            presets: Vec::new(),
        },
        NotNowRows::of,
    );
    rows.presets = panel
        .presets
        .clone()
        .unwrap_or_else(|| snooze_presets().to_vec());
    rows
}

pub fn rows_count(panel: &SnoozePanelState) -> usize {
    rows_of(panel).len()
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    panel: &SnoozePanelState,
    config: &SnoozeConfig,
    theme: &crate::theme::Theme,
) {
    if !panel.visible {
        return;
    }

    let popup = centered_rect(50, 60, area);
    frame.render_widget(Clear, popup);

    let title = panel
        .target
        .as_ref()
        .map_or(" Not now ", NotNowTarget::title);
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if panel.custom_input.is_some() {
        draw_custom_input(frame, inner, panel, theme);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    let rows = rows_of(panel);
    let later_label = panel.target.as_ref().and_then(NotNowTarget::later_label);
    let mut preset_number = 0;
    let items: Vec<ListItem> = (0..rows.len())
        .map(|index| {
            let (key, label, muted) = match rows.row(index) {
                NotNowRow::Later => (
                    "\u{21b5}".to_string(),
                    later_label.unwrap_or("Later").to_string(),
                    false,
                ),
                NotNowRow::Preset(preset) => {
                    preset_number += 1;
                    (
                        preset_number.to_string(),
                        format_preset(preset, config),
                        false,
                    )
                }
                NotNowRow::Custom => (" ".to_string(), CUSTOM_ROW_LABEL.to_string(), true),
            };
            let style = if index == panel.selected_index {
                theme.highlight_style()
            } else if muted {
                Style::default().fg(theme.text_secondary)
            } else {
                Style::default()
            };
            ListItem::new(format!(" {key}  {label}")).style(style)
        })
        .collect();

    let total = rows.len();
    let list_height = chunks[0].height as usize;
    frame.render_widget(List::new(items), chunks[0]);

    if total > list_height {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(list_height)).position(panel.selected_index);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.warning)),
            chunks[0],
            &mut scrollbar_state,
        );
    }

    frame.render_widget(
        Paragraph::new(format!(
            "Enter pick  1-{} a time  j/k move  Esc cancel",
            rows.presets.len()
        ))
        .style(Style::default().fg(theme.text_secondary)),
        chunks[1],
    );
}

fn draw_custom_input(
    frame: &mut Frame,
    inner: Rect,
    panel: &SnoozePanelState,
    theme: &crate::theme::Theme,
) {
    let input = panel.custom_input.as_deref().unwrap_or("");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new("Type a custom time:").style(Style::default().fg(theme.text_primary)),
        chunks[0],
    );
    let prompt = format!(" › {input}_");
    frame.render_widget(
        Paragraph::new(prompt).style(Style::default().fg(theme.text_primary)),
        chunks[1],
    );
    let hint = "Examples: fri 3 · tomorrow 9am · in 2h · next week · 3 oct";
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(theme.text_secondary)),
        chunks[2],
    );
    if let Some(error) = &panel.custom_error {
        frame.render_widget(
            Paragraph::new(format!("Error: {error}")).style(Style::default().fg(theme.error)),
            chunks[3],
        );
    } else {
        let style = if panel.custom_time.is_resolved() {
            Style::default().fg(theme.text_primary)
        } else {
            Style::default().fg(theme.text_secondary)
        };
        let preview = panel
            .custom_time
            .lines()
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(preview)
                .style(style)
                .wrap(Wrap { trim: false }),
            chunks[3],
        );
    }
    frame.render_widget(
        Paragraph::new("Enter set  Tab other reading  Esc back to the list")
            .style(Style::default().fg(theme.text_secondary)),
        chunks[4],
    );
}

fn format_preset(preset: SnoozePreset, config: &SnoozeConfig) -> String {
    mxr_config::snooze::format_preset(preset, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_test_support::render_to_string;

    fn custom_row_index() -> usize {
        snooze_presets().len()
    }

    #[test]
    fn later_comes_first_then_the_presets_then_custom() {
        let mail = NotNowRows::of(&NotNowTarget::Mail {
            message_id: mxr_core::id::MessageId::new(),
        });
        assert_eq!(mail.len(), snooze_presets().len() + 2);
        assert_eq!(mail.row(0), NotNowRow::Later);
        assert_eq!(mail.row(1), NotNowRow::Preset(snooze_presets()[0]));
        assert_eq!(mail.row(mail.len() - 1), NotNowRow::Custom);
        let todo = NotNowRows::of(&NotNowTarget::Todo {
            todo_id: "t1".into(),
        });
        assert_eq!(todo.len(), snooze_presets().len() + 1);
        assert_eq!(todo.row(0), NotNowRow::Preset(snooze_presets()[0]));
    }

    #[test]
    fn custom_mode_previews_the_resolved_time_live() {
        let mut panel = SnoozePanelState {
            visible: true,
            selected_index: custom_row_index(),
            custom_input: Some("in 2h".into()),
            ..Default::default()
        };
        panel.custom_time.update("in 2h", &SnoozeConfig::default());
        let snapshot = render_to_string(100, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 24),
                &panel,
                &SnoozeConfig::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("(in 2 hours)"),
            "resolution preview must appear before Enter; got:\n{snapshot}",
        );
    }
}
