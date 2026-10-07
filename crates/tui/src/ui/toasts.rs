use super::mail_list::color_rgb;
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
        let line = toast_line(toast, now);
        let content_width = (line.width() as u16 + 2).clamp(TOAST_MIN_WIDTH, TOAST_MAX_WIDTH);
        let width = content_width.min(area.width);
        let rect = Rect {
            x: area.right().saturating_sub(width + 1),
            y: bottom - TOAST_HEIGHT,
            width,
            height: TOAST_HEIGHT,
        };

        // A solid box in the type's colour, as the web app's toasts are,
        // with whichever text pole reads best on it.
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// The cell in the middle of the newest toast's text row.
    fn toast_cell(toast: &Toast, theme: &Theme) -> ratatui::buffer::Cell {
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).expect("terminal");
        let now = std::time::Instant::now();
        terminal
            .draw(|frame| draw(frame, frame.area(), &[toast], now, theme))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        // Bottom-right, above the status row: the text row is 3 rows up.
        let row = 12 - 1 - 2;
        let col = (0..80)
            .rev()
            .find(|x| buffer[(*x, row)].symbol() == "e")
            .expect("toast text on the row");
        buffer[(col, row)].clone()
    }

    #[test]
    fn each_type_is_a_solid_box_in_its_colour() {
        let theme = Theme::default();
        for (toast, colour) in [
            (Toast::success("Done here"), theme.success),
            (Toast::info("Archived one"), theme.accent),
            (Toast::warn("Archived some"), theme.warning),
            (Toast::error("Archive failed"), theme.error),
        ] {
            let cell = toast_cell(&toast, &theme);
            assert_eq!(cell.bg, colour, "{}", toast.text);
        }
    }

    #[test]
    fn text_takes_the_pole_that_reads_on_the_colour() {
        let theme = Theme::default();
        // Green and yellow are light: dark text. Red is darker: either pole
        // must clear the 4.5:1 that the other side would fail.
        assert_eq!(readable_on(theme.success, &theme), theme.contrast_dark);
        assert_eq!(readable_on(theme.warning, &theme), theme.contrast_dark);
        let ratio = |fg: Color, bg: Color| {
            contrast_ratio(
                relative_luminance(color_rgb(fg).unwrap()),
                relative_luminance(color_rgb(bg).unwrap()),
            )
        };
        for bg in [theme.success, theme.accent, theme.warning, theme.error] {
            let fg = readable_on(bg, &theme);
            assert!(ratio(fg, bg) >= 4.5, "{bg:?} on {fg:?}: {}", ratio(fg, bg));
        }
    }
}
