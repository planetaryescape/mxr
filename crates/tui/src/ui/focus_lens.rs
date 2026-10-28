//! Focus & reply: the conversation on screen, why it is here and its
//! newest message from them, under a count and a thin progress bar. At the
//! end it says what's left, as the web's finish does. Pure render; wiring
//! lives in `app/focus_actions.rs`, and the keys are in the hint bar.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{FocusPageState, FocusProgress};
use crate::ui::sanitize::{one_line, strip_control_chars};

pub struct FocusView<'a> {
    pub page: &'a FocusPageState,
    pub focused: bool,
    /// The newest message's text, reader-cleaned; `None` while it loads.
    pub body: Option<String>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `██████░░░░░░` for how many are handled.
fn bar(progress: FocusProgress, width: usize) -> String {
    let filled = (width * progress.done)
        .checked_div(progress.total)
        .unwrap_or(0);
    format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(width.saturating_sub(filled))
    )
}

/// The end of a session: only claim everyone when nobody is left anywhere.
fn finish_lines(page: &FocusPageState, theme: &crate::theme::Theme) -> Vec<Line<'static>> {
    let progress = page.progress();
    let deferred = page.deferred.len();
    let title = if page.more > 0 {
        "That's this batch.".to_string()
    } else if deferred > 0 {
        format!(
            "{} skipped.",
            plural(deferred, "conversation", "conversations")
        )
    } else if progress.total == 0 {
        "Nobody you write with is waiting on a reply.".to_string()
    } else {
        "Low tide. That's everyone you write with.".to_string()
    };
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            title,
            Style::default().fg(theme.success).bold(),
        )),
        Line::from(""),
    ];
    let muted = Style::default().fg(theme.text_muted);
    if progress.done > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "{} handled.",
                plural(progress.done, "conversation", "conversations")
            ),
            muted,
        )));
    }
    if page.more > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "{} waiting on a reply.",
                plural(
                    page.more as usize,
                    "more conversation",
                    "more conversations"
                )
            ),
            muted,
        )));
    }
    if page.others > 0 {
        let others = page.others as usize;
        lines.push(Line::from(Span::styled(
            format!(
                "{} who wrote to you {} in Messages.",
                plural(others, "other", "others"),
                if others == 1 { "is" } else { "are" }
            ),
            muted,
        )));
    }
    if deferred > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "j comes back to {}.",
                if deferred == 1 { "it" } else { "them" }
            ),
            muted,
        )));
    }
    lines
}

pub fn draw(frame: &mut Frame, area: Rect, view: &FocusView<'_>, theme: &crate::theme::Theme) {
    let page = view.page;
    let progress = page.progress();
    let current = page.current();
    let title = match current {
        Some(item) => format!(
            " Focus & reply \u{2500} {} of {} \u{b7} {} ",
            progress.position,
            progress.total,
            one_line(&item.sender)
        ),
        None => " Focus & reply ".to_string(),
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(view.focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;
    let muted = Style::default().fg(theme.text_muted);

    let mut lines: Vec<Line<'static>> = vec![Line::from(Span::styled(
        bar(progress, width),
        Style::default().fg(theme.accent),
    ))];

    // A failed source can't prove nobody is waiting: that is an error to
    // retry, never a finish.
    let loading = !page.loaded;
    match (current, &page.error) {
        _ if loading => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Finding everyone you owe…", muted)));
        }
        (None, Some(error)) => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Couldn't load who you owe a reply: {}", one_line(error)),
                Style::default().fg(theme.error),
            )));
            lines.push(Line::from(Span::styled("g F tries again.", muted)));
        }
        (None, None) => lines.extend(finish_lines(page, theme)),
        (Some(item), error) => {
            if let Some(error) = error {
                lines.push(Line::from(Span::styled(
                    format!(
                        "Some of who you owe couldn't be loaded: {}. g F tries again.",
                        one_line(error)
                    ),
                    Style::default().fg(theme.error),
                )));
            }
            lines.push(Line::from(""));
            let subject = if item.subject.trim().is_empty() {
                "(no subject)".to_string()
            } else {
                one_line(&item.subject)
            };
            lines.push(Line::from(Span::styled(
                subject,
                Style::default().fg(theme.text_primary).bold(),
            )));
            lines.push(Line::from(Span::styled(one_line(&item.reason), muted)));
            lines.push(Line::from(""));
            match &view.body {
                Some(body) => {
                    // Email text is untrusted: no control sequence reaches
                    // the terminal.
                    for line in strip_control_chars(body).lines() {
                        lines.push(Line::from(line.to_string()));
                    }
                }
                None => lines.push(Line::from(Span::styled("Loading the email…", muted))),
            }
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FocusItem, FocusSource};
    use mxr_core::{AccountId, MessageId, ThreadId};
    use mxr_test_support::render_to_string;

    fn item(sender: &str) -> FocusItem {
        FocusItem {
            thread_id: ThreadId::new(),
            message_id: MessageId::new(),
            account_id: AccountId::new(),
            sender: sender.into(),
            subject: "Lunch on Friday?".into(),
            reason: "Replied to your message \u{b7} 2d \u{b7} usually 4h".into(),
            source: FocusSource::Owed,
        }
    }

    fn render(page: &FocusPageState, body: Option<&str>) -> String {
        let theme = crate::theme::Theme::default();
        render_to_string(70, 14, |frame| {
            draw(
                frame,
                frame.area(),
                &FocusView {
                    page,
                    focused: true,
                    body: body.map(str::to_string),
                },
                &theme,
            );
        })
    }

    #[test]
    fn the_conversation_on_screen_shows_its_place_reason_and_email() {
        let mut page = FocusPageState::start(None, false, None);
        page.sync(vec![item("Nora"), item("Sam")]);
        let rendered = render(&page, Some("Are you free?\x1b[2J"));
        assert!(rendered.contains("1 of 2 \u{b7} Nora"), "{rendered}");
        assert!(rendered.contains("Lunch on Friday?"));
        assert!(rendered.contains("usually 4h"));
        assert!(rendered.contains("Are you free?"));
        assert!(!rendered.contains('\x1b'));
    }

    #[test]
    fn the_finish_says_what_was_handled_and_who_waits_elsewhere() {
        let first = item("Nora");
        let mut page = FocusPageState::start(None, false, None);
        page.sync(vec![first.clone()]);
        page.handled(&first.thread_id);
        page.others = 2;
        let rendered = render(&page, None);
        assert!(rendered.contains("Low tide. That's everyone you write with."));
        assert!(rendered.contains("1 conversation handled."));
        assert!(rendered.contains("2 others who wrote to you are in Messages."));
    }

    #[test]
    fn a_failed_source_with_nobody_left_is_an_error_not_a_finish() {
        let mut page = FocusPageState::start(None, false, None);
        page.error = Some("daemon unavailable".into());
        page.sync(Vec::new());
        let rendered = render(&page, None);
        assert!(rendered.contains("Couldn't load who you owe a reply"));
        assert!(!rendered.contains("Nobody you write with"));
    }
}
