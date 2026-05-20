#![cfg_attr(
    test,
    expect(
        clippy::panic,
        reason = "unit tests panic with fixture names when calendar parsing fails"
    )
)]

use chrono::{DateTime, Utc};
use icalendar::parser::{
    read_calendar, unfold, Component as ParsedCalendarComponent, Property as ParsedCalendarProperty,
};
use mail_parser::{Message, MessageParser, MimeHeaders};
use mxr_core::types::{
    Address, CalendarAttendee, CalendarMetadata, CalendarPerson, MessageMetadata, TextPlainFormat,
    UnsubscribeMethod,
};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct ParsedHeaders {
    pub from: Option<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub unsubscribe: UnsubscribeMethod,
    pub metadata: MessageMetadata,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("failed to parse RFC 5322 headers")]
    InvalidMessage,
}

pub fn raw_headers_from_pairs(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect()
}

pub fn parse_headers_from_pairs(
    headers: &[(String, String)],
    fallback_date: Option<DateTime<Utc>>,
) -> Result<ParsedHeaders, ParseError> {
    parse_headers_from_raw(&raw_headers_from_pairs(headers), fallback_date)
}

pub fn parse_headers_from_raw(
    raw_headers: &str,
    fallback_date: Option<DateTime<Utc>>,
) -> Result<ParsedHeaders, ParseError> {
    let mut raw_message = normalize_header_block(raw_headers);
    raw_message.push_str("\r\n");
    let parsed = MessageParser::default()
        .parse(raw_message.as_bytes())
        .ok_or(ParseError::InvalidMessage)?;
    Ok(extract_parsed_headers(
        &parsed,
        Some(normalize_header_block(raw_headers)),
        fallback_date,
    ))
}

pub fn parse_address_list(raw: &str) -> Vec<Address> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    parse_headers_from_pairs(&[("To".to_string(), raw.to_string())], Some(Utc::now()))
        .map(|parsed| parsed.to)
        .unwrap_or_default()
}

pub fn parse_message_metadata_from_raw(raw_message: &[u8]) -> Result<MessageMetadata, ParseError> {
    let parsed = MessageParser::default()
        .parse(raw_message)
        .ok_or(ParseError::InvalidMessage)?;
    let raw_headers = extract_raw_header_block(raw_message);
    Ok(extract_metadata(&parsed, raw_headers))
}

pub fn body_unsubscribe_from_html(html: &str) -> Option<UnsubscribeMethod> {
    static HREF_RE: OnceLock<Regex> = OnceLock::new();
    let re = HREF_RE.get_or_init(|| {
        Regex::new(r#"(?is)href\s*=\s*["']([^"']*(unsubscribe|opt-out|preferences)[^"']*)["']"#)
            .expect("body unsubscribe regex should compile")
    });
    re.captures(html).and_then(|caps| {
        caps.get(1).map(|url| UnsubscribeMethod::BodyLink {
            url: html_unescape(url.as_str()),
        })
    })
}

pub fn decode_format_flowed(text: &str, delsp: bool) -> String {
    let mut out = String::new();
    let mut current = String::new();

    for line in text.lines() {
        if line == "-- " {
            flush_paragraph(&mut out, &mut current);
            out.push_str("-- \n");
            continue;
        }

        if line.is_empty() {
            flush_paragraph(&mut out, &mut current);
            out.push('\n');
            continue;
        }

        let flowed = line.ends_with(' ');
        let segment = if flowed && delsp {
            line.trim_end_matches(' ')
        } else {
            line
        };

        current.push_str(segment);
        if flowed {
            if !delsp {
                current.push(' ');
            }
        } else {
            flush_paragraph(&mut out, &mut current);
        }
    }

    flush_paragraph(&mut out, &mut current);
    out.trim_end().to_string()
}

pub fn calendar_metadata_from_text(calendar_text: &str) -> Option<CalendarMetadata> {
    parse_calendar_metadata_from_text(calendar_text)
        .or_else(|| legacy_calendar_metadata_from_text(calendar_text))
}

/// Strict viewer-attendee match used by the send path. Errors when no
/// attendee matches an account address, or when multiple match (ambiguous
/// account aliases — send path refuses to guess).
pub fn matching_attendee_strict<'a>(
    calendar: &'a CalendarMetadata,
    account_emails: &[String],
) -> Result<&'a CalendarAttendee, MatchingAttendeeError> {
    let matches = calendar
        .attendees
        .iter()
        .filter(|attendee| {
            account_emails
                .iter()
                .any(|email| attendee.email.eq_ignore_ascii_case(email))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [attendee] => Ok(*attendee),
        [] => Err(MatchingAttendeeError::NoMatch),
        _ => Err(MatchingAttendeeError::Ambiguous),
    }
}

