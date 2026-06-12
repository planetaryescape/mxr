use crate::app::{Toast, ToastSeverity};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

/// Widest a toast box may grow. Long messages are truncated rather than
/// wrapped — toasts are glanceable notifications, not reading material.
const TOAST_MAX_WIDTH: u16 = 48;
const TOAST_MIN_WIDTH: u16 = 20;
/// Each toast renders as a single content line inside a border.
const TOAST_HEIGHT: u16 = 3;

/// Draw stacked toast boxes anchored bottom-right, directly above the
/// status bar. `toasts` is expected newest-first (see
/// `ToastQueue::visible`); the newest renders closest to the status bar.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    toasts: &[&Toast],
    now: std::time::Instant,
    theme: &Theme,
) {
    if toasts.is_empty() {
        return;
    }

    // Reserve the bottom status-bar row; stack upward from just above it.
    let mut bottom = area.bottom().saturating_sub(1);
    for toast in toasts {
        if bottom < area.y + TOAST_HEIGHT {
            break;
        }
        let line = toast_line(toast, now, theme);
        let content_width = (line.width() as u16 + 2).clamp(TOAST_MIN_WIDTH, TOAST_MAX_WIDTH);
        let width = content_width.min(area.width);
        let rect = Rect {
            x: area.right().saturating_sub(width + 1),
            y: bottom - TOAST_HEIGHT,
            width,
            height: TOAST_HEIGHT,
        };

        let color = severity_color(toast.severity, theme);
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(line)
                .block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(color))
                        .style(Style::default().bg(theme.modal_bg)),
                )
                .style(Style::default().fg(theme.text_primary)),
            rect,
        );
        bottom = rect.y;
    }
}

fn severity_color(severity: ToastSeverity, theme: &Theme) -> ratatui::style::Color {
    match severity {
        ToastSeverity::Info => theme.accent,
        ToastSeverity::Success => theme.success,
        ToastSeverity::Warn => theme.warning,
        ToastSeverity::Error => theme.error,
    }
}

fn toast_line<'a>(toast: &'a Toast, now: std::time::Instant, theme: &Theme) -> Line<'a> {
    let mut spans = vec![Span::styled(
        toast.text.as_str(),
        Style::default().fg(theme.text_primary),
    )];
    if let Some(hint) = toast.action_hint.as_deref() {
        let remaining = toast.remaining(now).as_secs();
        spans.push(Span::styled(
            format!(" — {hint} ({remaining}s)"),
            Style::default().fg(theme.text_secondary),
        ));
    }
    Line::from(spans)
}
