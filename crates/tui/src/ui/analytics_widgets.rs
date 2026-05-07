//! Shared visual primitives for the analytics page. Each function
//! takes a `Frame`, a sub-area, the data it visualises, and the
//! current `Theme`. All colors come from the theme — no hardcoded
//! palette here.

use crate::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{
    Bar, BarChart, BarGroup, Block, Borders, Gauge, LineGauge, Paragraph, Wrap,
};
use tui_big_text::{BigText, PixelSize};

/// A bordered "stat card": small label on top, big value in the
/// middle. Used in 3-up summary strips above each analytics tab. The
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
        theme
            .accent_style()
            .add_modifier(Modifier::BOLD)
    } else {
        theme.primary_style().add_modifier(Modifier::BOLD)
    };
    let lines = vec![Line::from(""), Line::from(Span::styled(value.to_string(), value_style))];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Big-text headline card: small label up top, the value rendered as
/// large block glyphs (via `tui-big-text`) below. Used for the
/// Wrapped window label and the Response-Time p50 hero. Falls back
/// to a plain styled paragraph when the area is too short for big
/// text (under 3 rows of inner space).
pub fn big_number_card(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.accent_style())
        .title(format!(" {label} "));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 5 {
        // Not enough vertical room for `BigText` — render a styled
        // bold value instead so the card still reads.
        let style = theme.accent_style().add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(value.to_string(), style)))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let big = BigText::builder()
        .pixel_size(PixelSize::Sextant)
        .style(theme.accent_style().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .lines(vec![Line::from(value.to_string())])
        .build();
    frame.render_widget(big, inner);
}

/// Horizontal bar chart for top-N rankings (top contacts, top
/// senders, top-N storage). `items` is `(label, count)`. `max_label`
/// truncates labels so the bars line up; pick a width based on the
/// available area in the caller.
pub fn horizontal_bar_chart(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[(String, u64)],
    theme: &Theme,
    max_label: usize,
) {
    if items.is_empty() {
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
            let truncated: String = label.chars().take(max_label).collect();
            Bar::default()
                .value(*count)
                .label(Line::from(truncated))
                .style(theme.accent_style())
                .value_style(theme.primary_style().add_modifier(Modifier::BOLD))
        })
        .collect();

    let chart = BarChart::default()
        .direction(Direction::Horizontal)
        .bar_width(1)
        .bar_gap(0)
        .data(BarGroup::default().bars(&bars))
        .block(
            Block::default()
                .title(format!(" {title} "))
                .borders(Borders::ALL)
                .border_style(theme.muted_style()),
        );
    frame.render_widget(chart, area);
}

/// Vertical bar chart used for distribution histograms (response
/// time histogram, hour-of-day, day-of-week, age bucket). `items` is
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

    // Bar width adapts to inner area: keep at least 1, prefer 3 when
    // there's room. Inner width = area.width - 2 (borders); reserve a
    // gap of 1 between bars.
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

/// Stack of `LineGauge`s for percentile/latency rows. Each row gets
/// scaled against `max_seconds`, color-banded by absolute duration
/// (≤1h green, ≤1d yellow, >1d red). `rows` is
/// `(label, value_seconds_or_none)`; `None` rows render as muted "—".
pub fn percentile_bars(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    rows: &[(String, Option<u32>)],
    max_seconds: u32,
    theme: &Theme,
) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(theme.muted_style());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if rows.is_empty() || inner.height == 0 {
        return;
    }

    // Each gauge takes one row; remaining height becomes spacer.
    let constraints: Vec<Constraint> = rows
        .iter()
        .map(|_| Constraint::Length(1))
        .chain(std::iter::once(Constraint::Min(0)))
        .collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let max = max_seconds.max(1) as f64;
    for (i, (label, value)) in rows.iter().enumerate() {
        match value {
            Some(v) => {
                let ratio = ((*v as f64) / max).clamp(0.0, 1.0);
                let style = duration_band_style(*v, theme);
                let gauge = LineGauge::default()
                    .label(format!(
                        "{label:<8} {}",
                        super::analytics_page::format_duration_seconds(*v)
                    ))
                    .ratio(ratio)
                    .filled_style(style)
                    .unfilled_style(theme.muted_style());
                frame.render_widget(gauge, chunks[i]);
            }
            None => {
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!("{label:<8} "), theme.muted_style()),
                        Span::styled("—", theme.muted_style()),
                    ])),
                    chunks[i],
                );
            }
        }
    }
}

/// Single ratio gauge for share-of-total values (mimetype share,
/// list share %). Title is rendered in the block; the percentage
/// label is rendered inside the gauge.
pub fn ratio_gauge(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    label: &str,
    ratio: f64,
    theme: &Theme,
) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(theme.muted_style());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    let value_pct = (ratio * 100.0).round() as u16;
    let gauge = Gauge::default()
        .label(format!("{label} ({value_pct}%)"))
        .ratio(ratio.clamp(0.0, 1.0))
        .gauge_style(theme.accent_style().add_modifier(Modifier::BOLD));
    frame.render_widget(gauge, chunks[1]);
}

/// Color band by absolute duration. Used by `percentile_bars` and
/// any other latency-coded display.
fn duration_band_style(seconds: u32, theme: &Theme) -> Style {
    if seconds <= 3600 {
        theme.success_style().add_modifier(Modifier::BOLD)
    } else if seconds <= 86_400 {
        theme.warning_style().add_modifier(Modifier::BOLD)
    } else {
        theme.error_style().add_modifier(Modifier::BOLD)
    }
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