/// Lenient viewer-attendee match used by read-path body views (`viewer_partstat`
/// derivation). Returns `None` instead of erroring on multi-match so the UI
/// renders the three action buttons rather than failing to load. Only the
/// strict send path errors on ambiguity.
pub fn matching_attendee_lenient<'a>(
    calendar: &'a CalendarMetadata,
    account_emails: &[String],
) -> Option<&'a CalendarAttendee> {
    let mut iter = calendar.attendees.iter().filter(|attendee| {
        account_emails
            .iter()
            .any(|email| attendee.email.eq_ignore_ascii_case(email))
    });
    let first = iter.next()?;
    if iter.next().is_some() {
        return None;
    }
    Some(first)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchingAttendeeError {
    NoMatch,
    Ambiguous,
}

impl std::fmt::Display for MatchingAttendeeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMatch => write!(f, "No account address is an attendee on this invite"),
            Self::Ambiguous => write!(
                f,
                "Calendar invite has multiple attendees matching account addresses"
            ),
        }
    }
}

impl std::error::Error for MatchingAttendeeError {}

fn parse_calendar_metadata_from_text(calendar_text: &str) -> Option<CalendarMetadata> {
    let unfolded = unfold(calendar_text);
    let calendar = read_calendar(&unfolded).ok()?;
    let method = property_value(&calendar.properties, "METHOD").map(str::to_string);
    let event = calendar
        .components
        .iter()
        .find(|component| component.name.as_str().eq_ignore_ascii_case("VEVENT"))?;

    let summary = component_property_value(event, "SUMMARY").map(str::to_string);
    let uid = component_property_value(event, "UID").map(str::to_string);
    let sequence = component_property_value(event, "SEQUENCE").and_then(|value| value.parse().ok());
    let organizer = component_property(event, "ORGANIZER").map(calendar_person_from_property);
    let attendees = event
        .properties
        .iter()
        .filter(|property| property.name.as_str().eq_ignore_ascii_case("ATTENDEE"))
        .map(calendar_attendee_from_property)
        .collect::<Vec<_>>();
    let rsvp_requested = attendees.iter().any(|attendee| attendee.rsvp == Some(true));
    let mut warnings = Vec::new();

    if uid.is_none() {
        warnings.push("calendar invite is missing UID".to_string());
    }
    if organizer.is_none() {
        warnings.push("calendar invite is missing organizer".to_string());
    }
    if attendees.is_empty() {
        warnings.push("calendar invite has no attendees".to_string());
    }

    if method.is_some()
        || summary.is_some()
        || uid.is_some()
        || organizer.is_some()
        || !attendees.is_empty()
    {
        Some(CalendarMetadata {
            method,
            summary,
            component_kind: Some(event.name.as_str().to_string()),
            uid,
            sequence,
            recurrence_id: component_property_value(event, "RECURRENCE-ID").map(str::to_string),
            dtstamp: component_property_value(event, "DTSTAMP").map(str::to_string),
            starts_at: component_property_value(event, "DTSTART").map(str::to_string),
            ends_at: component_property_value(event, "DTEND").map(str::to_string),
            description: component_property_value(event, "DESCRIPTION").map(str::to_string),
            location: component_property_value(event, "LOCATION").map(str::to_string),
            status: component_property_value(event, "STATUS").map(str::to_string),
            rrule: component_property_value(event, "RRULE").map(str::to_string),
            organizer,
            attendees,
            rsvp_requested,
            raw_ics: Some(calendar_text.to_string()),
            warnings,
            viewer_partstat: None,
            viewer_attendee_email: None,
            is_update: false,
        })
    } else {
        None
    }
}

