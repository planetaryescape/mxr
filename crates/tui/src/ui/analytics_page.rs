use crate::app::{AnalyticsState, AnalyticsView, ContactsMode, StorageMode, WrappedWindow};
use crate::ui::analytics_widgets::{
    big_number_card, format_count, histogram_bar_chart, horizontal_bar_chart, percentile_bars,
    ratio_gauge, stat_card,
};
use mxr_core::types::{ResponseTimeDirection, StaleBallInCourt, StorageGroupBy};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], state, theme);
    draw_table(frame, chunks[1], state, theme);
    draw_footer(frame, chunks[2], state, theme);
}

fn draw_header(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    let mut tabs = Vec::new();
    for view in [
        AnalyticsView::Storage,
        AnalyticsView::StaleThreads,
        AnalyticsView::Contacts,
        AnalyticsView::ResponseTime,
        AnalyticsView::Subscriptions,
        AnalyticsView::Wrapped,
    ] {
        let label = view.label();
        let style = if state.view == view {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_muted)
        };
        if !tabs.is_empty() {
            tabs.push(Span::styled(" | ", Style::default().fg(theme.text_muted)));
        }
        tabs.push(Span::styled(label, style));
    }
    let title = match state.view {
        AnalyticsView::Storage => match state.storage_mode {
            StorageMode::Breakdown => format!(
                "Storage  [mode=breakdown  group_by={}]",
                group_by_label(state.storage_group_by)
            ),
            StorageMode::LargestMessages => format!(
                "Storage  [mode=largest  limit={}{}]",
                state.largest_limit,
                state
                    .largest_since_days
                    .map(|d| format!("  since={d}d"))
                    .unwrap_or_default(),
            ),
        },
        AnalyticsView::StaleThreads => format!(
            "Stale Threads  [perspective={}  older_than={}d  within={}d]",
            stale_perspective_label(state.stale_perspective),
            state.stale_older_than_days,
            state.stale_within_days,
        ),
        AnalyticsView::Contacts => match state.contacts_mode {
            ContactsMode::Asymmetry => format!(
                "Contacts  [mode=asymmetry  min_inbound={}]",
                state.asymmetry_min_inbound
            ),
            ContactsMode::Decay => format!(
                "Contacts  [mode=decay  threshold={}d  lookback={}d]",
                state.decay_threshold_days, state.decay_max_lookback_days
            ),
        },
        AnalyticsView::ResponseTime => format!(
            "Response Time  [direction={}]",
            response_direction_label(state.response_time_direction)
        ),
        AnalyticsView::Subscriptions => format!(
            "Subscriptions  [limit={}{}]",
            state.subscriptions_limit,
            if state.subscriptions_rank {
                "  sort=open-rate"
            } else {
                ""
            }
        ),
        AnalyticsView::Wrapped => format!("Wrapped  [{}]", wrapped_window_label(state.wrapped_window)),
    };
    let mut full_title = format!(" {title} ");
    if state.is_refreshing_with_data() {
        full_title.push_str("↻ refreshing ");
    }
    let block = Block::default()
        .title(full_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(Line::from(tabs)), inner);
}

fn draw_table(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    // Cold load: no cached data and a request is in flight. Only here
    // do we replace the pane with a "Computing analytics..." block.
    // When stale data exists for the active view we fall through and
    // keep rendering it; the refreshing indicator in the header tells
    // the user a background refresh is running.
    if state.should_show_cold_load() {
        let block = Block::default()
            .title(" Loading ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.text_muted));
        frame.render_widget(
            Paragraph::new("Computing analytics...")
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    if let Some(error) = state.error.as_deref() {
        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.error));
        frame.render_widget(
            Paragraph::new(error.to_string())
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    match state.view {
        AnalyticsView::Storage => match state.storage_mode {
            StorageMode::Breakdown => draw_storage(frame, area, state, theme),
            StorageMode::LargestMessages => draw_largest_messages(frame, area, state, theme),
        },
        AnalyticsView::StaleThreads => draw_stale(frame, area, state, theme),
        AnalyticsView::Contacts => match state.contacts_mode {
            ContactsMode::Asymmetry => draw_asymmetry(frame, area, state, theme),
            ContactsMode::Decay => draw_decay(frame, area, state, theme),
        },
        AnalyticsView::ResponseTime => draw_response_time(frame, area, state, theme),
        AnalyticsView::Subscriptions => draw_subscriptions(frame, area, state, theme),
        AnalyticsView::Wrapped => draw_wrapped(frame, area, state, theme),
    }
}

/// Standard analytics-tab layout: a 3-up "stat strip" of cards on top,
/// then a chart pane, then (optionally) a detail table. Returns the
/// three sub-rectangles. `chart_height` controls how many rows the
/// chart gets; pass 0 to skip the chart and let the table take the
/// remaining space.
fn analytics_layout(area: Rect, chart_height: u16, with_table: bool) -> (Rect, Rect, Rect) {
    let strip_h = 5u16;
    let chunks = if with_table {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(strip_h),
                Constraint::Length(chart_height),
                Constraint::Min(0),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(strip_h),
                Constraint::Min(0),
                Constraint::Length(0),
            ])
            .split(area)
    };
    (chunks[0], chunks[1], chunks[2])
}

fn three_up(area: Rect) -> [Rect; 3] {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);
    [cols[0], cols[1], cols[2]]
}

