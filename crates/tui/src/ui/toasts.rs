use super::mail_list::color_rgb;
use crate::app::{Toast, ToastSeverity};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

const TOAST_MAX_WIDTH: u16 = 48;
const TOAST_MIN_WIDTH: u16 = 20;
const TOAST_HEIGHT: u16 = 3;

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

    let mut bottom = area.bottom().saturating_sub(1);
    for toast in toasts {
        if bottom < area.y + TOAST_HEIGHT {
            break;
        }
        let line = toast_line(toast, now);
        let content_width = (line.width() as u16 + 2).clamp(TOAST_MIN_WIDTH, TOAST_MAX_WIDTH);
        let width = content_width.min(area.width);
        let rect = Rect {
            x: area.right().saturating_sub(width + 1),
            y: bottom - TOAST_HEIGHT,
            width,
            height: TOAST_HEIGHT,
        };

        let color = severity_color(toast.severity, theme);
        let text = readable_on(color, theme);
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(line)
                .block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(text).bg(color))
                        .style(Style::default().bg(color)),
                )
                .style(Style::default().fg(text).bg(color)),
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

/// The theme's dark or light text pole, whichever has the higher WCAG
/// contrast ratio on `bg`. A colour with no RGB value (the terminal's own
/// default) keeps the primary text colour.
fn readable_on(bg: Color, theme: &Theme) -> Color {
    let Some(bg_rgb) = color_rgb(bg) else {
        return theme.text_primary;
    };
    let ratio = |fg: Color| {
        color_rgb(fg).map_or(0.0, |fg_rgb| {
            contrast_ratio(relative_luminance(fg_rgb), relative_luminance(bg_rgb))
        })
    };
    if ratio(theme.contrast_dark) >= ratio(theme.contrast_light) {
        theme.contrast_dark
    } else {
        theme.contrast_light
    }
}

fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let channel = |value: u8| {
        let c = f64::from(value) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast_ratio(a: f64, b: f64) -> f64 {
    let (light, dark) = if a >= b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

fn toast_line(toast: &Toast, now: std::time::Instant) -> Line<'_> {
    let mut spans = vec![Span::raw(toast.text.as_str())];
    if let Some(hint) = toast.action_hint.as_deref() {
        let remaining = toast.remaining(now).as_secs();
        spans.push(Span::raw(format!(" — {hint} ({remaining}s)")));
    }
    Line::from(spans)
}
