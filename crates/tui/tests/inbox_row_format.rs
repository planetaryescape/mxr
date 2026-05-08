//! Behavior tests for inbox-row formatting helpers.
//!
//! These pin the relative-time ladder ("5m" / "3h" / "Tue" / "Mar 4")
//! that appears in the inbox row's date column. Tests use a fixed `now`
//! anchor so they're deterministic regardless of when they're run.

use chrono::{Duration, TimeZone, Utc};
use mxr_core::types::Address;
use mxr_tui::ui::mail_list::{
    format_attachment_chip, format_date_relative, format_sender, format_subject_line,
};

fn anchor() -> chrono::DateTime<Utc> {
    // A fixed Tuesday afternoon, May 7 2024. Avoids weekday/calendar
    // boundary surprises during local debugging.
    Utc.with_ymd_and_hms(2024, 5, 7, 14, 30, 0).unwrap()
}

#[test]
fn under_one_minute_renders_as_now() {
    let now = anchor();
    let t = now - Duration::seconds(30);
    assert_eq!(format_date_relative(&t, &now), "now");
}

#[test]
fn within_the_hour_renders_as_minutes() {
    let now = anchor();
    let t = now - Duration::minutes(5);
    assert_eq!(format_date_relative(&t, &now), "5m");
}

#[test]
fn at_one_hour_boundary_switches_to_hours() {
    let now = anchor();
    let t = now - Duration::minutes(60);
    assert_eq!(
        format_date_relative(&t, &now),
        "1h",
        "60 minutes flips from minutes-format to hours-format"
    );
}

#[test]
fn within_the_day_renders_as_hours() {
    let now = anchor();
    let t = now - Duration::hours(3);
    assert_eq!(format_date_relative(&t, &now), "3h");
}

#[test]
fn at_one_day_boundary_switches_to_weekday() {
    let now = anchor();
    let t = now - Duration::hours(24);
    // 24 hours before a Tuesday afternoon is a Monday afternoon.
    assert_eq!(
        format_date_relative(&t, &now),
        "Mon",
        "24 hours flips from hours-format to weekday-format"
    );
}

#[test]
fn within_the_week_renders_as_weekday() {
    let now = anchor();
    let t = now - Duration::days(3);
    // 3 days before Tuesday May 7 is Saturday May 4.
    assert_eq!(format_date_relative(&t, &now), "Sat");
}

#[test]
fn at_seven_day_boundary_switches_to_month_day() {
    let now = anchor();
    let t = now - Duration::days(7);
    // 7 days before May 7 is April 30.
    assert_eq!(
        format_date_relative(&t, &now),
        "Apr 30",
        "7 days flips from weekday-format to month-day-format"
    );
}

#[test]
fn older_than_a_week_in_same_year_renders_as_month_day() {
    let now = anchor();
    let t = Utc.with_ymd_and_hms(2024, 3, 4, 9, 0, 0).unwrap();
    assert_eq!(format_date_relative(&t, &now), "Mar 4");
}

#[test]
fn older_than_a_year_includes_year() {
    let now = anchor();
    let t = Utc.with_ymd_and_hms(2023, 11, 22, 9, 0, 0).unwrap();
    let formatted = format_date_relative(&t, &now);
    assert!(
        formatted.contains("23"),
        "year-different format should disambiguate the year, got {formatted:?}"
    );
}

#[test]
fn future_date_falls_back_to_absolute_format() {
    // Future-dated email shouldn't say "now" or weird negative tenses;
    // fall back to a stable absolute format.
    let now = anchor();
    let t = now + Duration::days(2);
    let formatted = format_date_relative(&t, &now);
    assert!(
        formatted.chars().filter(|c| *c == '/').count() >= 2,
        "future date should use month/day/year format, got {formatted:?}"
    );
}

#[test]
fn sender_uses_display_name_when_present() {
    let addr = Address {
        name: Some("Alice Example".into()),
        email: "alice@example.com".into(),
    };
    assert_eq!(format_sender(&addr, 18), "Alice Example");
}

#[test]
fn sender_falls_back_to_email_when_display_name_absent() {
    let addr = Address {
        name: None,
        email: "alice@example.com".into(),
    };
    assert_eq!(format_sender(&addr, 18), "alice@example.com");
}

#[test]
fn sender_falls_back_to_email_when_display_name_empty() {
    let addr = Address {
        name: Some("   ".into()),
        email: "alice@example.com".into(),
    };
    assert_eq!(
        format_sender(&addr, 18),
        "alice@example.com",
        "blank-only display names treated as absent"
    );
}