fn draw_storage(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    if state.storage_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No storage data yet. Sync first, then come back.",
            theme,
        );
        return;
    }

    let total_bytes: u64 = state.storage_rows.iter().map(|r| r.bytes).sum();
    let total_count: u64 = state.storage_rows.iter().map(|r| r.count as u64).sum();
    let top_share = state
        .storage_rows
        .first()
        .map(|r| {
            if total_bytes == 0 {
                0.0
            } else {
                (r.bytes as f64) / (total_bytes as f64) * 100.0
            }
        })
        .unwrap_or(0.0);

    let (strip, chart, table) = analytics_layout(area, 12, true);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Total", &format_bytes(total_bytes), theme, true);
    stat_card(
        frame,
        cards[1],
        "Items",
        &format_count(total_count),
        theme,
        false,
    );
    stat_card(
        frame,
        cards[2],
        "Top share",
        &format!("{top_share:.1}%"),
        theme,
        false,
    );

    let bars: Vec<(String, u64)> = state
        .storage_rows
        .iter()
        .take(10)
        .map(|r| (format!("{} {}", r.key, format_bytes(r.bytes)), r.bytes))
        .collect();
    horizontal_bar_chart(frame, chart, "Top by size", &bars, theme, 28);

    let header =
        Row::new(vec!["Key", "Bytes", "Count"]).style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .storage_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.key.clone(),
                format_bytes(row.bytes),
                row.count.to_string(),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(60),
        Constraint::Percentage(20),
        Constraint::Percentage(20),
    ];
    render_table(
        frame,
        table,
        " Storage ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn draw_stale(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    if state.stale_rows.is_empty() {
        empty_state(frame, area, "No stale threads in this window.", theme);
        return;
    }

    let count = state.stale_rows.len() as u64;
    let oldest = state
        .stale_rows
        .iter()
        .map(|r| r.days_stale)
        .max()
        .unwrap_or(0);
    let median = {
        let mut ages: Vec<u32> = state.stale_rows.iter().map(|r| r.days_stale).collect();
        ages.sort_unstable();
        ages.get(ages.len() / 2).copied().unwrap_or(0)
    };

    let (strip, chart, table) = analytics_layout(area, 10, true);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Stale", &format_count(count), theme, true);
    stat_card(frame, cards[1], "Oldest", &format!("{oldest}d"), theme, false);
    stat_card(frame, cards[2], "Median", &format!("{median}d"), theme, false);

    let mut buckets = [0u64; 4]; // 7-14, 14-30, 30-90, 90+
    for r in &state.stale_rows {
        let d = r.days_stale;
        if d < 14 {
            buckets[0] += 1;
        } else if d < 30 {
            buckets[1] += 1;
        } else if d < 90 {
            buckets[2] += 1;
        } else {
            buckets[3] += 1;
        }
    }
    let hist = vec![
        ("7-14d".to_string(), buckets[0]),
        ("14-30d".to_string(), buckets[1]),
        ("30-90d".to_string(), buckets[2]),
        ("90d+".to_string(), buckets[3]),
    ];
    histogram_bar_chart(frame, chart, "Age distribution", &hist, theme);

    let header = Row::new(vec!["Subject", "Counterparty", "Days Stale", "Latest"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .stale_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.latest_subject.clone(),
                row.counterparty_email.clone(),
                row.days_stale.to_string(),
                row.latest_date.format("%Y-%m-%d").to_string(),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(45),
        Constraint::Percentage(30),
        Constraint::Percentage(10),
        Constraint::Percentage(15),
    ];
    render_table(
        frame,
        table,
        " Stale Threads ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn draw_asymmetry(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    if state.asymmetry_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No contacts crossed the inbound threshold yet.",
            theme,
        );
        return;
    }

    let count = state.asymmetry_rows.len() as u64;
    let max_asym = state
        .asymmetry_rows
        .iter()
        .map(|r| r.asymmetry)
        .fold(0.0_f64, f64::max);
    let avg_asym = state
        .asymmetry_rows
        .iter()
        .map(|r| r.asymmetry)
        .sum::<f64>()
        / (count as f64).max(1.0);

    let (strip, chart, table) = analytics_layout(area, 12, true);
    let cards = three_up(strip);
    stat_card(
        frame,
        cards[0],
        "Contacts",
        &format_count(count),
        theme,
        true,
    );
    stat_card(
        frame,
        cards[1],
        "Max",
        &format!("{max_asym:.2}"),
        theme,
        false,
    );
    stat_card(
        frame,
        cards[2],
        "Avg",
        &format!("{avg_asym:.2}"),
        theme,
        false,
    );

    let bars: Vec<(String, u64)> = state
        .asymmetry_rows
        .iter()
        .take(10)
        .map(|r| {
            let label: String = r.email.chars().take(28).collect();
            (
                format!("{label} {}/{}", r.total_inbound, r.total_outbound),
                r.total_inbound as u64,
            )
        })
        .collect();
    horizontal_bar_chart(frame, chart, "Top by inbound", &bars, theme, 36);

    let header = Row::new(vec!["Email", "Inbound", "Outbound", "Asymmetry"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .asymmetry_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.email.clone(),
                row.total_inbound.to_string(),
                row.total_outbound.to_string(),
                format!("{:.2}", row.asymmetry),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(50),
        Constraint::Percentage(15),
        Constraint::Percentage(15),
        Constraint::Percentage(20),
    ];
    render_table(
        frame,
        table,
        " Contact Asymmetry ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn draw_response_time(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    let Some(summary) = state.response_time.as_ref() else {
        empty_state(
            frame,
            area,
            "No response-time data yet. Sync first, then refresh.",
            theme,
        );
        return;
    };

    if summary.sample_count == 0 {
        empty_state(
            frame,
            area,
            "No reply pairs in scope. Try widening the filter.",
            theme,
        );
        return;
    }

    // Vertical split: hero card | percentile bars | histogram.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .split(area);

    let direction_label = match summary.direction {
        ResponseTimeDirection::IReplied => "→ I replied",
        ResponseTimeDirection::TheyReplied => "← they replied",
    };
    let hero_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[0]);
    big_number_card(
        frame,
        hero_split[0],
        "Clock p50",
        &format_duration_seconds(summary.clock_p50_seconds),
        theme,
    );
    let mut meta_lines = vec![
        Line::from(Span::styled(
            direction_label,
            theme.accent_style().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("samples: {}", format_count(summary.sample_count as u64))),
    ];
    if let Some(cp) = state.response_time_counterparty.as_deref() {
        meta_lines.push(Line::from(format!("counterparty: {cp}")));
    }
    if let Some(d) = state.response_time_since_days {
        meta_lines.push(Line::from(format!("since: {d}d")));
    }
    let meta_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.muted_style())
        .title(" Direction ");
    frame.render_widget(
        Paragraph::new(meta_lines)
            .block(meta_block)
            .wrap(Wrap { trim: false }),
        hero_split[1],
    );

    let max_p90 = summary
        .clock_p90_seconds
        .max(summary.business_hours_p90_seconds.unwrap_or(0));
    let rows = vec![
        ("clock p50".to_string(), Some(summary.clock_p50_seconds)),
        ("clock p90".to_string(), Some(summary.clock_p90_seconds)),
        ("biz p50".to_string(), summary.business_hours_p50_seconds),
        ("biz p90".to_string(), summary.business_hours_p90_seconds),
    ];
    percentile_bars(frame, chunks[1], "Percentiles", &rows, max_p90, theme);

    let buckets: Vec<(String, u64)> = summary
        .histogram
        .iter()
        .map(|b| {
            let label = histogram_bucket_label(b.upper_bound_seconds);
            (label, b.count as u64)
        })
        .collect();
    histogram_bar_chart(frame, chunks[2], "Distribution", &buckets, theme);
}

