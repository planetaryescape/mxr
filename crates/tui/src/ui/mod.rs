pub mod accounts_page;
pub mod activity_modal;
pub mod analytics_filter_modal;
pub mod analytics_page;
pub mod analytics_widgets;
pub mod attachment_modal;
pub mod briefing_modal;
pub mod bulk_confirm_modal;
pub mod calendar_invites_lens;
pub mod command_palette;
pub mod compose_picker;
pub mod deliveries_page;
pub mod diagnostics_page;
pub mod draft_options_modal;
pub mod drafts_modal;
pub mod error_modal;
pub mod expert_modal;
pub mod help_modal;
pub mod hint_bar;
pub mod label_picker;
pub mod mail_list;
pub mod message_view;
pub mod onboarding_modal;
pub mod owed_lens;
pub mod platform_modal;
pub mod reply_queue_modal;
pub mod rules_page;
pub mod sanitize;
pub mod save_attachment_modal;
pub mod saved_search_form;
pub mod saved_search_tabs;
pub mod screener_modal;
pub mod search_bar;
pub mod search_page;
pub mod search_query;
pub mod send_confirm_modal;
pub mod sender_profile_modal;
pub mod sidebar;
pub mod snippets_modal;
pub mod snooze_modal;
pub mod status_bar;
pub mod subscriptions_page;
pub mod summary_modal;
pub mod toasts;
pub mod unsubscribe_modal;
pub mod url_modal;
pub mod whois_modal;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Centered popup rect sized as a percentage of `area`. Single shared
/// implementation for every modal renderer — replaces the per-module
/// copies that had drifted into two different argument orders.
/// Centered rect that is `percent_x` wide but a *fixed* number of rows tall
/// (clamped to the area), so a modal can size itself to its content instead of
/// a fixed percentage that clips when content grows.
pub(crate) fn centered_rect_fixed_height(percent_x: u16, height: u16, area: Rect) -> Rect {
    let height = height.min(area.height);
    let top = area.height.saturating_sub(height) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod centered_rect_tests {
    use super::*;

    #[test]
    fn centered_rect_centers_within_area() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = centered_rect(60, 40, area);
        assert_eq!(popup.width, 60);
        assert_eq!(popup.height, 20);
        assert_eq!(popup.x, 20);
        assert_eq!(popup.y, 15);
    }
}
