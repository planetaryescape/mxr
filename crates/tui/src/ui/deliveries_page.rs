//! Rendering for the Deliveries screen.

use crate::app::DeliveriesState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &DeliveriesState, theme: &crate::theme::Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    draw_table(frame, chunks[0], state, theme);
    draw_footer(frame, chunks[1], state, theme);
}

fn draw_table(frame: &mut Frame, area: Rect, state: &DeliveriesState, theme: &crate::theme::Theme) {
    let title = format!(
        " Deliveries — {} ({}) ",
        state.filter.label(),
        state.rows.len()
    );

    if let Some(error) = &state.error {
        let p = Paragraph::new(format!("Failed to load deliveries:\n{error}"))
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(theme.error));
        frame.render_widget(p, area);
        return;
    }
    if state.rows.is_empty() {
        let msg = if state.loading {
            "Loading deliveries…"
        } else {
            "No deliveries. Shipping emails show up here as they arrive."
        };
        let p = Paragraph::new(msg)
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(theme.text_muted));
        frame.render_widget(p, area);
        return;
    }

    let widths = [
        Constraint::Length(18), // status
        Constraint::Length(22), // merchant
        Constraint::Length(10), // carrier
        Constraint::Length(12), // ETA
        Constraint::Fill(1),    // tracking / order
    ];
    let header = Row::new(["Status", "Merchant", "Carrier", "ETA", "Tracking"])
        .style(Style::default().fg(theme.text_muted).bold());

    let rows = state.rows.iter().map(|d| {
        let merchant = d
            .merchant
            .as_deref()
            .or(d.carrier.as_deref())
            .unwrap_or("?");
        let eta = d
            .delivered_at
            .or(d.eta_until)
            .or(d.eta_from)
            .map(|t| t.format("%b %d").to_string())
            .unwrap_or_else(|| "—".to_string());
        let tracking = d
            .tracking_number
            .clone()
            .or_else(|| d.order_number.as_ref().map(|o| format!("#{o}")))
            .unwrap_or_else(|| "—".to_string());
        Row::new([
            status_label(&d.status).to_string(),
            truncate(merchant, 22),
            d.carrier.clone().unwrap_or_else(|| "—".to_string()),
            eta,
            tracking,
        ])
    });

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL))
        .row_highlight_style(Style::default().bg(theme.selection_bg))
        .column_spacing(1);

    let mut table_state = TableState::default();
    table_state.select(Some(state.selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn draw_footer(
    frame: &mut Frame,
    area: Rect,
    _state: &DeliveriesState,
    theme: &crate::theme::Theme,
) {
    let help = "j/k move · r resolve · d dismiss · D filter · g refresh";
    let p = Paragraph::new(help).style(Style::default().fg(theme.text_muted));
    frame.render_widget(p, area);
}

fn status_label(status: &str) -> &str {
    match status {
        "ordered" => "Ordered",
        "info_received" => "Label created",
        "in_transit" => "In transit",
        "out_for_delivery" => "Out for delivery",
        "attempt_fail" => "Attempt failed",
        "available_for_pickup" => "Ready for pickup",
        "delivered" => "Delivered",
        "exception" => "Exception",
        "returned" => "Returned",
        "expired" => "Expired",
        other => other,
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}