fn histogram_bucket_label(upper_bound_seconds: u32) -> String {
    if upper_bound_seconds == u32::MAX {
        return "3d+".into();
    }
    if upper_bound_seconds < 60 {
        format!("<{upper_bound_seconds}s")
    } else if upper_bound_seconds < 3600 {
        format!("<{}m", upper_bound_seconds / 60)
    } else if upper_bound_seconds < 86_400 {
        format!("<{}h", upper_bound_seconds / 3600)
    } else {
        format!("<{}d", upper_bound_seconds / 86_400)
    }
}

fn render_table<'a>(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    header: Row<'a>,
    rows: Vec<Row<'a>>,
    widths: &[Constraint],
    selected_index: usize,
    theme: &crate::theme::Theme,
) {
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title.to_string())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .row_highlight_style(theme.highlight_style())
        .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(Some(selected_index));
    frame.render_stateful_widget(table, area, &mut state);
}

/// Slice 3: Storage in `LargestMessages` mode renders a table of
/// individual messages by size, mirroring `mxr storage --by message`.
fn draw_largest_messages(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    if state.largest_message_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No large messages yet. Sync first, then come back.",
            theme,
        );
        return;
    }

    let total_bytes: u64 = state
        .largest_message_rows
        .iter()
        .map(|r| r.size_bytes)
        .sum();
    let count = state.largest_message_rows.len() as u64;
    let biggest = state
        .largest_message_rows
        .first()
        .map(|r| r.size_bytes)
        .unwrap_or(0);

    let (strip, chart, table) = analytics_layout(area, 12, true);
    let cards = three_up(strip);
    stat_card(
        frame,
        cards[0],
        "Sum",
        &format_bytes(total_bytes),
        theme,
        true,
    );
    stat_card(frame, cards[1], "Messages", &format_count(count), theme, false);
    stat_card(
        frame,
        cards[2],
        "Biggest",
        &format_bytes(biggest),
        theme,
        false,
    );

    let bars: Vec<(String, u64)> = state
        .largest_message_rows
        .iter()
        .take(10)
        .map(|r| {
            let subject: String = r.subject.chars().take(28).collect();
            (
                format!("{subject} {}", format_bytes(r.size_bytes)),
                r.size_bytes,
            )
        })
        .collect();
    horizontal_bar_chart(frame, chart, "Top by size", &bars, theme, 36);

    let header = Row::new(vec!["Subject", "From", "Size", "Date"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .largest_message_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.subject.clone(),
                row.from_email.clone(),
                format_bytes(row.size_bytes),
                row.date.format("%Y-%m-%d").to_string(),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(50),
        Constraint::Percentage(25),
        Constraint::Percentage(12),
        Constraint::Percentage(13),
    ];
    render_table(
        frame,
        table,
        " Largest Messages ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

/// Slice 4: Contacts in `Decay` mode lists going-cold relationships
/// (inbound newer than outbound by a threshold). `last_outbound_at`
/// is `Option<DateTime<Utc>>` so the column renders `-` when the
/// counterparty has never been written back to (guards against a
/// silent `unwrap_or(0)` rendering "0 days" for never-replied).
fn draw_decay(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    if state.decay_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No decaying contacts in this window.",
            theme,
        );
        return;
    }

    let count = state.decay_rows.len() as u64;
    let longest = state
        .decay_rows
        .iter()
        .map(|r| r.days_since_inbound)
        .max()
        .unwrap_or(0);
    let median = {
        let mut ds: Vec<u32> = state.decay_rows.iter().map(|r| r.days_since_inbound).collect();
        ds.sort_unstable();
        ds.get(ds.len() / 2).copied().unwrap_or(0)
    };

    let (strip, chart, table) = analytics_layout(area, 10, true);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Cold contacts", &format_count(count), theme, true);
    stat_card(frame, cards[1], "Longest gap", &format!("{longest}d"), theme, false);
    stat_card(frame, cards[2], "Median gap", &format!("{median}d"), theme, false);

    let mut buckets = [0u64; 4]; // <60, 60-90, 90-180, 180+
    for r in &state.decay_rows {
        let d = r.days_since_inbound;
        if d < 60 {
            buckets[0] += 1;
        } else if d < 90 {
            buckets[1] += 1;
        } else if d < 180 {
            buckets[2] += 1;
        } else {
            buckets[3] += 1;
        }
    }
    let hist = vec![
        ("30-60d".to_string(), buckets[0]),
        ("60-90d".to_string(), buckets[1]),
        ("90-180d".to_string(), buckets[2]),
        ("180d+".to_string(), buckets[3]),
    ];
    histogram_bar_chart(frame, chart, "Days since inbound", &hist, theme);

    let header = Row::new(vec!["Email", "Days since inbound", "Days since outbound"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .decay_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.email.clone(),
                row.days_since_inbound.to_string(),
                row.days_since_outbound
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "-".into()),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(55),
        Constraint::Percentage(22),
        Constraint::Percentage(23),
    ];
    render_table(
        frame,
        table,
        " Contact Decay ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

/// Slice 6: Subscriptions table. Default sort matches the daemon's
/// `latest date DESC`. The `subscriptions_rank` toggle re-sorts
/// locally by open-rate ASC (ties broken by archived_unread DESC) and
/// swaps the rightmost columns to surface the ranking columns the
/// CLI's `--rank` mode shows.
fn draw_subscriptions(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    if state.subscriptions.is_empty() {
        empty_state(
            frame,
            area,
            "No mailing-list senders detected. Sync first, then come back.",
            theme,
        );
        return;
    }

    let total_msgs: u64 = state
        .subscriptions
        .iter()
        .map(|s| s.message_count as u64)
        .sum();
    let avg_open = {
        let (sum, n) = state.subscriptions.iter().fold((0.0_f64, 0u32), |(s, n), r| {
            if r.message_count == 0 {
                (s, n)
            } else {
                (s + (r.opened_count as f64) / (r.message_count as f64), n + 1)
            }
        });
        if n == 0 {
            0.0
        } else {
            sum / (n as f64) * 100.0
        }
    };

    let (strip, chart, table) = analytics_layout(area, 12, true);
    let cards = three_up(strip);
    stat_card(
        frame,
        cards[0],
        "Senders",
        &format_count(state.subscriptions.len() as u64),
        theme,
        true,
    );
    stat_card(
        frame,
        cards[1],
        "Messages",
        &format_count(total_msgs),
        theme,
        false,
    );
    stat_card(
        frame,
        cards[2],
        "Avg open",
        &format!("{avg_open:.1}%"),
        theme,
        false,
    );

    if state.subscriptions_rank {
        // Bottom-10 by open rate.
        let mut ranked: Vec<&_> = state.subscriptions.iter().collect();
        ranked.sort_by(|a, b| {
            let ra = open_rate(a.message_count, a.opened_count);
            let rb = open_rate(b.message_count, b.opened_count);
            ra.partial_cmp(&rb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.archived_unread_count.cmp(&a.archived_unread_count))
        });
        let bars: Vec<(String, u64)> = ranked
            .iter()
            .take(10)
            .map(|s| {
                let label: String = s
                    .sender_name
                    .clone()
                    .unwrap_or_else(|| s.sender_email.clone())
                    .chars()
                    .take(28)
                    .collect();
                let rate = open_rate(s.message_count, s.opened_count) * 100.0;
                (
                    format!("{label} {rate:.0}%"),
                    s.archived_unread_count as u64,
                )
            })
            .collect();
        horizontal_bar_chart(frame, chart, "Bottom by open rate", &bars, theme, 36);
    } else {
        // Top-10 by message count.
        let mut top: Vec<&_> = state.subscriptions.iter().collect();
        top.sort_by(|a, b| b.message_count.cmp(&a.message_count));
        let bars: Vec<(String, u64)> = top
            .iter()
            .take(10)
            .map(|s| {
                let label: String = s
                    .sender_name
                    .clone()
                    .unwrap_or_else(|| s.sender_email.clone())
                    .chars()
                    .take(28)
                    .collect();
                (
                    format!("{label} ({})", s.message_count),
                    s.message_count as u64,
                )
            })
            .collect();
        horizontal_bar_chart(frame, chart, "Top by volume", &bars, theme, 36);
    }

    let mut indexed: Vec<usize> = (0..state.subscriptions.len()).collect();
    if state.subscriptions_rank {
        indexed.sort_by(|&a, &b| {
            let ra = &state.subscriptions[a];
            let rb = &state.subscriptions[b];
            let rate_a = if ra.message_count == 0 {
                f64::INFINITY
            } else {
                ra.opened_count as f64 / ra.message_count as f64
            };
            let rate_b = if rb.message_count == 0 {
                f64::INFINITY
            } else {
                rb.opened_count as f64 / rb.message_count as f64
            };
            rate_a
                .partial_cmp(&rate_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| rb.archived_unread_count.cmp(&ra.archived_unread_count))
        });
    }
    let (header, widths): (Row<'_>, [Constraint; 5]) = if state.subscriptions_rank {
        (
            Row::new(vec!["Sender", "Email", "Count", "Opened", "Arch/Unrd"])
                .style(Style::default().fg(theme.text_muted).bold()),
            [
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(10),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
            ],
        )
    } else {
        (
            Row::new(vec!["Sender", "Email", "Count", "Method", "Latest Subject"])
                .style(Style::default().fg(theme.text_muted).bold()),
            [
                Constraint::Percentage(20),
                Constraint::Percentage(25),
                Constraint::Percentage(8),
                Constraint::Percentage(12),
                Constraint::Percentage(35),
            ],
        )
    };
    let rows: Vec<Row> = indexed
        .into_iter()
        .map(|i| {
            let s = &state.subscriptions[i];
            if state.subscriptions_rank {
                Row::new(vec![
                    s.sender_name.clone().unwrap_or_default(),
                    s.sender_email.clone(),
                    s.message_count.to_string(),
                    s.opened_count.to_string(),
                    s.archived_unread_count.to_string(),
                ])
            } else {
                Row::new(vec![
                    s.sender_name.clone().unwrap_or_default(),
                    s.sender_email.clone(),
                    s.message_count.to_string(),
                    unsubscribe_method_label(&s.unsubscribe).to_string(),
                    s.latest_subject.clone(),
                ])
            }
        })
        .collect();
    render_table(
        frame,
        table,
        " Subscriptions ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn open_rate(message_count: u32, opened: u32) -> f64 {
    if message_count == 0 {
        0.0
    } else {
        (opened as f64) / (message_count as f64)
    }
}

fn unsubscribe_method_label(method: &mxr_core::types::UnsubscribeMethod) -> &'static str {
    use mxr_core::types::UnsubscribeMethod;
    match method {
        UnsubscribeMethod::OneClick { .. } => "one-click",
        UnsubscribeMethod::HttpLink { .. } => "link",
        UnsubscribeMethod::Mailto { .. } => "mailto",
        UnsubscribeMethod::BodyLink { .. } => "body-link",
        UnsubscribeMethod::None => "-",
    }
}

/// Wrapped year-in-review dashboard. A header strip (BigText window
/// label + window range + totals) sits above two rows of three tiles
/// — Volume, When, Contacts, Reply discipline, Storage, Newsletters
/// — and a full-width Superlatives strip. Each tile picks the
/// widget that fits its data shape (BarChart for Volume + When,
/// horizontal BarChart for top contacts, percentile bars for reply
/// discipline, ratio gauges for storage + newsletters share).
/// Tile selection (`wrapped_selected_tile`, 0..=6) draws the focused
/// border around the selected tile.
fn draw_wrapped(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    let Some(summary) = state.wrapped.as_ref() else {
        empty_state(
            frame,
            area,
            "No wrapped summary yet. Press 'r' to compute, or wait for sync to populate the underlying data.",
            theme,
        );
        return;
    };

    // Outer split: header (8 rows) | body (rest).
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(area);
    draw_wrapped_header(frame, outer_chunks[0], summary, theme);

    // Body split: row1 | row2 | superlatives.
    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Percentage(45),
            Constraint::Min(4),
        ])
        .split(outer_chunks[1]);

    let row1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(body_chunks[0]);
    let row2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(body_chunks[1]);

    let selected = state.wrapped_selected_tile;
    draw_wrapped_volume(frame, row1[0], summary, theme, selected == 0);
    draw_wrapped_when(frame, row1[1], summary, theme, selected == 1);
    draw_wrapped_contacts(frame, row1[2], summary, theme, selected == 2);
    draw_wrapped_reply(frame, row2[0], summary, theme, selected == 3);
    draw_wrapped_storage(frame, row2[1], summary, theme, selected == 4);
    draw_wrapped_newsletters(frame, row2[2], summary, theme, selected == 5);
    draw_wrapped_superlatives(frame, body_chunks[2], summary, theme, selected == 6);
}

