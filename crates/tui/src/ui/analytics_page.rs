use crate::app::{AnalyticsState, AnalyticsView, ContactsMode, StorageMode, WrappedWindow};
use crate::ui::analytics_widgets::{
    big_text_banner, format_count, histogram_bar_chart, stat_card,
};
use mxr_core::types::{ResponseTimeDirection, StaleBallInCourt, StorageGroupBy};
use ratatui::prelude::*;
use ratatui::widgets::*;
use tui_big_text::PixelSize;

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

/// Standard tabular-tab layout: a 3-up "stat strip" of cards on top
/// and a detail table below. The strip is 4 rows; the table takes
/// the rest. We deliberately do NOT include a chart pane between —
/// charts that just illustrate a sorted column duplicate the table.
fn strip_and_table(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);
    (chunks[0], chunks[1])
}

/// Layout for tabs whose tail is a real distribution chart, not a
/// redundant bar list. Strip on top, chart in the middle, table at
/// the bottom. Used by Contacts Decay (the only tab with a genuine
/// distribution to render).
fn strip_chart_table(area: Rect, chart_h: u16) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(chart_h),
            Constraint::Min(0),
        ])
        .split(area);
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

fn four_up(area: Rect) -> [Rect; 4] {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);
    [cols[0], cols[1], cols[2], cols[3]]
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
        .map(|r| share_pct(r.bytes, total_bytes))
        .unwrap_or(0.0);

    let (strip, table) = strip_and_table(area);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Total", &format_bytes(total_bytes), theme, true);
    stat_card(frame, cards[1], "Items", &format_count(total_count), theme, false);
    stat_card(
        frame,
        cards[2],
        "Top share",
        &format!("{top_share:.1}%"),
        theme,
        false,
    );

    // `Share` (% of total bytes) replaces the redundant bar chart —
    // it answers "which keys disproportionately eat the mailbox?"
    // inline, in the same row as the count.
    let header = Row::new(vec!["Key", "Bytes", "Count", "Share"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .storage_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.key.clone(),
                format_bytes(row.bytes),
                row.count.to_string(),
                format!("{:.1}%", share_pct(row.bytes, total_bytes)),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(55),
        Constraint::Percentage(18),
        Constraint::Percentage(15),
        Constraint::Percentage(12),
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

fn share_pct(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64) / (total as f64) * 100.0
    }
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

    let (strip, table) = strip_and_table(area);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Stale", &format_count(count), theme, true);
    stat_card(frame, cards[1], "Oldest", &format!("{oldest}d"), theme, false);
    stat_card(frame, cards[2], "Median", &format!("{median}d"), theme, false);

    // No age-distribution histogram: the active filter
    // (older_than_days .. within_days) collapses every row into a
    // single bucket. The strip already says "oldest / median".
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
    let one_sided = state
        .asymmetry_rows
        .iter()
        .filter(|r| r.total_outbound == 0)
        .count() as u64;

    let (strip, table) = strip_and_table(area);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Contacts", &format_count(count), theme, true);
    stat_card(
        frame,
        cards[1],
        "One-sided",
        &format_count(one_sided),
        theme,
        false,
    );
    stat_card(
        frame,
        cards[2],
        "Avg asym.",
        &format!("{avg_asym:.2}"),
        theme,
        false,
    );
    let _ = max_asym; // surfaced via table column; one_sided is the more actionable summary

    // No bar chart: the table is already sorted by asymmetry, and
    // a bar of `total_inbound` next to `total_inbound` is just
    // visual duplication.
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

    // Stat strip on top + histogram below. We dropped the BigText
    // hero (one number doesn't deserve a billboard) and the
    // percentile-bar block (those scaled to p90, so the p90 bar
    // always rendered 100% full and conveyed nothing). Distribution
    // shape + p50/p90 text annotations beats both.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);
    let cards = four_up(chunks[0]);

    let direction_label = match summary.direction {
        ResponseTimeDirection::IReplied => "you reply",
        ResponseTimeDirection::TheyReplied => "they reply",
    };
    stat_card(
        frame,
        cards[0],
        "p50",
        &format_duration_seconds(summary.clock_p50_seconds),
        theme,
        true,
    );
    stat_card(
        frame,
        cards[1],
        "p90",
        &format_duration_seconds(summary.clock_p90_seconds),
        theme,
        false,
    );
    stat_card(
        frame,
        cards[2],
        "samples",
        &format!(
            "{} ({})",
            format_count(summary.sample_count as u64),
            direction_label
        ),
        theme,
        false,
    );
    let scope_value = match (
        state.response_time_counterparty.as_deref(),
        state.response_time_since_days,
    ) {
        (Some(cp), Some(d)) => format!("{}, {d}d", short_email(cp)),
        (Some(cp), None) => short_email(cp).to_string(),
        (None, Some(d)) => format!("{d}d"),
        (None, None) => "all-time".to_string(),
    };
    stat_card(frame, cards[3], "scope", &scope_value, theme, false);

    // Distribution histogram with p50/p90 callouts in the title — a
    // chart with an annotation that puts the percentiles where they
    // sit on the distribution. Bars containing p50/p90 are bolded.
    let p50_idx = histogram_bucket_index(summary.clock_p50_seconds);
    let p90_idx = histogram_bucket_index(summary.clock_p90_seconds);
    let p50_bucket = summary
        .histogram
        .get(p50_idx)
        .map(|b| histogram_bucket_label(b.upper_bound_seconds));
    let p90_bucket = summary
        .histogram
        .get(p90_idx)
        .map(|b| histogram_bucket_label(b.upper_bound_seconds));
    let title = match (p50_bucket, p90_bucket) {
        (Some(p50b), Some(p90b)) => format!(
            "Distribution · p50 {} (in {}) · p90 {} (in {})",
            format_duration_seconds(summary.clock_p50_seconds),
            p50b,
            format_duration_seconds(summary.clock_p90_seconds),
            p90b,
        ),
        _ => "Distribution".to_string(),
    };
    let buckets: Vec<(String, u64)> = summary
        .histogram
        .iter()
        .map(|b| {
            (
                histogram_bucket_label(b.upper_bound_seconds),
                b.count as u64,
            )
        })
        .collect();
    histogram_bar_chart(frame, chunks[1], &title, &buckets, theme);
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

/// Find which response-time histogram bucket a duration falls in,
/// using the same edges (`RESPONSE_TIME_HISTOGRAM_EDGES`) the store
/// uses to populate the histogram. Clamped to the last bucket.
fn histogram_bucket_index(seconds: u32) -> usize {
    use mxr_core::types::RESPONSE_TIME_HISTOGRAM_EDGES;
    for (i, edge) in RESPONSE_TIME_HISTOGRAM_EDGES.iter().enumerate() {
        if seconds < *edge {
            return i;
        }
    }
    RESPONSE_TIME_HISTOGRAM_EDGES.len().saturating_sub(1)
}

/// Truncate an email for display in tight spaces (stat cards, badges).
/// Keeps the local-part if possible, otherwise truncates with ellipsis.
fn short_email(email: &str) -> &str {
    if email.len() <= 18 {
        email
    } else {
        // Find first '@' and keep that bit if short.
        if let Some(at) = email.find('@') {
            if at <= 14 {
                return &email[..at];
            }
        }
        &email[..18]
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

    let (strip, table) = strip_and_table(area);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Sum", &format_bytes(total_bytes), theme, true);
    stat_card(frame, cards[1], "Messages", &format_count(count), theme, false);
    stat_card(frame, cards[2], "Biggest", &format_bytes(biggest), theme, false);

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

    let (strip, chart, table) = strip_chart_table(area, 10);
    let cards = three_up(strip);
    stat_card(frame, cards[0], "Cold contacts", &format_count(count), theme, true);
    stat_card(frame, cards[1], "Longest gap", &format!("{longest}d"), theme, false);
    stat_card(frame, cards[2], "Median gap", &format!("{median}d"), theme, false);

    // Decay buckets ARE genuinely a distribution (the threshold is
    // a floor, not a window — gaps spread across a long tail).
    // Keep the histogram; it's not redundant with the table.
    let mut buckets = [0u64; 4];
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

    let (strip, table) = strip_and_table(area);
    let cards = three_up(strip);
    stat_card(
        frame,
        cards[0],
        "Senders",
        &format_count(state.subscriptions.len() as u64),
        theme,
        true,
    );
    stat_card(frame, cards[1], "Messages", &format_count(total_msgs), theme, false);
    stat_card(
        frame,
        cards[2],
        "Avg open",
        &format!("{avg_open:.1}%"),
        theme,
        false,
    );

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
    // Open % replaces the redundant top-by-volume bar — it
    // surfaces the engagement question ("which lists am I ignoring?")
    // inline against the message count, where the answer lives.
    let (header, widths): (Row<'_>, [Constraint; 6]) = if state.subscriptions_rank {
        (
            Row::new(vec!["Sender", "Email", "Count", "Opened", "Open %", "Arch/Unrd"])
                .style(Style::default().fg(theme.text_muted).bold()),
            [
                Constraint::Percentage(22),
                Constraint::Percentage(30),
                Constraint::Percentage(9),
                Constraint::Percentage(11),
                Constraint::Percentage(11),
                Constraint::Percentage(17),
            ],
        )
    } else {
        (
            Row::new(vec!["Sender", "Email", "Count", "Open %", "Method", "Latest Subject"])
                .style(Style::default().fg(theme.text_muted).bold()),
            [
                Constraint::Percentage(18),
                Constraint::Percentage(23),
                Constraint::Percentage(8),
                Constraint::Percentage(8),
                Constraint::Percentage(11),
                Constraint::Percentage(32),
            ],
        )
    };
    let rows: Vec<Row> = indexed
        .into_iter()
        .map(|i| {
            let s = &state.subscriptions[i];
            let pct = open_rate(s.message_count, s.opened_count) * 100.0;
            if state.subscriptions_rank {
                Row::new(vec![
                    s.sender_name.clone().unwrap_or_default(),
                    s.sender_email.clone(),
                    s.message_count.to_string(),
                    s.opened_count.to_string(),
                    format!("{pct:.0}%"),
                    s.archived_unread_count.to_string(),
                ])
            } else {
                Row::new(vec![
                    s.sender_name.clone().unwrap_or_default(),
                    s.sender_email.clone(),
                    s.message_count.to_string(),
                    format!("{pct:.0}%"),
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

/// Wrapped year-in-review dashboard. A compact header (BigText
/// window label + window range + totals) sits above two rows of
/// three tiles — Volume, When, Contacts, Reply discipline, Storage,
/// Newsletters — and a full-width Superlatives strip. Each tile
/// answers ONE question with the most direct rendering for that
/// data shape:
///   * Volume:  the in:out ratio, not three mismatched bars.
///   * When:    a smooth 24h sparkline + AM/PM split, not 24 mini-bars.
///   * Contacts: top-1 with share %, not a stack of bars duplicating the count.
///   * Reply:   p50/p90 + named fastest/slowest, not a maxed-out gauge.
///   * Storage / Newsletters: stat lines, with empty state when data is absent.
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

    // Compact 5-row header: BigText (3 inner rows) + borders. Frees
    // ~3 extra rows of tile real estate vs the previous 8-row hero.
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(area);
    draw_wrapped_header(frame, outer_chunks[0], summary, theme);

    // Body: row1 | row2 | superlatives.
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
    let label = wrapped_short_label(&summary.label);
    big_text_banner(frame, cols[0], "Wrapped", &label, PixelSize::Sextant, theme);

    let total_msgs =
        summary.volume.inbound_count as u64 + summary.volume.outbound_count as u64;
    let info = vec![
        Line::from(Span::styled(
            format!(
                "{} → {}",
                summary.window_start.format("%Y-%m-%d"),
                summary.window_end.format("%Y-%m-%d"),
            ),
            theme.accent_style().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "{} messages · {} threads",
            format_count(total_msgs),
            format_count(summary.volume.thread_count as u64),
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.muted_style())
        .title(" Window ");
    frame.render_widget(
        Paragraph::new(info).block(block).wrap(Wrap { trim: false }),
        cols[1],
    );
}

/// Compact label for the BigText banner — `"2026 year-to-date"`
/// becomes `"2026 YTD"`, `"last 90 days"` becomes `"LAST 90D"`.
/// Keeps the banner narrow so the header doesn't wrap.
fn wrapped_short_label(label: &str) -> String {
    let upper = label.to_uppercase();
    upper
        .replace("YEAR-TO-DATE", "YTD")
        .replace(" DAYS", "D")
        .trim()
        .to_string()
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
    // Volume's story is the *ratio*, not three bars at mismatched
    // scales. With in=5214 and out=159 in the same chart, the `out`
    // bar disappears. Ratio + counts as text answers the actual
    // question ("am I lurking or participating?").
    let block = wrapped_tile_block("Volume", theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let v = &summary.volume;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);
    let ratio_text = ratio_label(v.inbound_count, v.outbound_count);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                ratio_text,
                theme.accent_style().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled("inbound : outbound", theme.muted_style())),
        ])
        .alignment(Alignment::Center),
        chunks[0],
    );
    let kv = vec![
        ("inbound".to_string(), format_count(v.inbound_count as u64)),
        ("outbound".to_string(), format_count(v.outbound_count as u64)),
        ("threads".to_string(), format_count(v.thread_count as u64)),
    ];
    let label_w = kv.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    let lines: Vec<Line> = kv
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("{label:<label_w$}"), theme.muted_style()),
                Span::raw("  "),
                Span::styled(value.clone(), theme.primary_style().add_modifier(Modifier::BOLD)),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        chunks[1],
    );
}

/// Pretty-print a ratio between two counts. Big sides get the
/// scale; equal counts collapse to "1:1". Zero on either side is
/// rendered explicitly as the count followed by ":0" or "0:" so
/// the reader doesn't see a confusing infinity.
fn ratio_label(a: u32, b: u32) -> String {
    if a == 0 && b == 0 {
        return "0:0".into();
    }
    if b == 0 {
        return format!("{a}:0");
    }
    if a == 0 {
        return format!("0:{b}");
    }
    let ratio = (a as f64) / (b as f64);
    if ratio >= 1.0 {
        format!("{:.0}:1", ratio)
    } else {
        format!("1:{:.0}", 1.0 / ratio)
    }
}

fn draw_wrapped_when(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    let pat = &summary.time_patterns;
    let peak = pat
        .busiest_hour_utc
        .map(|h| format!("{h:02}:00 UTC"))
        .unwrap_or_else(|| "—".into());
    let title = format!("When · peak {peak}");

    // Smooth 24h sparkline + AM/PM split subtitle. The shape (where
    // the curve rises/falls across the day) is what tells you
    // something; 24 individual bars labelled "0 0 0 1 1 2 2" are
    // just noise. Inline rather than via the sparkline_card helper
    // so we can preserve focus state on the outer wrapped_tile_block.
    let block = wrapped_tile_block(&title, theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let data: Vec<u64> = pat
        .hour_distribution
        .iter()
        .map(|c| *c as u64)
        .collect();
    let total: u64 = data.iter().sum();
    if total == 0 {
        frame.render_widget(
            Paragraph::new("(no time-of-day data)").style(theme.muted_style()),
            inner,
        );
        return;
    }
    let am_total: u64 = data[0..12].iter().sum();
    let pm_total: u64 = data[12..24].iter().sum();
    let evening: u64 = data[18..24].iter().sum();
    let subtitle = format!(
        "AM {:.0}% · PM {:.0}% · evening {:.0}%",
        (am_total as f64) / (total as f64) * 100.0,
        (pm_total as f64) / (total as f64) * 100.0,
        (evening as f64) / (total as f64) * 100.0,
    );

    // Inner split: sparkline | axis | subtitle.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let max = data.iter().copied().max().unwrap_or(1).max(1);
    frame.render_widget(
        Sparkline::default()
            .data(&data)
            .max(max)
            .style(theme.accent_style().add_modifier(Modifier::BOLD)),
        chunks[0],
    );
    let axis_w = chunks[1].width as usize;
    let mut axis = String::with_capacity(axis_w);
    axis.push_str("00");
    let pad = axis_w.saturating_sub(4);
    axis.push_str(&" ".repeat(pad));
    axis.push_str("23");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(axis, theme.muted_style()))),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(subtitle, theme.secondary_style()))),
        chunks[2],
    );
}

