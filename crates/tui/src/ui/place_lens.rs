//! Reading and Paper trail: mail that isn't from people, bundled by sender.
//!
//! Renders `Request::ListPlace` as one header per sender (name, count, and
//! why the sender is here) with the bundle's messages under it; one cursor
//! walks the messages. Nothing is styled or counted as unread. Also draws
//! the sweep preview and the "move sender to…" menu. Pure render; wiring
//! lives in `app/place_actions.rs`.

use mxr_protocol::{MailPlaceData, PlaceBundleData, PlaceMessageData, SenderKindData};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, PendingSweepConfirm, PlacePageState, SenderKindMenu};
use crate::ui::centered_rect_fixed_height;
use crate::ui::sanitize::{one_line, truncate};

pub struct PlaceView<'a> {
    pub page: &'a PlacePageState,
    pub place: MailPlaceData,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
}

pub(crate) const fn place_title(place: MailPlaceData) -> &'static str {
    match place {
        MailPlaceData::Reading => "Reading",
        MailPlaceData::PaperTrail => "Paper trail",
    }
}

const FOOTER: &str = "  enter open \u{b7} p pin \u{b7} K move sender \u{b7} S sweep sender \u{b7} A sweep all \u{b7} u undo";

pub fn draw(frame: &mut Frame, area: Rect, view: &PlaceView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let block = Block::bordered()
        .title(format!(" {} ", place_title(view.place)))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_line = 0usize;
    let mut row_index = 0usize;
    let page = view.page;

    if !page.loaded {
        lines.push(Line::from(Span::styled(
            format!("  Loading {}…", place_title(view.place)),
            Style::default().fg(theme.text_muted),
        )));
    } else if page.bundles.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {} is clear.", place_title(view.place)),
            Style::default().fg(theme.text_secondary),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "  {} from {}",
                plural(page.total_messages, "message", "messages"),
                plural(page.total_bundles, "sender", "senders"),
            ),
            Style::default().fg(theme.text_muted),
        )));
    }

    let more_senders = page.total_bundles as usize > page.bundles.len();

    for bundle in &page.bundles {
        lines.push(Line::from(""));
        lines.push(bundle_header(bundle, width, theme));
        lines.push(Line::from(Span::styled(
            format!("    here because: {}", one_line(&bundle.kind.reason)),
            Style::default().fg(theme.text_muted),
        )));
        for message in &bundle.messages {
            let selected = row_index == view.selected_index;
            if selected {
                selected_line = lines.len();
            }
            lines.push(message_line(message, selected, width, theme));
            row_index += 1;
        }
        let hidden = (bundle.message_count as usize).saturating_sub(bundle.messages.len());
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                format!("      and {hidden} more (+ to load)"),
                Style::default().fg(theme.text_muted),
            )));
        }
    }

    if more_senders {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "  {} more senders (> to load)",
                page.total_bundles as usize - page.bundles.len()
            ),
            Style::default().fg(theme.text_muted),
        )));
    }

    let footer_height = 1u16;
    let body_height = inner.height.saturating_sub(footer_height) as usize;
    let scroll = selected_line.saturating_sub(body_height.saturating_sub(2));
    let body_area = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(footer_height),
        height: footer_height,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            FOOTER,
            Style::default().fg(theme.text_muted),
        ))),
        footer_area,
    );
}