fn draw_wrapped_header(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    let label = summary.label.to_uppercase();
    big_number_card(frame, cols[0], "Wrapped", &label, theme);

    let total_msgs =
        summary.volume.inbound_count as u64 + summary.volume.outbound_count as u64;
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "{} → {}",
                summary.window_start.format("%Y-%m-%d"),
                summary.window_end.format("%Y-%m-%d"),
            ),
            theme.accent_style().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("messages: {}", format_count(total_msgs))),
        Line::from(format!(
            "threads:  {}",
            format_count(summary.volume.thread_count as u64)
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.muted_style())
        .title(" Window ");
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        cols[1],
    );
}

fn wrapped_tile_block<'a>(
    title: &'a str,
    theme: &crate::theme::Theme,
    focused: bool,
) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(format!(" {title} "))
}

fn draw_wrapped_volume(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let block = wrapped_tile_block("Volume", theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let bars = vec![
        ("in".to_string(), summary.volume.inbound_count as u64),
        ("out".to_string(), summary.volume.outbound_count as u64),
        ("threads".to_string(), summary.volume.thread_count as u64),
    ];
    histogram_bar_chart(frame, inner, "", &bars, theme);
}

fn draw_wrapped_when(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let title = format!(
        "When · busiest {}",
        summary
            .time_patterns
            .busiest_hour_utc
            .map(|h| format!("{h:02}:00 UTC"))
            .unwrap_or_else(|| "—".into())
    );
    let block = wrapped_tile_block(&title, theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let bars: Vec<(String, u64)> = summary
        .time_patterns
        .hour_distribution
        .iter()
        .enumerate()
        .map(|(h, c)| (format!("{h:02}"), *c as u64))
        .collect();
    histogram_bar_chart(frame, inner, "", &bars, theme);
}

fn draw_wrapped_contacts(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let block = wrapped_tile_block("Top inbound", theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let bars: Vec<(String, u64)> = summary
        .top_contacts
        .most_emailed_to_me
        .iter()
        .take(5)
        .map(|c| {
            let label: String = c.email.chars().take(22).collect();
            (format!("{label} ({})", c.count), c.count as u64)
        })
        .collect();
    horizontal_bar_chart(frame, inner, "", &bars, theme, 28);
}

fn draw_wrapped_reply(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    if let Some(reply) = summary.reply_discipline.as_ref() {
        let max_p90 = reply
            .clock_p90_seconds
            .max(reply.business_hours_p90_seconds.unwrap_or(0));
        let title = format!("Reply discipline · samples {}", reply.sample_count);
        let block = wrapped_tile_block(&title, theme, focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let rows = vec![
            ("clock p50".to_string(), Some(reply.clock_p50_seconds)),
            ("clock p90".to_string(), Some(reply.clock_p90_seconds)),
            ("biz p50".to_string(), reply.business_hours_p50_seconds),
            ("biz p90".to_string(), reply.business_hours_p90_seconds),
        ];
        percentile_bars(frame, inner, "", &rows, max_p90, theme);
    } else {
        let block = wrapped_tile_block("Reply discipline", theme, focused);
        frame.render_widget(
            Paragraph::new("(no reply pairs yet)")
                .style(theme.muted_style())
                .block(block),
            area,
        );
    }
}

fn draw_wrapped_storage(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let storage = &summary.storage;
    let title = format!("Storage · {}", format_bytes(storage.total_bytes));
    let block = wrapped_tile_block(&title, theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);
    let (label, ratio) = match storage.top_mimetype.as_ref() {
        Some(top) => {
            let r = if storage.total_bytes == 0 {
                0.0
            } else {
                (top.bytes as f64) / (storage.total_bytes as f64)
            };
            (format!("{} ({})", top.key, format_bytes(top.bytes)), r)
        }
        None => ("(no attachments)".to_string(), 0.0),
    };
    ratio_gauge(frame, chunks[0], "Top mime share", &label, ratio, theme);
    let mut detail = Vec::new();
    if let Some(heaviest) = storage.heaviest_message.as_ref() {
        let subject: String = heaviest.subject.chars().take(40).collect();
        detail.push(Line::from(format!(
            "heaviest: {subject} ({})",
            format_bytes(heaviest.size_bytes)
        )));
    }
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_wrapped_newsletters(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let news = &summary.newsletters;
    let title = format!("Newsletters · {} lists", news.unique_lists);
    let block = wrapped_tile_block(&title, theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);
    let share_ratio = (news.list_share_of_inbound_pct / 100.0).clamp(0.0, 1.0);
    ratio_gauge(
        frame,
        chunks[0],
        "Share of inbound",
        &format!("{:.1}%", news.list_share_of_inbound_pct),
        share_ratio,
        theme,
    );
    let mut detail = Vec::new();
    if let Some(top) = news.top_list.as_ref() {
        let opened_pct = if top.message_count == 0 {
            0.0
        } else {
            (top.opened_count as f64) / (top.message_count as f64) * 100.0
        };
        let id: String = top.list_id.chars().take(40).collect();
        detail.push(Line::from(format!(
            "top: {id} ({} msgs, {opened_pct:.0}% opened)",
            top.message_count
        )));
    }
    frame.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_wrapped_superlatives(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let sup = &summary.superlatives;
    let block = wrapped_tile_block("Superlatives", theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let accent = theme.accent_style().add_modifier(Modifier::BOLD);
    let muted = theme.muted_style();

    let longest_lines = match sup.longest_thread.as_ref() {
        Some(t) => {
            let subject: String = t.subject.chars().take(60).collect();
            vec![
                Line::from(Span::styled("longest thread", muted)),
                Line::from(Span::styled(subject, theme.primary_style())),
                Line::from(Span::styled(format!("{} messages", t.message_count), accent)),
            ]
        }
        None => vec![Line::from(Span::styled("(no longest thread)", muted))],
    };
    frame.render_widget(
        Paragraph::new(longest_lines).wrap(Wrap { trim: false }),
        cols[0],
    );

    let ghosted_lines = match sup.most_ghosted.as_ref() {
        Some(g) => vec![
            Line::from(Span::styled("most ghosted", muted)),
            Line::from(Span::styled(g.email.clone(), theme.primary_style())),
            Line::from(Span::styled(
                format!("{} inbound, 0 replied", g.inbound_count),
                accent,
            )),
        ],
        None => vec![Line::from(Span::styled("(no most-ghosted)", muted))],
    };
    frame.render_widget(
        Paragraph::new(ghosted_lines).wrap(Wrap { trim: false }),
        cols[1],
    );
}

fn empty_state(frame: &mut Frame, area: Rect, message: &str, theme: &crate::theme::Theme) {
    let block = Block::default()
        .title(" No Data ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_muted));
    frame.render_widget(
        Paragraph::new(message)
            .style(Style::default().fg(theme.text_muted))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    let _ = state;
    let hint = "Tab/Shift-Tab:switch view  j/k:select  r:refresh  Esc:mailbox";
    frame.render_widget(
        Paragraph::new(hint).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn group_by_label(group_by: StorageGroupBy) -> &'static str {
    match group_by {
        StorageGroupBy::Mimetype => "mimetype",
        StorageGroupBy::Sender => "sender",
        StorageGroupBy::Label => "label",
    }
}

fn stale_perspective_label(perspective: StaleBallInCourt) -> &'static str {
    match perspective {
        StaleBallInCourt::Mine => "mine",
        StaleBallInCourt::Theirs => "theirs",
    }
}

fn response_direction_label(direction: ResponseTimeDirection) -> &'static str {
    match direction {
        ResponseTimeDirection::IReplied => "i_replied",
        ResponseTimeDirection::TheyReplied => "they_replied",
    }
}

fn wrapped_window_label(window: WrappedWindow) -> String {
    match window {
        WrappedWindow::Ytd => "year-to-date".into(),
        WrappedWindow::Year(y) => format!("year={y}"),
        WrappedWindow::SinceDays(d) => format!("last {d} days"),
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub(super) fn format_duration_seconds(seconds: u32) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else if seconds < 86_400 {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}d{}h", seconds / 86_400, (seconds % 86_400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AnalyticsState;
    use mxr_core::types::{
        ContactAsymmetryRow, ResponseTimeDirection, ResponseTimeSummary, StorageBucket,
    };
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn theme() -> crate::theme::Theme {
        crate::theme::Theme::default()
    }

    /// Phase 2.5 / Behavior 1: a populated storage view renders the
    /// table rows, the sums, and the title. Catches "renderer drops
    /// rows from the daemon response" regressions.
    #[test]
    fn storage_view_renders_rows_from_state() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.storage_rows = vec![StorageBucket {
            key: "hello@example.com".into(),
            bytes: 1024 * 1024 * 7,
            count: 42,
        }];
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("Storage"), "header missing: {rendered}");
        assert!(
            rendered.contains("hello@example.com"),
            "row key missing: {rendered}"
        );
        assert!(rendered.contains("MiB"), "byte format missing: {rendered}");
        assert!(rendered.contains("42"), "row count missing: {rendered}");
    }

    /// Phase 2.5 / Behavior 3: an empty result set renders an
    /// empty-state message instead of a blank pane or a panic.
    #[test]
    fn storage_view_renders_empty_state_when_no_rows() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(
            rendered.contains("No storage data"),
            "empty-state message missing: {rendered}"
        );
    }

    /// Response Time view now renders a hero card (clock p50), a
    /// percentile-bar stack with `clock p50 / p90 / biz p50 / biz p90`
    /// labels and a `—` sentinel when business-hours percentiles are
    /// unset, plus a histogram pane labeled "Distribution".
    #[test]
    fn response_time_view_renders_hero_percentiles_and_histogram() {
        use mxr_core::types::ResponseTimeBucket;
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::ResponseTime;
        state.response_time = Some(ResponseTimeSummary {
            direction: ResponseTimeDirection::IReplied,
            sample_count: 17,
            clock_p50_seconds: 90,
            clock_p90_seconds: 3600,
            business_hours_p50_seconds: None,
            business_hours_p90_seconds: None,
            histogram: vec![
                ResponseTimeBucket {
                    upper_bound_seconds: 60,
                    count: 5,
                },
                ResponseTimeBucket {
                    upper_bound_seconds: 300,
                    count: 8,
                },
                ResponseTimeBucket {
                    upper_bound_seconds: u32::MAX,
                    count: 4,
                },
            ],
        });
        let rendered = render_to_string(120, 30, |frame| {
            draw(frame, Rect::new(0, 0, 120, 30), &state, &theme());
        });
        // Direction badge.
        assert!(
            rendered.contains("I replied"),
            "direction badge missing: {rendered}"
        );
        // Sample count somewhere in the meta block.
        assert!(rendered.contains("17"), "sample count missing: {rendered}");
        // Percentile labels render in the LineGauges.
        assert!(rendered.contains("p50"), "p50 label missing: {rendered}");
        assert!(rendered.contains("p90"), "p90 label missing: {rendered}");
        // Business-hours rows fall back to "—".
        assert!(
            rendered.contains("biz p50"),
            "biz p50 label missing: {rendered}"
        );
        assert!(
            rendered.contains("—"),
            "business-hours `—` sentinel missing: {rendered}"
        );
        // Histogram pane title.
        assert!(
            rendered.contains("Distribution"),
            "histogram pane missing: {rendered}"
        );
    }

    /// Empty histogram with `sample_count == 0` should fall through
    /// to the empty-state block, not crash.
    #[test]
    fn response_time_view_renders_empty_state_when_zero_samples() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::ResponseTime;
        state.response_time = Some(ResponseTimeSummary {
            direction: ResponseTimeDirection::IReplied,
            sample_count: 0,
            clock_p50_seconds: 0,
            clock_p90_seconds: 0,
            business_hours_p50_seconds: None,
            business_hours_p90_seconds: None,
            histogram: vec![],
        });
        let rendered = render_to_string(120, 30, |frame| {
            draw(frame, Rect::new(0, 0, 120, 30), &state, &theme());
        });
        assert!(
            rendered.contains("No reply pairs in scope")
                || rendered.contains("No response-time data"),
            "empty-state missing: {rendered}"
        );
    }

    /// Phase 2.5: contact asymmetry renders rows with email + counts.
    #[test]
    fn asymmetry_view_renders_rows() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Contacts;
        state.contacts_mode = ContactsMode::Asymmetry;
        state.asymmetry_rows = vec![ContactAsymmetryRow {
            email: "noreply@example.com".into(),
            display_name: None,
            total_inbound: 10,
            total_outbound: 0,
            asymmetry: 1.0,
            last_seen_at: chrono::Utc::now(),
        }];
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("noreply@example.com"));
        assert!(
            rendered.contains("1.00"),
            "asymmetry value missing: {rendered}"
        );
    }

    /// Phase 2.5 / Behavior 5: an error message replaces the table
    /// instead of leaving the view in a usable but empty state, so
    /// the user knows the request failed and isn't an empty result.
    #[test]
    fn loaded_error_replaces_table_with_error_block() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.error = Some("daemon unavailable".into());
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("Error"));
        assert!(rendered.contains("daemon unavailable"));
        assert!(
            !rendered.contains("No storage data"),
            "empty-state must not render alongside an error"
        );
    }

    /// Phase 0 cache: when a refresh is in flight AND the active view
    /// already has data, the renderer must keep painting that data
    /// instead of replacing it with a "Computing analytics..." block.
    /// Regression target: cycling tabs felt slow because every switch
    /// blanked the screen.
    #[test]
    fn refreshing_with_cached_data_keeps_view_visible() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.loading = true;
        state.storage_rows = vec![StorageBucket {
            key: "cached@example.com".into(),
            bytes: 2048,
            count: 3,
        }];
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(
            !rendered.contains("Computing analytics"),
            "cold-load block must NOT replace cached data: {rendered}"
        );
        assert!(
            rendered.contains("cached@example.com"),
            "cached row should still render: {rendered}"
        );
        assert!(
            rendered.contains("refreshing"),
            "refreshing indicator should be in the header: {rendered}"
        );
    }

    /// Phase 0 cache: a true cold load (loading + no data) still
    /// shows the "Computing analytics..." block.
    #[test]
    fn cold_load_still_blanks_view() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.loading = true;
        // No data populated.
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(
            rendered.contains("Computing analytics"),
            "cold-load block should render when no cached data exists: {rendered}"
        );
    }
}

