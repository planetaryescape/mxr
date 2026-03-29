use chrono::{DateTime, Utc};
use mail_parser::{Message, MessageParser, MimeHeaders};
use mxr_core::types::{
    Address, CalendarMetadata, MessageMetadata, TextPlainFormat, UnsubscribeMethod,
};
use regex::Regex;
use std::sync::OnceLock;
use url::Url;

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
            .unwrap()
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
        Some(CalendarMetadata { method, summary })
    } else {
        None
    }
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
        unsubscribe: parse_list_unsubscribe(message),
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

fn parse_list_unsubscribe(message: &Message<'_>) -> UnsubscribeMethod {
    let entries: Vec<String> = match message.list_unsubscribe().as_address() {
        Some(mail_parser::Address::List(list)) => list
            .iter()
            .filter_map(|addr| addr.address.as_ref().map(|value| value.to_string()))
            .collect(),
        Some(mail_parser::Address::Group(groups)) => groups
            .iter()
            .flat_map(|group| group.addresses.iter())
            .filter_map(|addr| addr.address.as_ref().map(|value| value.to_string()))
            .collect(),
        None => Vec::new(),
    };
    if entries.is_empty() {
        return UnsubscribeMethod::None;
    }

    let one_click = message
        .header_raw("List-Unsubscribe-Post")
        .map(|value| value.to_ascii_lowercase())
        .map(|value| value.contains("list-unsubscribe=one-click"))
        .unwrap_or(false);

    if one_click {
        if let Some(url) = entries
            .iter()
            .find(|entry| entry.starts_with("https://") || entry.starts_with("http://"))
        {
            return UnsubscribeMethod::OneClick {
                url: url.to_string(),
            };
        }
    }

    for entry in &entries {
        if let Some(mailto) = entry.strip_prefix("mailto:") {
            return parse_mailto_unsubscribe(mailto);
        }
    }

    if let Some(url) = entries
        .iter()
        .find(|entry| entry.starts_with("https://") || entry.starts_with("http://"))
    {
        return UnsubscribeMethod::HttpLink {
            url: url.to_string(),
        };
    }

    UnsubscribeMethod::None
}

fn parse_mailto_unsubscribe(mailto: &str) -> UnsubscribeMethod {
    let mut subject = None;
    let address = if let Some((address, query)) = mailto.split_once('?') {
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if key.eq_ignore_ascii_case("subject") {
                subject = Some(value.to_string());
            }
        }
        address.to_string()
    } else if let Ok(url) = Url::parse(&format!("mailto:{mailto}")) {
        for (key, value) in url.query_pairs() {
            if key.eq_ignore_ascii_case("subject") {
                subject = Some(value.to_string());
            }
        }
        url.path().to_string()
    } else {
        mailto.to_string()
    };

    UnsubscribeMethod::Mailto { address, subject }
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
