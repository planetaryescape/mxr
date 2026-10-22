//! Messages' filter picker (Ctrl-f): the turn as a radio group, then Unread
//! and Past your pace as toggles. Each choice applies at once, so the modal
//! only shows what the page holds and where the cursor sits.

use super::centered_rect;
use crate::app::MessagesFilterPicker;
use crate::theme::Theme;
use mxr_protocol::{MessagesFilterData, MessagesTurnData};
use ratatui::prelude::*;
use ratatui::widgets::*;

const TURN_ROWS: [(&str, Option<MessagesTurnData>); 3] = [
    ("All", None),
    ("Your turn", Some(MessagesTurnData::Mine)),
    ("Waiting on them", Some(MessagesTurnData::Theirs)),
];

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    picker: Option<MessagesFilterPicker>,
    filter: MessagesFilterData,
    theme: &Theme,
) {
    let Some(picker) = picker else { return };

    let popup_area = centered_rect(44, 40, area);
    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" Messages filter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut lines: Vec<Line> = TURN_ROWS
        .iter()
        .enumerate()
        .map(|(row, (label, turn))| {
            let mark = if filter.turn == *turn { "(•)" } else { "( )" };
            row_line(row, picker.cursor, format!("{mark} {label}"), theme)
        })
        .collect();
    lines.push(Line::from(""));
    for (row, (label, on)) in [
        (3, ("Unread", filter.unread)),
        (4, ("Past your pace", filter.past_pace)),
    ] {
        let mark = if on { "[x]" } else { "[ ]" };
        lines.push(row_line(
            row,
            picker.cursor,
            format!("{mark} {label}"),
            theme,
        ));
    }
    lines.push(row_line(
        5,
        picker.cursor,
        "Clear filters".to_string(),
        theme,
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k move   Enter pick or toggle   Esc close",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn row_line(row: usize, cursor: usize, text: String, theme: &Theme) -> Line<'static> {
    let style = if row == cursor {
        Style::default().fg(theme.accent).bold()
    } else {
        Style::default()
    };
    Line::from(Span::styled(text, style))
}
