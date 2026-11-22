use chrono::{Duration, TimeZone, Utc};
use mxr_core::types::Address;
use mxr_tui::ui::mail_list::{
    format_attachment_chip, format_date_relative, format_sender, format_subject_line,
    format_thread_count_badge,
};

fn anchor() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 5, 7, 14, 30, 0).unwrap()
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
fn at_one_day_boundary_switches_to_weekday() {
    let now = anchor();
    let t = now - Duration::hours(24);
    assert_eq!(
        format_date_relative(&t, &now),
        "Mon",
        "24 hours flips from hours-format to weekday-format"
    );
}

#[test]
fn at_seven_day_boundary_switches_to_month_day() {
    let now = anchor();
    let t = now - Duration::days(7);
    assert_eq!(
        format_date_relative(&t, &now),
        "Apr 30",
        "7 days flips from weekday-format to month-day-format"
    );
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
    let now = anchor();
    let t = now + Duration::days(2);
    let formatted = format_date_relative(&t, &now);
    assert!(
        formatted.chars().filter(|c| *c == '/').count() >= 2,
        "future date should use month/day/year format, got {formatted:?}"
    );
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
fn thread_count_badge_marks_conversations_only() {
    assert_eq!(format_thread_count_badge(1), "");
    assert_eq!(format_thread_count_badge(2), "↔2");
    assert_eq!(format_thread_count_badge(12), "↔12");
}

#[test]
fn attachment_chip_uses_kibibytes_at_one_thousand_twenty_four() {
    assert_eq!(format_attachment_chip(true, 1024), "📎 1K");
}

#[test]
fn attachment_chip_uses_mebibytes_at_one_megabyte() {
    assert_eq!(format_attachment_chip(true, 1024 * 1024), "📎 1M");
}

#[test]
fn subject_line_includes_snippet_when_room_available() {
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
