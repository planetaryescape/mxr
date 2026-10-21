use super::centered_rect;
use crate::app::PendingBulkConfirm;
use crate::ui::sanitize::truncate;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    pending: Option<&PendingBulkConfirm>,
    theme: &crate::theme::Theme,
) {
    let Some(pending) = pending else {
        return;
    };

    let popup = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(format!(" {} ", pending.title))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let width = usize::from(inner.width);
    let preview = &pending.preview;
    let not_in_view = preview.not_in_view;
    // Detail, a blank line, a blank line and the key hint take four rows; the
    // rest is list. One row is kept for "and K more" when the list overflows.
    let fixed_rows = 4 + usize::from(not_in_view > 0);
    let list_rows = usize::from(inner.height).saturating_sub(fixed_rows);
    let overflows = preview.rows.len() > list_rows;
    let visible = if overflows {
        list_rows.saturating_sub(1)
    } else {
        list_rows
    };
    let start = pending
        .preview_scroll
        .min(preview.rows.len().saturating_sub(visible));
    let end = (start + visible).min(preview.rows.len());

    let mut lines = vec![Line::from(pending.detail.clone()), Line::from("")];
    for row in &preview.rows[start..end] {
        lines.push(Line::from(truncate(
            &format!("{} — {}", row.sender, row.subject),
            width,
        )));
    }
    if overflows {
        lines.push(Line::from(format!(
            "and {} more (j/k to scroll)",
            preview.rows.len() - end
        )));
    }
    if not_in_view > 0 {
        lines.push(Line::from(format!("{not_in_view} not in this view")));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("[Enter] confirm   [y] confirm   [Esc] cancel"));
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::app::{BulkPreview, MutationEffect, PendingBulkConfirm};
    use mxr_protocol::{MutationCommand, Request};

    #[test]
    fn pending_bulk_confirm_is_constructible() {
        let pending = PendingBulkConfirm {
            title: "Archive messages".into(),
            detail: "You are about to archive these 15 messages.".into(),
            request: Request::mutation(MutationCommand::Archive {
                message_ids: vec![],
            }),
            effect: MutationEffect::RefreshList,
            optimistic_effect: None,
            status_message: "Archiving...".into(),
            preview: BulkPreview::default(),
            preview_scroll: 0,
        };
        let _ = draw;
        assert!(pending.detail.contains("15"));
    }
}