#[test]
fn sender_truncates_long_display_name_with_ellipsis() {
    let addr = Address {
        name: Some("Alexander Septimus Pemberton".into()),
        email: "alex@example.com".into(),
    };
    let formatted = format_sender(&addr, 18);
    assert_eq!(
        formatted.chars().count(),
        18,
        "truncated text occupies exactly the max width"
    );
    assert!(
        formatted.ends_with('…'),
        "truncation marked with trailing ellipsis: {formatted:?}"
    );
    assert!(
        formatted.starts_with("Alexander"),
        "truncation preserves the original prefix"
    );
}

#[test]
fn sender_passes_through_text_at_or_below_max_width() {
    // Boundary: text at exactly max width should NOT be truncated.
    let addr = Address {
        name: Some("Alice Anderson Z".into()), // 16 chars
        email: "alice@example.com".into(),
    };
    let formatted = format_sender(&addr, 16);
    assert_eq!(formatted, "Alice Anderson Z");
    assert!(!formatted.contains('…'));
}

#[test]
fn attachment_chip_is_empty_when_no_attachments() {
    assert_eq!(format_attachment_chip(false, 0), "");
    assert_eq!(
        format_attachment_chip(false, 99_999),
        "",
        "size is irrelevant when has_attachments is false"
    );
}

#[test]
fn attachment_chip_uses_bytes_for_small_messages() {
    assert_eq!(format_attachment_chip(true, 512), "📎 512B");
}

#[test]
fn attachment_chip_uses_kibibytes_at_one_thousand_twenty_four() {
    // Boundary: exactly 1024 bytes is the KiB threshold.
    assert_eq!(format_attachment_chip(true, 1024), "📎 1K");
}

#[test]
fn attachment_chip_uses_kibibytes_for_kilobyte_messages() {
    assert_eq!(format_attachment_chip(true, 45 * 1024), "📎 45K");
}

#[test]
fn attachment_chip_uses_mebibytes_at_one_megabyte() {
    // Boundary: exactly 1 MiB.
    assert_eq!(format_attachment_chip(true, 1024 * 1024), "📎 1M");
}

#[test]
fn attachment_chip_uses_mebibytes_for_megabyte_messages() {
    assert_eq!(format_attachment_chip(true, 5 * 1024 * 1024 + 100), "📎 5M");
}

#[test]
fn subject_line_omits_snippet_when_snippet_is_blank() {
    let (subject, snippet) = format_subject_line("Project update", "   ", 80);
    assert_eq!(subject, "Project update");
    assert!(snippet.is_none(), "blank snippets should be omitted");
}

#[test]
fn subject_line_includes_snippet_when_room_available() {
    // 80 chars is plenty of room for "Project update" + " · " + snippet.
    let (subject, snippet) = format_subject_line(
        "Project update",
        "Status meeting moved to Tuesday at 10am.",
        80,
    );
    assert_eq!(subject, "Project update");
    assert_eq!(
        snippet.as_deref(),
        Some("Status meeting moved to Tuesday at 10am."),
        "snippet preserved when there's room"
    );
}

#[test]
fn subject_line_truncates_long_snippet_with_ellipsis() {
    // Width 30: "Project update" (14) + " · " (3) = 17, leaves 13 for snippet.
    let (subject, snippet) = format_subject_line(
        "Project update",
        "Status meeting moved to Tuesday at 10am.",
        30,
    );
    assert_eq!(subject, "Project update");
    let snippet = snippet.expect("snippet present at width 30");
    assert!(
        snippet.ends_with('…'),
        "long snippet ellipsised: {snippet:?}"
    );
    assert_eq!(snippet.chars().count(), 13);
}

#[test]
fn subject_line_omits_snippet_when_row_too_narrow() {
    // Width 18: "Project update" (14) + " · " (3) = 17, only 1 char left
    // for snippet — not worth showing.
    let (subject, snippet) =
        format_subject_line("Project update", "Long detailed snippet content", 18);
    assert_eq!(subject, "Project update");
    assert!(
        snippet.is_none(),
        "snippet omitted when not enough room for a useful preview"
    );
}

#[test]
fn subject_line_truncates_subject_when_subject_alone_overflows() {
    let (subject, snippet) = format_subject_line(
        "An extraordinarily long email subject line",
        "Snippet text",
        20,
    );
    assert_eq!(subject.chars().count(), 20);
    assert!(subject.ends_with('…'));
    assert!(
        snippet.is_none(),
        "no snippet when subject alone fills the column"
    );
}