fn legacy_calendar_metadata_from_text(calendar_text: &str) -> Option<CalendarMetadata> {
    let mut method = None;
    let mut summary = None;

    for line in calendar_text.lines() {
        let line = line.trim();
        if method.is_none() {
            method = line
                .strip_prefix("METHOD:")
                .map(|value| value.trim().to_string());
        }
        if summary.is_none() {
            summary = line
                .strip_prefix("SUMMARY:")
                .map(|value| value.trim().to_string());
        }
        if method.is_some() && summary.is_some() {
            break;
        }
    }

    if method.is_some() || summary.is_some() {
        Some(CalendarMetadata {
            method,
            summary,
            component_kind: None,
            uid: None,
            sequence: None,
            recurrence_id: None,
            dtstamp: None,
            starts_at: None,
            ends_at: None,
            description: None,
            location: None,
            status: None,
            rrule: None,
            organizer: None,
            attendees: Vec::new(),
            rsvp_requested: false,
            raw_ics: Some(calendar_text.to_string()),
            warnings: vec!["calendar invite could not be parsed as RFC 5545".to_string()],
            viewer_partstat: None,
            viewer_attendee_email: None,
            is_update: false,
        })
    } else {
        None
    }
}

fn component_property<'a>(
    component: &'a ParsedCalendarComponent<'a>,
    name: &str,
) -> Option<&'a ParsedCalendarProperty<'a>> {
    component
        .properties
        .iter()
        .find(|property| property.name.as_str().eq_ignore_ascii_case(name))
}

fn component_property_value<'a>(
    component: &'a ParsedCalendarComponent<'a>,
    name: &str,
) -> Option<&'a str> {
    component_property(component, name).map(|property| property.val.as_str())
}