fn draw_wrapped_contacts(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    // Top-1 with share % beats listing 5 senders next to bars
    // that duplicate the count. The interesting fact is the
    // *concentration* (one sender = X% of inbound), not the
    // ranking the table already shows.
    let block = wrapped_tile_block("Top inbound", theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let total_in = summary.volume.inbound_count as u64;
    let top = summary.top_contacts.most_emailed_to_me.first();
    let top5_share: u64 = summary
        .top_contacts
        .most_emailed_to_me
        .iter()
        .take(5)
        .map(|c| c.count as u64)
        .sum();
    let lines = match top {
        Some(c) => {
            let pct = share_pct(c.count as u64, total_in);
            let top5_pct = share_pct(top5_share, total_in);
            vec![
                Line::from(Span::styled(
                    c.email.clone(),
                    theme.primary_style().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{} msgs · {pct:.1}% of inbound", format_count(c.count as u64)),
                    theme.accent_style(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("top 5 = {top5_pct:.0}% of inbound"),
                    theme.muted_style(),
                )),
            ]
        }
        None => vec![Line::from(Span::styled(
            "(no inbound senders)",
            theme.muted_style(),
        ))],
    };
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_wrapped_reply(
    frame: &mut Frame,
    area: Rect,
    summary: &mxr_core::types::WrappedSummary,
    theme: &crate::theme::Theme,
    focused: bool,
) {
    // p50/p90 as numbers + the named fastest/slowest extremes that
    // already exist in WrappedReplyDiscipline. Names anchor stats —
    // "you replied in 12s to bob@x.com" lands harder than a gauge
    // bar that's 100% full because it's scaled to its own max.
    let Some(reply) = summary.reply_discipline.as_ref() else {
        let block = wrapped_tile_block("Reply discipline", theme, focused);
        frame.render_widget(
            Paragraph::new("(no reply pairs yet)")
                .style(theme.muted_style())
                .block(block),
            area,
        );
        return;
    };

    let title = format!(
        "Reply discipline · samples {}",
        format_count(reply.sample_count as u64)
    );
    let block = wrapped_tile_block(&title, theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    // Top: p50 · p90 (clock and biz on one line each).
    let p50p90 = vec![
        Line::from(vec![
            Span::styled("p50 ", theme.muted_style()),
            Span::styled(
                format_duration_seconds(reply.clock_p50_seconds),
                theme.accent_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("p90 ", theme.muted_style()),
            Span::styled(
                format_duration_seconds(reply.clock_p90_seconds),
                theme.accent_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        match (reply.business_hours_p50_seconds, reply.business_hours_p90_seconds) {
            (Some(p50), Some(p90)) => Line::from(vec![
                Span::styled("biz p50 ", theme.muted_style()),
                Span::styled(format_duration_seconds(p50), theme.primary_style()),
                Span::raw("   "),
                Span::styled("biz p90 ", theme.muted_style()),
                Span::styled(format_duration_seconds(p90), theme.primary_style()),
            ]),
            _ => Line::from(Span::styled("(business-hours pending)", theme.muted_style())),
        },
    ];
    frame.render_widget(
        Paragraph::new(p50p90).wrap(Wrap { trim: false }),
        chunks[0],
    );

    // Bottom: fastest / slowest with names attached.
    let mut extremes = Vec::new();
    if let Some(f) = reply.fastest.as_ref() {
        extremes.push(Line::from(vec![
            Span::styled("fastest  ", theme.muted_style()),
            Span::styled(
                format_duration_seconds(f.latency_seconds),
                theme.success_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · "),
            Span::styled(short_email(&f.counterparty_email).to_string(), theme.primary_style()),
        ]));
    }
    if let Some(s) = reply.slowest.as_ref() {
        extremes.push(Line::from(vec![
            Span::styled("slowest  ", theme.muted_style()),
            Span::styled(
                format_duration_seconds(s.latency_seconds),
                theme.error_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · "),
            Span::styled(short_email(&s.counterparty_email).to_string(), theme.primary_style()),
        ]));
    }
    if extremes.is_empty() {
        extremes.push(Line::from(Span::styled(
            "(no extremes recorded)",
            theme.muted_style(),
        )));
    }
    frame.render_widget(
        Paragraph::new(extremes).wrap(Wrap { trim: false }),
        chunks[1],
    );
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
    if inner.height == 0 {
        return;
    }

    // Empty-state when there's nothing to show — a "Top mime share"
    // gauge with 0% and no label is just a broken-looking box.
    let no_top = storage.top_mimetype.is_none();
    let no_heaviest = storage.heaviest_message.is_none();
    if no_top && no_heaviest {
        frame.render_widget(
            Paragraph::new("(no attachments)").style(theme.muted_style()),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    if let Some(top) = storage.top_mimetype.as_ref() {
        let pct = share_pct(top.bytes, storage.total_bytes);
        lines.push(Line::from(vec![
            Span::styled("top mime  ", theme.muted_style()),
            Span::styled(
                top.key.clone(),
                theme.primary_style().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("          "),
            Span::styled(
                format!("{} · {pct:.1}%", format_bytes(top.bytes)),
                theme.accent_style(),
            ),
        ]));
        lines.push(Line::from(""));
    }
    if let Some(heaviest) = storage.heaviest_message.as_ref() {
        let subject: String = heaviest.subject.chars().take(40).collect();
        lines.push(Line::from(vec![
            Span::styled("heaviest  ", theme.muted_style()),
            Span::styled(subject, theme.primary_style()),
        ]));
        lines.push(Line::from(vec![
            Span::raw("          "),
            Span::styled(
                format_bytes(heaviest.size_bytes),
                theme.accent_style().add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
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

    // Empty state instead of an empty `Gauge` and "0 lists" label
    // when no list-id headers are detected. Renders as muted text,
    // not a broken-looking box.
    if news.unique_lists == 0 && news.top_list.is_none() {
        let block = wrapped_tile_block("Newsletters", theme, focused);
        frame.render_widget(
            Paragraph::new("(no list-id headers detected)")
                .style(theme.muted_style())
                .block(block),
            area,
        );
        return;
    }

    let title = format!(
        "Newsletters · {} lists · {:.1}% of inbound",
        news.unique_lists, news.list_share_of_inbound_pct
    );
    let block = wrapped_tile_block(&title, theme, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let mut lines: Vec<Line> = Vec::new();
    if let Some(top) = news.top_list.as_ref() {
        let opened_pct = if top.message_count == 0 {
            0.0
        } else {
            (top.opened_count as f64) / (top.message_count as f64) * 100.0
        };
        let id: String = top.list_id.chars().take(40).collect();
        lines.push(Line::from(vec![
            Span::styled("top list  ", theme.muted_style()),
            Span::styled(
                id,
                theme.primary_style().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("          "),
            Span::styled(
                format!("{} msgs · {opened_pct:.0}% opened", top.message_count),
                theme.accent_style(),
            ),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
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

    /// Response Time view renders a 4-card stat strip (p50, p90,
    /// samples, scope) and a histogram with p50/p90 callouts in
    /// the title. No more BigText hero, no more percentile-bar
    /// block — both were removed because they didn't add info a
    /// number couldn't carry.
    #[test]
    fn response_time_view_renders_stat_strip_and_annotated_histogram() {
        use mxr_core::types::{ResponseTimeBucket, RESPONSE_TIME_HISTOGRAM_EDGES};
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::ResponseTime;
        state.response_time = Some(ResponseTimeSummary {
            direction: ResponseTimeDirection::IReplied,
            sample_count: 17,
            clock_p50_seconds: 90,
            clock_p90_seconds: 3600,
            business_hours_p50_seconds: None,
            business_hours_p90_seconds: None,
            // Production data always has all 8 buckets.
            histogram: RESPONSE_TIME_HISTOGRAM_EDGES
                .iter()
                .enumerate()
                .map(|(i, &edge)| ResponseTimeBucket {
                    upper_bound_seconds: edge,
                    count: if i < 3 { (i as u32 + 1) * 3 } else { 0 },
                })
                .collect(),
        });
        let rendered = render_to_string(160, 30, |frame| {
            draw(frame, Rect::new(0, 0, 160, 30), &state, &theme());
        });
        // Stat strip: card labels.
        assert!(rendered.contains("p50"), "p50 card missing: {rendered}");
        assert!(rendered.contains("p90"), "p90 card missing: {rendered}");
        assert!(
            rendered.contains("samples"),
            "samples card missing: {rendered}"
        );
        assert!(rendered.contains("scope"), "scope card missing: {rendered}");
        // Direction phrasing in the samples card.
        assert!(
            rendered.contains("you reply"),
            "direction phrasing missing: {rendered}"
        );
        // Sample count.
        assert!(rendered.contains("17"), "sample count missing: {rendered}");
        // Histogram pane title carries the percentile annotations.
        assert!(
            rendered.contains("Distribution"),
            "histogram pane missing: {rendered}"
        );
        assert!(
            rendered.contains("p50 1m30s"),
            "p50 annotation missing: {rendered}"
        );
        assert!(
            rendered.contains("p90 1h0m"),
            "p90 annotation missing: {rendered}"
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