#[cfg(test)]
mod cache_tests {
    use crate::app::{AnalyticsState, AnalyticsView, StorageMode};
    use mxr_core::types::{StorageBucket, StorageGroupBy};

    /// Tab-switch must NOT trigger a refetch when the destination
    /// view already has data and the cache is fresh.
    #[test]
    fn fresh_cache_skips_refetch_on_tab_switch() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.storage_rows = vec![StorageBucket {
            key: "k".into(),
            bytes: 1,
            count: 1,
        }];
        // Mark the Storage view as just refreshed.
        state.mark_refreshed();

        // After a successful refresh, refresh_pending should be
        // logically false (the dispatcher cleared it earlier). And
        // the destination view should report fresh.
        assert!(state.has_data_for_view(AnalyticsView::Storage));
        assert!(state.cache_is_fresh(AnalyticsView::Storage));
    }

    /// A view with no data is never fresh, regardless of TTL.
    #[test]
    fn empty_view_is_not_fresh() {
        let state = AnalyticsState::default();
        assert!(!state.has_data_for_view(AnalyticsView::Storage));
        assert!(!state.cache_is_fresh(AnalyticsView::Storage));
    }

    /// Cache key must change when filters change so freshness is
    /// scoped per-filter-combo. Different group_by → different key.
    #[test]
    fn cache_key_distinguishes_filter_combos() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.storage_mode = StorageMode::Breakdown;
        state.storage_group_by = StorageGroupBy::Sender;
        let key_sender = state.current_cache_key();
        state.storage_group_by = StorageGroupBy::Mimetype;
        let key_mime = state.current_cache_key();
        assert_ne!(
            key_sender, key_mime,
            "different group_by must produce distinct cache keys"
        );
    }

    /// `should_show_cold_load` is the gate that decides whether to
    /// blank the view. Stale data must keep the view visible.
    #[test]
    fn should_show_cold_load_only_without_data() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.loading = true;
        assert!(state.should_show_cold_load(), "no data → cold load");

        state.storage_rows = vec![StorageBucket {
            key: "k".into(),
            bytes: 1,
            count: 1,
        }];
        assert!(
            !state.should_show_cold_load(),
            "with cached data, refresh must not blank the view"
        );
        assert!(
            state.is_refreshing_with_data(),
            "should report refreshing-with-data so the badge renders"
        );
    }
}
