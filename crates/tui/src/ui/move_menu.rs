//! The move menu `X` and `K` open, and the arrivals list Enter on Now's
//! line opens (D119). Pure render; wiring lives in `app/move_actions.rs`.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ArrivalsListState, MoveMenu};
use crate::ui::centered_rect_fixed_height;
use crate::ui::sanitize::{one_line, truncate};

/// "Move to…": one key per mode; for the sender, only the modes a sender
/// can live in.
pub fn draw_move_menu(
    frame: &mut Frame,
    area: Rect,
    menu: Option<&MoveMenu>,
    theme: &crate::theme::Theme,
) {
    let Some(menu) = menu else {
        return;
    };
    let popup = centered_rect_fixed_height(52, if menu.sender_only { 10 } else { 13 }, area);
    frame.render_widget(Clear, popup);
    let title = if menu.sender_only {
        " Everything from this sender goes to… "
    } else {
        " Move to… "
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = vec![
        Line::from(Span::styled(
            truncate(&one_line(&menu.display), 46),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (key, mode) in crate::app::MOVE_CHOICES {
        if menu.sender_only && !crate::app::sender_mode(mode) {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("  {key}  "), Style::default().fg(theme.accent)),
            Span::raw(mode.name()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if menu.sender_only {
            "From now on, for all their mail. Esc closes."
        } else {
            "This email only. M, U or R: all mail from this sender. Esc closes."
        },
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The emails behind one count of the arrivals line, newest first, each
/// with when it arrived, who sent it, the subject and where it went.
pub fn draw_arrivals_list(
    frame: &mut Frame,
    area: Rect,
    list: Option<&ArrivalsListState>,
    theme: &crate::theme::Theme,
) {
    let Some(state) = list else {
        return;
    };
    let popup = crate::ui::centered_rect(80, 70, area);
    frame.render_widget(Clear, popup);
    let which = state.bucket.map_or_else(
        || "Everything that arrived".to_string(),
        |bucket| match (bucket.mode(), bucket) {
            (Some(mode), _) => format!("Arrived in {}", mode.name()),
            (None, mxr_protocol::ArrivalBucketData::Spam) => "In Spam".to_string(),
            (None, mxr_protocol::ArrivalBucketData::ScreenedOut) => "Screened out".to_string(),
            (None, _) => "Still sorting".to_string(),
        },
    );
    let block = Block::bordered()
        .title(format!(" {which} "))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let width = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    match &state.list {
        None => lines.push(Line::from(Span::styled(
            "Loading…",
            Style::default().fg(theme.text_muted),
        ))),
        Some(list) if list.items.is_empty() => lines.push(Line::from(Span::styled(
            "None.",
            Style::default().fg(theme.text_muted),
        ))),
        Some(list) => {
            let shown = list.items.len();
            let total = list.total;
            lines.push(Line::from(Span::styled(
                if (shown as u32) < total {
                    format!("{total} emails, newest {shown} shown")
                } else if total == 1 {
                    "1 email".to_string()
                } else {
                    format!("{total} emails")
                },
                Style::default().fg(theme.text_secondary),
            )));
            for (i, item) in list.items.iter().enumerate() {
                let selected = i == state.selected;
                let when = item
                    .first_seen_at
                    .with_timezone(&chrono::Local)
                    .format("%a %H:%M")
                    .to_string();
                let who = one_line(item.sender_name.as_deref().unwrap_or(&item.sender_email));
                let chip = one_line(&item.chip);
                let chip_width = chip.chars().count().min(width / 3);
                let who_width = 18.min(width / 4);
                let subject_width =
                    width.saturating_sub(2 + 9 + 2 + who_width + 2 + chip_width + 2);
                let style = if selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                lines.push(Line::from(vec![
                    Span::styled(if selected { "\u{258c} " } else { "  " }, style),
                    Span::styled(format!("{when:<9}  "), style.fg(theme.text_muted)),
                    Span::styled(
                        format!("{:<who_width$}  ", truncate(&who, who_width)),
                        style.fg(theme.text_primary),
                    ),
                    Span::styled(
                        format!(
                            "{:<subject_width$}  ",
                            truncate(&one_line(&item.subject), subject_width)
                        ),
                        style.fg(theme.text_secondary),
                    ),
                    Span::styled(truncate(&chip, chip_width), style.fg(theme.text_muted)),
                ]));
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tab next count  Enter open  X move  K sender  u undo  Esc close",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{ArrivalBucketData, ArrivalItemData, ArrivalListData};
    use mxr_test_support::render_to_string;

    fn item(subject: &str) -> ArrivalItemData {
        ArrivalItemData {
            account_id: AccountId::new(),
            message_id: MessageId::new(),
            thread_id: ThreadId::new(),
            sender_email: "digest@weekly.example".into(),
            sender_name: Some("The Weekly".into()),
            subject: subject.into(),
            date: Utc::now(),
            first_seen_at: Utc::now(),
            arrived_in: ArrivalBucketData::Reading,
            bucket: ArrivalBucketData::Reading,
            reason: Some("has List-Unsubscribe".into()),
            chip: "→ Reading · has List-Unsubscribe".into(),
            moved: false,
            not_sure: false,
            unread: true,
            also_todo: false,
            also_archive: false,
        }
    }

    #[test]
    fn the_menu_offers_one_key_per_mode_and_the_sender_menu_three() {
        let menu = MoveMenu {
            message_id: MessageId::new(),
            display: "Maya Ortiz".into(),
            sender_only: false,
            not_sure: false,
        };
        let rendered = render_to_string(80, 20, |frame| {
            draw_move_menu(
                frame,
                Rect::new(0, 0, 80, 20),
                Some(&menu),
                &crate::theme::Theme::default(),
            );
        });
        for line in ["m  Messages", "x  To do", "u  Updates", "r  Reading", "e  Archive"] {
            assert!(rendered.contains(line), "{line}\n{rendered}");
        }
        let sender = MoveMenu {
            sender_only: true,
            ..menu
        };
        let rendered = render_to_string(80, 20, |frame| {
            draw_move_menu(
                frame,
                Rect::new(0, 0, 80, 20),
                Some(&sender),
                &crate::theme::Theme::default(),
            );
        });
        assert!(!rendered.contains("To do"), "{rendered}");
        assert!(!rendered.contains("Archive"), "{rendered}");
    }

    #[test]
    fn the_list_shows_each_email_with_where_it_went_and_no_control_text() {
        let state = ArrivalsListState {
            since: Utc::now(),
            until: Utc::now(),
            buckets: vec![ArrivalBucketData::Reading],
            bucket: Some(ArrivalBucketData::Reading),
            list: Some(ArrivalListData {
                since: Utc::now(),
                until: Utc::now(),
                bucket: Some(ArrivalBucketData::Reading),
                total: 2,
                items: vec![item("Issue 41"), item("Evil\u{1b}[31m subject\u{202e}")],
            }),
            selected: 0,
        };
        let rendered = render_to_string(120, 24, |frame| {
            draw_arrivals_list(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&state),
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Arrived in Reading"), "{rendered}");
        assert!(rendered.contains("2 emails"), "{rendered}");
        assert!(rendered.contains("Issue 41"), "{rendered}");
        assert!(rendered.contains("→ Reading"), "{rendered}");
        assert!(!rendered.contains('\u{1b}'));
        assert!(!rendered.contains('\u{202e}'));
    }
}