fn property_value<'a>(properties: &'a [ParsedCalendarProperty<'a>], name: &str) -> Option<&'a str> {
    properties
        .iter()
        .find(|property| property.name.as_str().eq_ignore_ascii_case(name))
        .map(|property| property.val.as_str())
}

fn calendar_person_from_property(property: &ParsedCalendarProperty<'_>) -> CalendarPerson {
    let uri = property.val.as_str().to_string();
    CalendarPerson {
        email: calendar_email_from_uri(&uri),
        name: property_param(property, "CN").map(str::to_string),
        uri: Some(uri),
    }
}

fn calendar_attendee_from_property(property: &ParsedCalendarProperty<'_>) -> CalendarAttendee {
    let uri = property.val.as_str().to_string();
    CalendarAttendee {
        email: calendar_email_from_uri(&uri),
        name: property_param(property, "CN").map(str::to_string),
        uri: Some(uri),
        partstat: property_param(property, "PARTSTAT").map(str::to_string),
        role: property_param(property, "ROLE").map(str::to_string),
        rsvp: property_param(property, "RSVP").map(|value| value.eq_ignore_ascii_case("TRUE")),
    }
}

fn property_param<'a>(property: &'a ParsedCalendarProperty<'a>, name: &str) -> Option<&'a str> {
    property
        .params
        .iter()
        .find(|param| param.key.as_str().eq_ignore_ascii_case(name))
        .and_then(|param| param.val.as_ref().map(|value| value.as_str()))
}

fn calendar_email_from_uri(uri: &str) -> String {
    uri.strip_prefix("mailto:")
        .or_else(|| uri.strip_prefix("MAILTO:"))
        .unwrap_or(uri)
        .to_string()
}

pub fn extract_parsed_headers(
    message: &Message<'_>,
    raw_headers: Option<String>,
    fallback_date: Option<DateTime<Utc>>,
) -> ParsedHeaders {
    ParsedHeaders {
        from: message.from().and_then(extract_first_addr),
        to: message.to().map(extract_addrs).unwrap_or_default(),
        cc: message.cc().map(extract_addrs).unwrap_or_default(),
        bcc: message.bcc().map(extract_addrs).unwrap_or_default(),
        subject: message
            .subject()
            .map(|subject| subject.to_string())
            .unwrap_or_default(),
        date: message
            .date()
            .and_then(|date| DateTime::from_timestamp(date.to_timestamp(), 0))
            .or(fallback_date)
            .unwrap_or_else(Utc::now),
        message_id_header: message.message_id().map(|id| format!("<{id}>")),
        in_reply_to: message
            .in_reply_to()
            .as_text_list()
            .and_then(|ids| ids.first().map(|id| format!("<{id}>"))),
        references: message
            .references()
            .as_text_list()
            .map(|ids| ids.iter().map(|id| format!("<{id}>")).collect())
            .unwrap_or_default(),
        unsubscribe: convert_unsubscribe(list_unsubscribe::parse_from_message(message)),
        metadata: extract_metadata(message, raw_headers),
    }
}

fn extract_metadata(message: &Message<'_>, raw_headers: Option<String>) -> MessageMetadata {
    let content_language = message
        .header_values("Content-Language")
        .flat_map(|value| {
            value
                .as_text()
                .unwrap_or_default()
                .split(',')
                .map(|lang| lang.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|lang| !lang.is_empty())
        .collect();

    let auth_results = message
        .header_values("Authentication-Results")
        .filter_map(|value| value.as_text().map(|value| value.to_string()))
        .collect();

    let list_id = message.list_id().as_text().map(|value| value.to_string());
    let text_plain_format = message.content_type().and_then(parse_text_plain_format);

    MessageMetadata {
        list_id,
        auth_results,
        content_language,
        text_plain_format,
        text_plain_source: None,
        text_html_source: None,
        calendar: None,
        raw_headers,
    }
}

fn parse_text_plain_format(content_type: &mail_parser::ContentType<'_>) -> Option<TextPlainFormat> {
    if !content_type.ctype().eq_ignore_ascii_case("text")
        || !content_type
            .subtype()
            .unwrap_or_default()
            .eq_ignore_ascii_case("plain")
    {
        return None;
    }

    let format = content_type.attribute("format");
    let delsp = content_type
        .attribute("delsp")
        .map(|value| value.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    match format {
        Some(value) if value.eq_ignore_ascii_case("flowed") => {
            Some(TextPlainFormat::Flowed { delsp })
        }
        _ => Some(TextPlainFormat::Fixed),
    }
}

/// Bridge between the public `list-unsubscribe` crate's enum and mxr's
/// 5-variant `UnsubscribeMethod`. The `BodyLink` variant is produced
/// elsewhere by HTML body scraping; this path only converts the four
/// header-derived variants.
fn convert_unsubscribe(method: list_unsubscribe::UnsubscribeMethod) -> UnsubscribeMethod {
    match method {
        list_unsubscribe::UnsubscribeMethod::OneClick { url } => {
            UnsubscribeMethod::OneClick { url: url.into() }
        }
        list_unsubscribe::UnsubscribeMethod::HttpLink { url } => {
            UnsubscribeMethod::HttpLink { url: url.into() }
        }
        list_unsubscribe::UnsubscribeMethod::Mailto { address, subject } => {
            UnsubscribeMethod::Mailto { address, subject }
        }
        list_unsubscribe::UnsubscribeMethod::None => UnsubscribeMethod::None,
    }
}

fn extract_first_addr(addr: &mail_parser::Address<'_>) -> Option<Address> {
    match addr {
        mail_parser::Address::List(list) => list.first().map(to_address),
        mail_parser::Address::Group(groups) => groups
            .first()
            .and_then(|group| group.addresses.first())
            .map(to_address),
    }
}

fn extract_addrs(addr: &mail_parser::Address<'_>) -> Vec<Address> {
    match addr {
        mail_parser::Address::List(list) => list.iter().map(to_address).collect(),
        mail_parser::Address::Group(groups) => groups
            .iter()
            .flat_map(|group| group.addresses.iter())
            .map(to_address)
            .collect(),
    }
}

fn to_address(addr: &mail_parser::Addr<'_>) -> Address {
    Address {
        name: addr.name().map(|name| name.to_string()),
        email: addr.address().unwrap_or_default().to_string(),
    }
}

fn normalize_header_block(raw_headers: &str) -> String {
    raw_headers
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn extract_raw_header_block(raw_message: &[u8]) -> Option<String> {
    let raw = String::from_utf8_lossy(raw_message);
    let header_block = raw
        .split("\r\n\r\n")
        .next()
        .or_else(|| raw.split("\n\n").next())?;
    Some(normalize_header_block(header_block))
}

fn flush_paragraph(out: &mut String, current: &mut String) {
    if current.is_empty() {
        return;
    }
    out.push_str(current);
    out.push('\n');
    current.clear();
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use mxr_test_support::{fixture_stem, standards_fixture_bytes, standards_fixture_names};
    use serde_json::json;

    #[test]
    fn parses_address_list_with_comments_and_quotes() {
        let addresses =
            parse_address_list("\"Last, First\" <first@example.com>, second@example.com");
        assert_eq!(addresses.len(), 2);
        assert_eq!(addresses[0].name.as_deref(), Some("Last, First"));
        assert_eq!(addresses[1].email, "second@example.com");
    }

    #[test]
    fn parses_unsubscribe_mailto_subject() {
        let parsed = parse_headers_from_pairs(
            &[(
                "List-Unsubscribe".to_string(),
                "<mailto:list@example.com?subject=unsubscribe>".to_string(),
            )],
            Some(Utc::now()),
        )
        .unwrap();
        assert!(
            matches!(
                &parsed.unsubscribe,
                UnsubscribeMethod::Mailto {
                    address,
                    subject: Some(subject)
                } if address == "list@example.com" && subject == "unsubscribe"
            ),
            "{:?}",
            parsed.unsubscribe
        );
    }

    #[test]
    fn decodes_format_flowed() {
        let text = "Hello there \r\nworld\r\n\r\nNext paragraph\r\n";
        assert_eq!(
            decode_format_flowed(text, false),
            "Hello there  world\n\nNext paragraph"
        );
    }

    #[test]
    fn extracts_body_unsubscribe_link() {
        let html = r#"<a href="https://example.com/unsubscribe?id=1">unsubscribe</a>"#;
        assert!(matches!(
            body_unsubscribe_from_html(html),
            Some(UnsubscribeMethod::BodyLink { url }) if url.contains("unsubscribe")
        ));
    }

    #[test]
    fn parses_actionable_calendar_invite_metadata() {
        let calendar = concat!(
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "PRODID:-//mxr test//calendar email//EN\r\n",
            "METHOD:REQUEST\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:planning-123@example.com\r\n",
            "SEQUENCE:2\r\n",
            "DTSTAMP:20240515T120000Z\r\n",
            "DTSTART:20240520T090000Z\r\n",
            "DTEND:20240520T093000Z\r\n",
            "SUMMARY:Planning meeting\r\n",
            "LOCATION:Room 4\r\n",
            "ORGANIZER;CN=Alice Smith:mailto:alice@example.com\r\n",
            "ATTENDEE;CN=Bob Example;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:bob@example.com\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        );

        let parsed = calendar_metadata_from_text(calendar).expect("calendar metadata");

        assert_eq!(parsed.method.as_deref(), Some("REQUEST"));
        assert_eq!(parsed.component_kind.as_deref(), Some("VEVENT"));
        assert_eq!(parsed.summary.as_deref(), Some("Planning meeting"));
        assert_eq!(parsed.uid.as_deref(), Some("planning-123@example.com"));
        assert_eq!(parsed.sequence, Some(2));
        assert_eq!(parsed.location.as_deref(), Some("Room 4"));
        assert_eq!(
            parsed
                .organizer
                .as_ref()
                .map(|person| person.email.as_str()),
            Some("alice@example.com")
        );
        assert_eq!(parsed.attendees.len(), 1);
        assert_eq!(parsed.attendees[0].email, "bob@example.com");
        assert_eq!(
            parsed.attendees[0].partstat.as_deref(),
            Some("NEEDS-ACTION")
        );
        assert_eq!(parsed.attendees[0].rsvp, Some(true));
        assert_eq!(parsed.raw_ics.as_deref(), Some(calendar));
    }

    #[test]
    fn parses_folded_calendar_summary_and_keeps_non_actionable_warning() {
        let calendar = concat!(
            "BEGIN:VCALENDAR\r\n",
            "METHOD:REQUEST\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:folded@example.com\r\n",
            "SUMMARY:Quarterly planning and \r\n",
            " roadmap review\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        );

        let parsed = calendar_metadata_from_text(calendar).expect("calendar metadata");

        assert_eq!(
            parsed.summary.as_deref(),
            Some("Quarterly planning and roadmap review")
        );
        assert_eq!(parsed.uid.as_deref(), Some("folded@example.com"));
        assert!(
            parsed
                .warnings
                .iter()
                .any(|warning| warning.contains("organizer")),
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn matching_attendee_strict_errors_on_zero_or_multiple_matches() {
        let calendar = CalendarMetadata {
            attendees: vec![
                CalendarAttendee {
                    email: "alice@example.com".into(),
                    name: None,
                    uri: None,
                    partstat: None,
                    role: None,
                    rsvp: None,
                },
                CalendarAttendee {
                    email: "bob@example.com".into(),
                    name: None,
                    uri: None,
                    partstat: None,
                    role: None,
                    rsvp: None,
                },
            ],
            ..CalendarMetadata::default()
        };

        let no_match = matching_attendee_strict(&calendar, &["zzz@example.com".into()]);
        assert!(matches!(no_match, Err(MatchingAttendeeError::NoMatch)));

        let single = matching_attendee_strict(&calendar, &["bob@example.com".into()]);
        assert_eq!(single.unwrap().email, "bob@example.com");

        let ambiguous = matching_attendee_strict(
            &calendar,
            &["alice@example.com".into(), "bob@example.com".into()],
        );
        assert!(matches!(ambiguous, Err(MatchingAttendeeError::Ambiguous)));
    }

    #[test]
    fn matching_attendee_lenient_returns_none_for_ambiguous() {
        let calendar = CalendarMetadata {
            attendees: vec![
                CalendarAttendee {
                    email: "alice@example.com".into(),
                    name: None,
                    uri: None,
                    partstat: None,
                    role: None,
                    rsvp: None,
                },
                CalendarAttendee {
                    email: "bob@example.com".into(),
                    name: None,
                    uri: None,
                    partstat: None,
                    role: None,
                    rsvp: None,
                },
            ],
            ..CalendarMetadata::default()
        };

        let none = matching_attendee_lenient(&calendar, &["zzz@example.com".into()]);
        assert!(none.is_none());

        let single = matching_attendee_lenient(&calendar, &["bob@example.com".into()]);
        assert_eq!(single.map(|a| a.email.as_str()), Some("bob@example.com"));

        let ambiguous = matching_attendee_lenient(
            &calendar,
            &["alice@example.com".into(), "bob@example.com".into()],
        );
        assert!(
            ambiguous.is_none(),
            "lenient path soft-fails on multi-match, unlike strict"
        );
    }

    #[test]
    fn calendar_metadata_backwards_compat_without_viewer_fields() {
        // Old JSON (no viewer_partstat, viewer_attendee_email, is_update)
        // must still deserialize successfully — verifies the additive serde
        // contract that lets a v3 daemon talk to a v2 client.
        let json = r#"{
            "method": "REQUEST",
            "summary": "Demo",
            "component_kind": "VEVENT",
            "uid": "abc",
            "attendees": [],
            "rsvp_requested": true,
            "warnings": []
        }"#;
        let parsed: CalendarMetadata = serde_json::from_str(json).expect("must deserialize");
        assert_eq!(parsed.viewer_partstat, None);
        assert_eq!(parsed.viewer_attendee_email, None);
        assert!(!parsed.is_update);
    }

    #[test]
    fn parses_real_world_calendar_methods_and_recurrence_identity() {
        let cases = [
            (
                "outlook update",
                concat!(
                    "BEGIN:VCALENDAR\r\n",
                    "PRODID:-//Microsoft Corporation//Outlook 16.0 MIMEDIR//EN\r\n",
                    "VERSION:2.0\r\n",
                    "METHOD:REQUEST\r\n",
                    "BEGIN:VEVENT\r\n",
                    "UID:outlook-series@example.com\r\n",
                    "SEQUENCE:7\r\n",
                    "RECURRENCE-ID;TZID=Europe/London:20260520T150000\r\n",
                    "DTSTART;TZID=Europe/London:20260520T160000\r\n",
                    "DTEND;TZID=Europe/London:20260520T163000\r\n",
                    "RRULE:FREQ=WEEKLY;COUNT=4\r\n",
                    "SUMMARY:Updated planning\r\n",
                    "ORGANIZER;CN=Alice:mailto:alice@example.com\r\n",
                    "ATTENDEE;CN=User;PARTSTAT=NEEDS-ACTION;ROLE=REQ-PARTICIPANT;RSVP=TRUE:mailto:user@example.com\r\n",
                    "END:VEVENT\r\n",
                    "END:VCALENDAR\r\n",
                ),
                "REQUEST",
                Some("FREQ=WEEKLY;COUNT=4"),
                Some("20260520T150000"),
            ),
            (
                "apple cancel",
                concat!(
                    "BEGIN:VCALENDAR\r\n",
                    "PRODID:-//Apple Inc.//macOS Calendar//EN\r\n",
                    "VERSION:2.0\r\n",
                    "METHOD:CANCEL\r\n",
                    "BEGIN:VEVENT\r\n",
                    "UID:apple-cancel@example.com\r\n",
                    "SEQUENCE:3\r\n",
                    "STATUS:CANCELLED\r\n",
                    "DTSTART:20260521T090000Z\r\n",
                    "DTEND:20260521T093000Z\r\n",
                    "SUMMARY:Cancelled review\r\n",
                    "ORGANIZER;CN=Alice:mailto:alice@example.com\r\n",
                    "ATTENDEE;CN=User;PARTSTAT=ACCEPTED:mailto:user@example.com\r\n",
                    "END:VEVENT\r\n",
                    "END:VCALENDAR\r\n",
                ),
                "CANCEL",
                None,
                None,
            ),
            (
                "thunderbird reply",
                concat!(
                    "BEGIN:VCALENDAR\r\n",
                    "PRODID:-//Mozilla.org/NONSGML Mozilla Calendar V1.1//EN\r\n",
                    "VERSION:2.0\r\n",
                    "METHOD:REPLY\r\n",
                    "BEGIN:VEVENT\r\n",
                    "UID:thunderbird-reply@example.com\r\n",
                    "SEQUENCE:1\r\n",
                    "SUMMARY:Planning\r\n",
                    "ORGANIZER:mailto:alice@example.com\r\n",
                    "ATTENDEE;PARTSTAT=DECLINED:mailto:user@example.com\r\n",
                    "END:VEVENT\r\n",
                    "END:VCALENDAR\r\n",
                ),
                "REPLY",
                None,
                None,
            ),
        ];

        for (name, calendar, method, rrule, recurrence_id) in cases {
            let parsed = calendar_metadata_from_text(calendar)
                .unwrap_or_else(|| panic!("expected {name} calendar fixture to parse"));

            assert_eq!(parsed.method.as_deref(), Some(method), "{name}");
            assert_eq!(parsed.component_kind.as_deref(), Some("VEVENT"), "{name}");
            assert_eq!(parsed.rrule.as_deref(), rrule, "{name}");
            assert_eq!(parsed.recurrence_id.as_deref(), recurrence_id, "{name}");
            assert!(parsed.uid.is_some(), "{name}");
            assert!(parsed.organizer.is_some(), "{name}");
            assert!(!parsed.attendees.is_empty(), "{name}");
        }
    }

    #[test]
    fn standards_fixture_folded_flowed_headers_snapshot() {
        let raw = standards_fixture_bytes("folded-flowed.eml");
        let parsed = parse_message_metadata_from_raw(&raw).unwrap();
        let headers =
            parse_headers_from_raw(&extract_raw_header_block(&raw).unwrap(), Some(Utc::now()))
                .unwrap();

        insta::assert_yaml_snapshot!(
            "folded_flowed_headers",
            json!({
                "from": headers.from.as_ref().map(|addr| json!({"name": addr.name.clone(), "email": addr.email.clone()})),
                "subject": headers.subject,
                "message_id": headers.message_id_header,
                "in_reply_to": headers.in_reply_to,
                "references": headers.references,
                "unsubscribe": format!("{:?}", headers.unsubscribe),
                "list_id": parsed.list_id,
                "auth_results": parsed.auth_results,
                "content_language": parsed.content_language,
                "text_plain_format": format!("{:?}", parsed.text_plain_format),
            })
        );
    }

    #[test]
    fn standards_fixture_minimal_message_metadata_snapshot() {
        let raw = standards_fixture_bytes("malformed-minimal.eml");
        let parsed = parse_message_metadata_from_raw(&raw).unwrap();
        insta::assert_yaml_snapshot!(
            "malformed_minimal_metadata",
            json!({
                "list_id": parsed.list_id,
                "auth_results": parsed.auth_results,
                "content_language": parsed.content_language,
                "text_plain_format": format!("{:?}", parsed.text_plain_format),
                "raw_headers_present": parsed.raw_headers.is_some(),
            })
        );
    }

    #[test]
    fn standards_fixture_header_matrix_snapshots() {
        for fixture in standards_fixture_names() {
            let raw = standards_fixture_bytes(fixture);
            let headers = extract_raw_header_block(&raw).unwrap_or_default();
            let parsed = parse_headers_from_raw(&headers, Some(Utc::now())).unwrap();
            let metadata = parse_message_metadata_from_raw(&raw).unwrap();

            insta::assert_yaml_snapshot!(
                format!("fixture_headers__{}", fixture_stem(fixture)),
                json!({
                    "from": parsed.from.as_ref().map(|addr| json!({"name": addr.name.clone(), "email": addr.email.clone()})),
                    "to": parsed.to.iter().map(|addr| json!({"name": addr.name.clone(), "email": addr.email.clone()})).collect::<Vec<_>>(),
                    "cc": parsed.cc.iter().map(|addr| json!({"name": addr.name.clone(), "email": addr.email.clone()})).collect::<Vec<_>>(),
                    "bcc": parsed.bcc.iter().map(|addr| json!({"name": addr.name.clone(), "email": addr.email.clone()})).collect::<Vec<_>>(),
                    "subject": parsed.subject,
                    "message_id": parsed.message_id_header,
                    "in_reply_to": parsed.in_reply_to,
                    "references": parsed.references,
                    "unsubscribe": format!("{:?}", parsed.unsubscribe),
                    "list_id": metadata.list_id,
                    "auth_results": metadata.auth_results,
                    "content_language": metadata.content_language,
                    "text_plain_format": format!("{:?}", metadata.text_plain_format),
                    "raw_headers_present": metadata.raw_headers.is_some(),
                })
            );
        }
    }
}
