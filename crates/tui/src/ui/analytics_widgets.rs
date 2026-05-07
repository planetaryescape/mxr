//! Shared visual primitives for the analytics page. Each function
//! takes a `Frame`, a sub-area, the data it visualises, and the
//! current `Theme`. All colors come from the theme — no hardcoded
//! palette here.
//!
//! Charting policy: a chart on a metrics page must show something a
//! single number can't (a ratio, a distribution shape, or a position
//! on a scale). A bar chart that just illustrates "this number is
//! bigger than that number" duplicates the table — we omit it.

use crate::theme::Theme;
use ratatui::layout::{Alignment, Direction, Rect};
use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Wrap};
use tui_big_text::{BigText, PixelSize};

/// A bordered "stat card": small label up top, big value below it.
/// Used in 3-up summary strips above each analytics tab. The
/// `accent` flag picks between accent-coloured and primary-coloured
/// values so a strip can highlight the headline metric.
pub fn stat_card(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    theme: &Theme,
    accent: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.muted_style())
        .title(format!(" {label} "));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let value_style = if accent {
        theme.accent_style().add_modifier(Modifier::BOLD)
    } else {
        theme.primary_style().add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(value.to_string(), value_style)))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Compact title card with a `tui-big-text` label. Reserved for the
/// Wrapped header — celebratory, single-glance year-in-review banner.
/// Falls back to a styled paragraph when the inner height is too
/// small for big text. `pixel_size` controls how tall the glyphs
/// render: `Sextant` ≈ 3 rows, `HalfHeight` ≈ 4 rows, `Full` ≈ 8.
pub fn big_text_banner(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    pixel_size: PixelSize,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.accent_style())
        .title(format!(" {title} "));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let needed_rows: u16 = match pixel_size {
        PixelSize::Full => 8,
        PixelSize::HalfHeight | PixelSize::Quadrant => 4,
        PixelSize::ThirdHeight | PixelSize::Sextant => 3,
        _ => 4,
    };

    if inner.height < needed_rows {
        let style = theme.accent_style().add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(value.to_string(), style)))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let big = BigText::builder()
        .pixel_size(pixel_size)
        .style(theme.accent_style().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .lines(vec![Line::from(value.to_string())])
        .build();
    frame.render_widget(big, inner);
}

/// Vertical bar chart used for distribution histograms (response
/// time histogram, day-of-week, age bucket). `items` is
/// `(short_label, count)`. Bars get equal width; label widths come
/// from the inner area.
pub fn histogram_bar_chart(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[(String, u64)],
    theme: &Theme,
) {
    if items.is_empty() || items.iter().all(|(_, c)| *c == 0) {
        let block = Block::default()
            .title(format!(" {title} "))
            .borders(Borders::ALL)
            .border_style(theme.muted_style());
        frame.render_widget(
            Paragraph::new("(no data)")
                .style(theme.muted_style())
                .block(block),
            area,
        );
        return;
    }

    let bars: Vec<Bar> = items
        .iter()
        .map(|(label, count)| {
            Bar::default()
                .value(*count)
                .label(Line::from(label.clone()))
                .style(theme.accent_style())
                .value_style(theme.primary_style())
        })
        .collect();

    let n = items.len() as u16;
    let inner_w = area.width.saturating_sub(2);
    let bar_w = if n == 0 {
        1
    } else {
        ((inner_w / n).saturating_sub(1)).clamp(1, 5)
    };
    let chart = BarChart::default()
        .direction(Direction::Vertical)
        .bar_width(bar_w)
        .bar_gap(1)
        .data(BarGroup::default().bars(&bars))
        .block(
            Block::default()
                .title(format!(" {title} "))
                .borders(Borders::ALL)
                .border_style(theme.muted_style()),
        );
    frame.render_widget(chart, area);
}

/// Terse human count formatter: `123` → `123`, `1234` → `1.2k`,
/// `1_234_567` → `1.2M`. Lossy on purpose.
pub fn format_count(n: u64) -> String {
    if n < 1000 {
        format!("{n}")
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else if n < 1_000_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_formatter_breaks_at_thousand_million() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_000), "1.0k");
        assert_eq!(format_count(45_300), "45.3k");
        assert_eq!(format_count(1_100_000), "1.1M");
    }
}
