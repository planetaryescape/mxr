//! Archive's subscriptions in the TUI: one row per subscription (what,
//! amount, cadence, next charge, status) under the totals per currency,
//! and a card with the charge history and price changes. Renders
//! `Request::ListRecordSubscriptions`; the daemon works out every value.

use mxr_protocol::{RecordSubscriptionData, RecordSubscriptionsData};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::ui::sanitize::{one_line, truncate};
use crate::ui::todo_lens::wrap;

fn date(at: chrono::DateTime<chrono::Utc>) -> String {
    // Days are stored at midday UTC, so UTC reads the day meant.
    at.format("%-d %b %Y").to_string()
}

fn amount_cell(subscription: &RecordSubscriptionData) -> String {
    let Some(amount) = &subscription.amount else {
        return String::new();
    };
    let unchecked = subscription
        .fields
        .iter()
        .any(|field| field.field == "amount" && !field.checked);
    format!(
        "{}{}",
        one_line(&amount.display),
        if unchecked { " \u{b7}?" } else { "  " }
    )
}

/// "next 3 Jul 2025", "overdue since 3 May 2025", "ended 3 Apr 2025".
fn when(subscription: &RecordSubscriptionData) -> String {
    match (subscription.status.as_str(), subscription.next_expected) {
        ("overdue", Some(next)) => format!("overdue since {}", date(next)),
        ("ended", _) | (_, None) => format!("ended, last {}", date(subscription.last_charge)),
        (_, Some(next)) => format!("next {}", date(next)),
    }
}

fn row(
    subscription: &RecordSubscriptionData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let marker = if selected { "\u{258c} " } else { "  " };
    let amount = format!("{:>14}", amount_cell(subscription));
    let cadence = format!("  {:<10}", subscription.cadence_label);
    let when = when(subscription);
    let title_width = width
        .saturating_sub(2 + amount.len() + cadence.len() + when.len() + 2)
        .max(12);
    let status_color = match subscription.status.as_str() {
        "overdue" => theme.warning,
        "ended" => theme.text_muted,
        _ => theme.text_secondary,
    };
    Line::from(vec![
        Span::raw(marker),
        Span::styled(
            format!(
                "{:<title_width$}",
                truncate(&one_line(&subscription.title), title_width)
            ),
            Style::default().fg(if subscription.status == "ended" {
                theme.text_muted
            } else {
                theme.text_primary
            }),
        ),
        Span::styled(amount, Style::default().fg(theme.text_primary)),
        Span::styled(cadence, Style::default().fg(theme.text_secondary)),
        Span::styled(format!("  {when}"), Style::default().fg(status_color)),
    ])
}

/// The list's lines and which one is selected.
pub(crate) fn lines(
    data: Option<&RecordSubscriptionsData>,
    selected: usize,
    width: usize,
    theme: &crate::theme::Theme,
) -> (Vec<Line<'static>>, usize) {
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    let text = |s: String, style: Style| Line::from(Span::styled(format!("  {s}"), style));
    let mut out = Vec::new();
    let mut selected_line = 0;
    let Some(data) = data else {
        out.push(text("Loading subscriptions…".into(), muted));
        return (out, 0);
    };
    out.push(text(one_line(&data.header), secondary));
    if !data.totals.is_empty() {
        let totals: Vec<String> = data
            .totals
            .iter()
            .map(|t| {
                format!(
                    "{} a month \u{b7} {} a year",
                    one_line(&t.per_month.display),
                    one_line(&t.per_year.display)
                )
            })
            .collect();
        out.push(text(
            format!("{} live: {}", data.live, totals.join("  +  ")),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ));
    }
    for signal in &data.signals {
        out.push(text(
            truncate(
                &format!("! {}", one_line(&signal.label)),
                width.saturating_sub(4),
            ),
            Style::default().fg(theme.warning),
        ));
    }
    if let Some(empty) = &data.empty_state {
        out.push(Line::from(""));
        for line in wrap(&one_line(empty), width.saturating_sub(4)) {
            out.push(text(line, secondary));
        }
    }
    let mut ended_header = false;
    for (index, subscription) in data.subscriptions.iter().enumerate() {
        if index == 0 && subscription.status != "ended" {
            out.push(Line::from(""));
        }
        if subscription.status == "ended" && !ended_header {
            out.push(Line::from(""));
            out.push(text("Ended".into(), muted.add_modifier(Modifier::BOLD)));
            ended_header = true;
        }
        if index == selected {
            selected_line = out.len();
        }
        out.push(row(subscription, index == selected, width, theme));
    }
    (out, selected_line)
}

/// The card: what it is, the fields with where they came from, the price
/// changes and every charge.
pub(crate) fn card_text(
    subscription: &RecordSubscriptionData,
    width: usize,
) -> Vec<(String, bool)> {
    let mut out = vec![
        (one_line(&subscription.title), true),
        (one_line(&subscription.status_reason), false),
        (String::new(), false),
    ];
    for field in &subscription.fields {
        let mark = if field.checked {
            "\u{25cf}"
        } else {
            "\u{25cb}"
        };
        out.push((
            truncate(
                &format!(
                    "{mark} {:<12} {}  ({})",
                    one_line(&field.label),
                    one_line(&field.value),
                    one_line(&field.source_label)
                ),
                width,
            ),
            false,
        ));
    }
    if !subscription.price_changes.is_empty() {
        out.push((String::new(), false));
        out.push(("Price changes".into(), true));
        for change in &subscription.price_changes {
            out.push((format!("  {}", one_line(&change.label)), false));
        }
    }
    out.push((String::new(), false));
    out.push((format!("{} charges", subscription.charge_count), true));
    for charge in subscription.charges.iter().rev() {
        out.push((
            format!(
                "  {:<12} {}{}",
                date(charge.date),
                charge
                    .amount
                    .as_ref()
                    .map(|a| one_line(&a.display))
                    .unwrap_or_default(),
                if charge.checked { "" } else { " \u{b7}?" }
            ),
            false,
        ));
    }
    if subscription.one_offs > 0 {
        out.push((String::new(), false));
        out.push((
            format!(
                "{} other {} from {} not part of it",
                subscription.one_offs,
                if subscription.one_offs == 1 {
                    "charge"
                } else {
                    "charges"
                },
                one_line(&subscription.issuer)
            ),
            false,
        ));
    }
    out
}

pub(crate) fn draw_card(
    frame: &mut Frame,
    area: Rect,
    subscription: &RecordSubscriptionData,
    theme: &crate::theme::Theme,
) {
    let popup = super::centered_rect(76, 80, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Subscription ")
        .title_bottom(" Y copy amount  o email  p issuer  Esc close ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let lines: Vec<Line> = card_text(subscription, inner.width.saturating_sub(1) as usize)
        .into_iter()
        .map(|(text, strong)| {
            Line::from(Span::styled(
                text,
                if strong {
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_secondary)
                },
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}
