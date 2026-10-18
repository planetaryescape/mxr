//! Local, provider-neutral observations, not a sender identity or reply obligation.

/// Cached evidence shared by every automatic mail view. Missing content is unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailEvidence {
    pub list_id: Option<String>,
    pub unsubscribe: bool,
    pub auto_generated: bool,
    pub bulk: bool,
    pub is_reply: bool,
    /// Only the authored plain text is used for conservative attention detection.
    pub text: Option<String>,
}

/// Unfold cached RFC headers. Header names are case insensitive; body text is never read.
pub fn header_values(raw: &str) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalized.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = fields.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            fields.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    fields
}

/// List-Id is an identifier inside angle brackets, not an email address.
pub fn normalize_list_id(value: &str) -> Option<String> {
    let value = value.trim();
    let id = match value.split_once('<') {
        Some((_, rest)) => rest.split_once('>')?.0,
        None => value,
    };
    (!id.is_empty()
        && !id.chars().any(char::is_whitespace)
        && !id.contains(['<', '>'])
        && id.contains('.'))
    .then(|| id.to_ascii_lowercase())
}

/// Positive syntax evidence of an RFC message-id, not just a nonempty field.
pub fn has_reply_reference(value: &str) -> bool {
    value.split('<').skip(1).any(|part| {
        part.split_once('>').is_some_and(|(id, _)| {
            id.split_once('@')
                .is_some_and(|(left, right)| !left.is_empty() && !right.is_empty())
                && !id.chars().any(char::is_whitespace)
                && !id.contains(['<', '>'])
        })
    })
}

impl MailEvidence {
    pub fn cached(metadata: &crate::MessageMetadata, text: Option<String>) -> Self {
        let mut evidence = Self {
            list_id: metadata.list_id.as_deref().and_then(normalize_list_id),
            text,
            ..Self::default()
        };
        for (name, value) in header_values(metadata.raw_headers.as_deref().unwrap_or_default()) {
            match name.as_str() {
                "list-id" => evidence.list_id = normalize_list_id(&value).or(evidence.list_id),
                "list-unsubscribe" => {
                    evidence.unsubscribe |= value.split('<').skip(1).any(|part| {
                        part.split_once('>').is_some_and(|(uri, _)| {
                            let uri = uri.trim().to_ascii_lowercase();
                            ["https://", "http://", "mailto:"].iter().any(|scheme| {
                                uri.strip_prefix(*scheme).is_some_and(|rest| {
                                    !rest.is_empty() && !rest.chars().any(char::is_whitespace)
                                })
                            })
                        })
                    })
                }
                "auto-submitted" => {
                    let keyword = value.split([';', ' ', '\t']).next().unwrap_or_default();
                    evidence.auto_generated |= matches!(
                        keyword.to_ascii_lowercase().as_str(),
                        "auto-generated" | "auto-replied"
                    );
                }
                "precedence" => {
                    evidence.bulk |= matches!(
                        value.to_ascii_lowercase().as_str(),
                        "bulk" | "list" | "junk"
                    )
                }
                "in-reply-to" | "references" => evidence.is_reply |= has_reply_reference(&value),
                _ => {}
            }
        }
        evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_headers_are_folded_case_insensitive_and_bounded_by_header_end() {
        let metadata = crate::MessageMetadata {
            raw_headers: Some("LiSt-Id: Example\r\n\t<Weekly.Example.org>\r\nAuto-Submitted: auto-generated\r\nPrecedence: bulk\r\n\r\nList-Unsubscribe: <https://ignored.example>".into()),
            ..Default::default()
        };
        let evidence = MailEvidence::cached(&metadata, None);
        assert_eq!(evidence.list_id.as_deref(), Some("weekly.example.org"));
        assert!(evidence.auto_generated && evidence.bulk);
        assert!(!evidence.unsubscribe);
        assert_eq!(normalize_list_id("Broken <"), None);
    }

    #[test]
    fn malformed_or_negative_automation_fields_do_not_prove_automation() {
        for value in [
            "",
            "no",
            "NO; owner=sender",
            "unknown-value",
            "<auto-generated>",
        ] {
            let metadata = crate::MessageMetadata {
                raw_headers: Some(format!("Auto-Submitted: {value}\r")),
                ..Default::default()
            };
            assert!(
                !MailEvidence::cached(&metadata, None).auto_generated,
                "{value}"
            );
        }
        let metadata = crate::MessageMetadata {
            raw_headers: Some("Auto-Submitted: auto-replied\rPrecedence: bulk\r".into()),
            ..Default::default()
        };
        let evidence = MailEvidence::cached(&metadata, None);
        assert!(evidence.auto_generated && evidence.bulk);
        let malformed = crate::MessageMetadata {
            raw_headers: Some(
                "List-Unsubscribe: <>\r\nIn-Reply-To: nonsense\r\nList-Id: Broken <\r\n".into(),
            ),
            ..Default::default()
        };
        let evidence = MailEvidence::cached(&malformed, None);
        assert!(!evidence.unsubscribe && !evidence.is_reply && evidence.list_id.is_none());
    }
}
