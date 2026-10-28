//! The move menu `X` and `P` open, and the arrivals list Enter on Now's
//! line opens (D119). Pure render; wiring lives in `app/move_actions.rs`.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ArrivalsListState, MoveMenu, MoveOption};
use crate::ui::centered_rect_fixed_height;
use crate::ui::sanitize::{one_line, truncate};

/// "Move to…": each option prints its key (D168), a lowercase letter for
/// this email and a capital for its sender; the highlighted one runs on
/// Enter. On a new sender's row the capitals answer its question.
pub fn draw_move_menu(
    frame: &mut Frame,
    area: Rect,
    menu: Option<&MoveMenu>,
    theme: &crate::theme::Theme,
) {
    let Some(menu) = menu else {
        return;
    };
    let options = menu.options();
    let height = u16::try_from(options.len())
        .unwrap_or(u16::MAX)
        .saturating_add(7);
    let popup = centered_rect_fixed_height(56, height, area);
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
            truncate(&one_line(&menu.display), 50),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (index, option) in options.iter().enumerate() {
        let label = match option {
            MoveOption::Email(mode) => {
                if menu.current == Some(*mode) {
                    format!("{} (keep it here)", mode.name())
                } else {
                    mode.name().to_string()
                }
            }
            MoveOption::Sender(mode) => format!("{}: all their mail", mode.name()),
            MoveOption::NewSender(kind) => menu
                .new_sender
                .as_ref()
                .and_then(|question| question.choices.iter().find(|choice| choice.kind == *kind))
                .map_or_else(String::new, |choice| {
                    format!("New sender: {}", one_line(&choice.label))
                }),
        };
        let row = Line::from(vec![
            Span::styled(
                format!("  {}  ", option.key()),
                Style::default().fg(theme.accent),
            ),
            Span::raw(label),
        ]);
        lines.push(if index == menu.selected {
            row.style(theme.highlight_style())
        } else {
            row
        });
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if menu.sender_only {
            "From now on, for all their mail. Enter picks, Esc closes."
        } else {
            "Lowercase: this email. Capital: the sender. Enter picks, Esc closes."
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
            (None, mxr_protocol::ArrivalBucketData::Trash) => "In Trash".to_string(),
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
            // The count line above and the blank and key lines below stay;
            // the rows between scroll so the selection is always on screen.
            let visible = (inner.height as usize).saturating_sub(3).max(1);
            let offset = (state.selected + 1).saturating_sub(visible);
            for (i, item) in list.items.iter().enumerate().skip(offset).take(visible) {
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

    fn menu() -> MoveMenu {
        MoveMenu {
            message_id: MessageId::new(),
            display: "Maya Ortiz".into(),
            sender_only: false,
            not_sure: false,
            current: None,
            new_sender: None,
            selected: 0,
        }
    }

    fn render(menu: &MoveMenu) -> String {
        render_to_string(80, 24, |frame| {
            draw_move_menu(
                frame,
                Rect::new(0, 0, 80, 24),
                Some(menu),
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn the_menu_prints_each_options_key_and_the_sender_menu_three() {
        let rendered = render(&menu());
        for line in [
            "m  Messages",
            "x  To do",
            "u  Updates",
            "r  Reading",
            "e  Archive",
            "M  Messages: all their mail",
            "U  Updates: all their mail",
            "R  Reading: all their mail",
        ] {
            assert!(rendered.contains(line), "{line}\n{rendered}");
        }
        let sender = MoveMenu {
            sender_only: true,
            ..menu()
        };
        let rendered = render(&sender);
        assert!(!rendered.contains("To do"), "{rendered}");
        assert!(!rendered.contains("Archive"), "{rendered}");
        assert!(rendered.contains("M  Messages"), "{rendered}");
    }

    #[test]
    fn a_new_sender_answers_with_capitals_and_block_on_d() {
        let question = mxr_protocol::ScreenerQuestionData {
            account_id: AccountId::new(),
            sender_email: "iris@example.com".into(),
            question: "New sender. Keep in Messages?".into(),
            choices: [
                (mxr_protocol::SenderKindData::People, "Messages"),
                (mxr_protocol::SenderKindData::PaperTrail, "Updates"),
                (mxr_protocol::SenderKindData::Reading, "Reading"),
                (mxr_protocol::SenderKindData::ScreenedOut, "Block"),
            ]
            .into_iter()
            .map(|(kind, label)| mxr_protocol::ScreenerChoiceData {
                kind,
                label: label.into(),
            })
            .collect(),
        };
        let rendered = render(&MoveMenu {
            new_sender: Some(question),
            ..menu()
        });
        for line in [
            "m  Messages",
            "M  New sender: Messages",
            "U  New sender: Updates",
            "R  New sender: Reading",
            "D  New sender: Block",
        ] {
            assert!(rendered.contains(line), "{line}\n{rendered}");
        }
    }

    #[test]
    fn a_not_sure_question_marks_where_it_is_now() {
        let rendered = render(&MoveMenu {
            not_sure: true,
            current: Some(mxr_protocol::ModeKindData::Updates),
            selected: 2,
            ..menu()
        });
        assert!(rendered.contains("u  Updates (keep it here)"), "{rendered}");
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

    #[test]
    fn the_list_scrolls_to_keep_the_selection_on_screen() {
        let items: Vec<ArrivalItemData> = (0..30).map(|n| item(&format!("Issue {n:02}"))).collect();
        let at = |selected: usize| ArrivalsListState {
            since: Utc::now(),
            until: Utc::now(),
            buckets: vec![ArrivalBucketData::Reading],
            bucket: Some(ArrivalBucketData::Reading),
            list: Some(ArrivalListData {
                since: Utc::now(),
                until: Utc::now(),
                bucket: Some(ArrivalBucketData::Reading),
                total: 30,
                items: items.clone(),
            }),
            selected,
        };
        let render = |selected: usize| {
            render_to_string(100, 16, |frame| {
                draw_arrivals_list(
                    frame,
                    Rect::new(0, 0, 100, 16),
                    Some(&at(selected)),
                    &crate::theme::Theme::default(),
                );
            })
        };
        let top = render(0);
        assert!(top.contains("Issue 00"), "{top}");
        assert!(!top.contains("Issue 29"), "{top}");
        // Far down the list, the selected row is drawn and the first isn't.
        for selected in [9, 20, 29] {
            let rendered = render(selected);
            let marker = rendered
                .lines()
                .find(|line| line.contains('\u{258c}'))
                .unwrap_or_else(|| panic!("no selected row for {selected}\n{rendered}"));
            assert!(
                marker.contains(&format!("Issue {selected:02}")),
                "{rendered}"
            );
            assert!(rendered.contains("Tab next count"), "{rendered}");
        }
        assert!(!render(29).contains("Issue 00"));
    }
}