fn bundle_header(
    bundle: &PlaceBundleData,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let who = one_line(
        bundle
            .sender_name
            .as_deref()
            .unwrap_or(&bundle.sender_email),
    );
    let count = bundle.message_count.to_string();
    let pinned = if bundle.pinned_count > 0 {
        format!(" \u{b7} {} pinned", bundle.pinned_count)
    } else {
        String::new()
    };
    let who_width = width.saturating_sub(4 + count.len() + pinned.len()).max(8);
    Line::from(vec![
        Span::styled(
            format!("  {}", truncate(&who, who_width)),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {count}"), Style::default().fg(theme.text_muted)),
        Span::styled(pinned, Style::default().fg(theme.accent)),
    ])
}

fn message_line(
    message: &PlaceMessageData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let marker = if selected { "\u{258c} " } else { "  " };
    let pin = if message.pinned { "\u{2022} " } else { "  " };
    let date = message.date.format("%-d %b").to_string();
    // marker (2) + indent (2) + pin (2) + subject + gap (2) + date
    let subject_width = width
        .saturating_sub(2 + 2 + 2 + 2 + date.chars().count())
        .max(8);
    let mut spans = vec![
        Span::raw(marker),
        Span::raw("  "),
        Span::styled(pin, Style::default().fg(theme.accent)),
        Span::styled(
            format!(
                "{:<subject_width$}  ",
                truncate(&one_line(&message.subject), subject_width)
            ),
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(date, Style::default().fg(theme.text_muted)),
    ];
    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
    Line::from(spans)
}

/// The sweep preview: what the daemon's dry run says will be archived.
pub fn draw_sweep_confirm(
    frame: &mut Frame,
    area: Rect,
    confirm: Option<&PendingSweepConfirm>,
    theme: &crate::theme::Theme,
) {
    let Some(confirm) = confirm else {
        return;
    };
    let preview = &confirm.preview;
    // Sized to its content: a count, up to five subjects, the keys.
    let height = u16::try_from(preview.sample_subjects.len()).unwrap_or(5) + 10;
    let popup = centered_rect_fixed_height(64, height, area);
    frame.render_widget(Clear, popup);
    let scope = match &confirm.target.sender_email {
        Some(sender) => format!("{sender} in {}", place_title(confirm.target.place)),
        None => format!("all of {}", place_title(confirm.target.place)),
    };
    let block = Block::bordered()
        .title(format!(" Sweep {} ", one_line(&scope)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // A sweep reaches mail the lens has not loaded yet: say how much.
    let shown = if confirm.shown < preview.count {
        format!(" ({} shown here)", confirm.shown)
    } else {
        String::new()
    };
    let mut lines = vec![Line::from(format!(
        "Archive {}{shown}{}.",
        plural(preview.count, "message", "messages"),
        if preview.pinned_excluded > 0 {
            format!(
                "; {} stays",
                plural(preview.pinned_excluded, "pinned message", "pinned messages")
            )
        } else {
            String::new()
        }
    ))];
    if confirm.target.sender_email.is_none() {
        lines.push(Line::from(Span::styled(
            format!(
                "From {}",
                plural(preview.senders.len() as u32, "sender", "senders")
            ),
            Style::default().fg(theme.text_muted),
        )));
    }
    lines.push(Line::from(""));
    for subject in &preview.sample_subjects {
        lines.push(Line::from(Span::styled(
            format!("  {}", truncate(&one_line(subject), 56)),
            Style::default().fg(theme.text_secondary),
        )));
    }
    lines.push(Line::from(""));
    let button = |label: String, focused: bool| {
        let style = if focused {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_secondary)
        };
        Span::styled(format!("[ {label} ]"), style)
    };
    lines.push(Line::from(vec![
        button("Cancel".into(), !confirm.sweep_focused),
        Span::raw("  "),
        button(sweep_confirm_label(confirm), confirm.sweep_focused),
    ]));
    lines.push(Line::from(Span::styled(
        "Tab switches, Enter chooses, Esc cancels; u undoes a sweep",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The confirm button names the scope, so the whole place never reads
/// like one sender's bundle: "Archive all 143 from 35 senders".
fn sweep_confirm_label(confirm: &PendingSweepConfirm) -> String {
    let preview = &confirm.preview;
    if confirm.target.sender_email.is_some() {
        return format!("Archive {}", plural(preview.count, "message", "messages"));
    }
    format!(
        "Archive all {} from {}",
        preview.count,
        plural(preview.senders.len() as u32, "sender", "senders")
    )
}

/// The "move sender to…" menu. Its keys: p people, r Reading, t Paper
/// trail, x screened out, a automatic.
pub const SENDER_KIND_CHOICES: [(char, Option<SenderKindData>, &str); 5] = [
    ('p', Some(SenderKindData::People), "People: on the desk"),
    ('r', Some(SenderKindData::Reading), "Reading"),
    ('t', Some(SenderKindData::PaperTrail), "Paper trail"),
    ('x', Some(SenderKindData::ScreenedOut), "Screened out"),
    ('a', None, "Automatic: let the rules decide"),
];

pub fn draw_sender_kind_menu(
    frame: &mut Frame,
    area: Rect,
    menu: Option<&SenderKindMenu>,
    theme: &crate::theme::Theme,
) {
    let Some(menu) = menu else {
        return;
    };
    let popup = centered_rect_fixed_height(52, 14, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Move sender to… ")
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
        Line::from(Span::styled(
            format!("here because: {}", one_line(&menu.current.reason)),
            Style::default().fg(theme.text_muted),
        )),
        Line::from(""),
    ];
    for (key, kind, label) in SENDER_KIND_CHOICES {
        let current = kind == Some(menu.current.kind) && menu.current.corrected
            || kind.is_none() && !menu.current.corrected;
        lines.push(Line::from(vec![
            Span::styled(format!("  {key}  "), Style::default().fg(theme.accent)),
            Span::raw(label),
            Span::styled(
                if current { "  (now)" } else { "" },
                Style::default().fg(theme.text_muted),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Remembered for this sender's future mail. Esc closes.",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::SweepTarget;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{KindRuleData, MailKindData, SweepPreviewData};
    use mxr_test_support::render_to_string;

    fn bundle(name: &str, subjects: &[&str], pinned: usize) -> PlaceBundleData {
        PlaceBundleData {
            account_id: AccountId::new(),
            sender_email: format!("{}@shop.example", name.to_lowercase()),
            sender_name: Some(name.into()),
            kind: MailKindData {
                kind: SenderKindData::PaperTrail,
                rule: KindRuleData::AutomatedAddress,
                reason: "automated sender, has List-Unsubscribe".into(),
                corrected: false,
            },
            message_count: subjects.len() as u32 + 2,
            unread_count: 3,
            pinned_count: u32::from(pinned > 0),
            newest_at: chrono::Utc::now(),
            newest_subject: subjects[0].into(),
            messages: subjects
                .iter()
                .enumerate()
                .map(|(index, subject)| PlaceMessageData {
                    message_id: MessageId::new(),
                    thread_id: ThreadId::new(),
                    subject: (*subject).into(),
                    snippet: String::new(),
                    date: chrono::Utc::now(),
                    unread: true,
                    pinned: index < pinned,
                    starred: false,
                })
                .collect(),
        }
    }

    fn page(bundles: Vec<PlaceBundleData>) -> PlacePageState {
        PlacePageState {
            place: Some(MailPlaceData::PaperTrail),
            total_bundles: bundles.len() as u32,
            total_messages: bundles.iter().map(|b| b.message_count).sum(),
            bundles,
            loaded: true,
        }
    }

    fn render(page: &PlacePageState) -> String {
        render_to_string(100, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 20),
                &PlaceView {
                    page,
                    place: MailPlaceData::PaperTrail,
                    selected_index: 1,
                    active_pane: &ActivePane::MailList,
                },
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn bundles_say_who_how_many_and_why_without_unread_counts() {
        let rendered = render(&page(vec![bundle("Shop", &["Receipt 2", "Receipt 1"], 1)]));
        assert!(rendered.contains("Paper trail"), "{rendered}");
        assert!(rendered.contains("4 messages from 1 sender"));
        assert!(rendered.contains("Shop  4 \u{b7} 1 pinned"));
        assert!(rendered.contains("here because: automated sender, has List-Unsubscribe"));
        assert!(rendered.contains("\u{2022} Receipt 2"), "pinned marker");
        assert!(rendered.contains("and 2 more (+ to load)"));
        assert!(!rendered.to_lowercase().contains("unread"));
        assert!(!rendered.contains('\u{2014}'), "no em dashes");
    }

    #[test]
    fn an_empty_place_is_clear_and_mail_text_is_sanitised() {
        assert!(render(&page(vec![])).contains("Paper trail is clear."));
        let hostile = bundle("Eve\u{1b}]0;pwned\u{7}", &["Invoice\u{1b}[2J\r\nnow"], 0);
        let rendered = render(&page(vec![hostile]));
        assert!(!rendered.chars().any(|c| c == '\u{1b}' || c == '\u{7}'));
    }

    #[test]
    fn the_sweep_preview_names_the_count_and_the_pins_that_stay() {
        let confirm = PendingSweepConfirm {
            target: SweepTarget {
                place: MailPlaceData::PaperTrail,
                account_id: None,
                sender_email: Some("receipts@shop.example".into()),
            },
            preview: SweepPreviewData {
                place: MailPlaceData::PaperTrail,
                sender_email: Some("receipts@shop.example".into()),
                count: 13,
                pinned_excluded: 1,
                senders: vec![],
                sample_subjects: vec!["Receipt 14".into()],
                preview_token: Some("tok".into()),
            },
            shown: 1,
            sweep_focused: true,
        };
        let rendered = render_to_string(100, 30, |frame| {
            draw_sweep_confirm(
                frame,
                Rect::new(0, 0, 100, 30),
                Some(&confirm),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            rendered.contains("Archive 13 messages (1 shown here); 1 pinned message stays."),
            "{rendered}"
        );
        assert!(rendered.contains("Receipt 14"));
    }
}
