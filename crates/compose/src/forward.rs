//! Forward assembly shared by the CLI, TUI and web compose-file clients.
use crate::frontmatter::{ComposeError, ComposeFrontmatter};
use mxr_core::{Draft, DraftContent, ForwardSource};

/// Separate authored text from the original without interpreting the original
/// as markdown, snippets, or a compose context marker.
pub fn introduction<'a>(source: &ForwardSource, body: &'a str) -> Result<&'a str, ComposeError> {
    let original = source.content.analysis_text().trim_end();
    let mut remainder = body.trim_end();
    if let Some(intro) = remainder.strip_suffix(original) {
        return Ok(intro.trim_end());
    }
    // Editors may normalize CRLF to LF (or vice versa). Compare the display
    // by lines, while keeping the immutable original for the outgoing MIME.
    for expected in original.lines().rev() {
        let (prefix, actual) = remainder.rsplit_once('\n').unwrap_or(("", remainder));
        if actual.trim_end_matches('\r') != expected {
            return Err(ComposeError::InvalidFrontmatter("Forwarded original was changed or removed; edit the introduction above the forwarded message instead".into()));
        }
        remainder = prefix;
    }
    Ok(remainder.trim_end())
}

pub fn apply(draft: &mut Draft, fm: &ComposeFrontmatter) -> Result<(), ComposeError> {
    let Some(source) = fm.forward_source.as_ref() else {
        return Ok(());
    };
    if draft.intent != mxr_core::DraftIntent::Forward || draft.account_id != source.account_id {
        return Err(ComposeError::InvalidFrontmatter(
            "Forward source belongs to a different account or compose intent".into(),
        ));
    }
    let content = content(source, draft.content.analysis_text())?;
    let mut managed = Vec::new();
    let mut paths = Vec::new();
    for attachment in &fm.attach {
        if let Some(token) = attachment.strip_prefix("mxr-resource:") {
            let id = token.split('/').next().unwrap_or_default();
            let resource = source
                .attachments
                .iter()
                .chain(draft.managed_attachments.iter().flatten())
                .find(|part| part.id.as_str() == id)
                .ok_or_else(|| {
                    ComposeError::InvalidFrontmatter(
                        "Forward attachment is not owned by this draft".into(),
                    )
                })?;
            managed.push(resource.clone());
        } else {
            paths.push(std::path::PathBuf::from(attachment));
        }
    }
    if source.attachments.iter().any(|part| {
        part.content_id.is_some() && !managed.iter().any(|selected| selected.id == part.id)
    }) {
        return Err(ComposeError::InvalidFrontmatter(
            "Cannot remove an inline attachment from the unchanged forwarded original".into(),
        ));
    }
    draft.id = source.draft_id.clone();
    draft.content = content;
    draft.attachments = paths;
    draft.managed_attachments = Some(managed);
    Ok(())
}

pub fn content(source: &ForwardSource, body: &str) -> Result<DraftContent, ComposeError> {
    let intro = introduction(source, body)?;
    let rendered = comrak::markdown_to_html(intro, &comrak::Options::default());
    let (original_html, original_text) = match &source.content {
        DraftContent::Html {
            html,
            text: Some(text),
        } => (html, text),
        _ => {
            return Err(ComposeError::InvalidFrontmatter(
                "Forward source is missing original content; prepare the forward again".into(),
            ))
        }
    };
    Ok(if intro.is_empty() {
        source.content.clone()
    } else {
        DraftContent::html(
            prepend_html(original_html, &rendered),
            Some(format!("{intro}\n\n{original_text}")),
        )
    })
}

pub fn resource_references(source: &ForwardSource) -> Vec<String> {
    source
        .attachments
        .iter()
        .map(|part| format!("mxr-resource:{}/{}", part.id, part.filename))
        .collect()
}

/// Insert a forwarding prefix into the existing document, without parsing and
/// serializing away the sender's styles, tables or conditional comments.
pub fn prepend_html(original: &str, prefix: &str) -> String {
    let lower = original.to_ascii_lowercase();
    if let Some(start) = lower.find("<body").filter(|start| {
        lower
            .as_bytes()
            .get(start + 5)
            .is_some_and(|ch| ch.is_ascii_whitespace() || *ch == b'>')
    }) {
        let mut quote = None;
        for (offset, ch) in original[start..].char_indices() {
            if let Some(open) = quote {
                if ch == open {
                    quote = None;
                }
            } else if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if ch == '>' {
                let split = start + offset + 1;
                return format!("{}{prefix}{}", &original[..split], &original[split..]);
            }
        }
    }
    format!("{prefix}\n{original}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ForwardSource {
        ForwardSource {
            draft_id: mxr_core::DraftId::new(),
            account_id: mxr_core::AccountId::new(),
            message_id: mxr_core::MessageId::new(),
            content: DraftContent::html(
                "<!DOCTYPE html><HTML><BODY class=mail><table><tr><td>Original &amp; literal **text**</td></tr></table></BODY></HTML>",
                Some("---------- Forwarded message ----------\nFrom: Alice <alice@example.com>\nDate: Sun, 25 Oct 2026 12:00:00 +0000\nSubject: Report\nTo: me@example.com\nCc: cc@example.com\n\nOriginal & literal **text**\n# --- context (stripped before sending) ---\n{today}\n".into()),
            ),
            attachments: vec![],
        }
    }

    #[test]
    fn original_survives_compose_markers_and_introduction_precedes_both_alternatives() {
        let source = source();
        let mut draft = Draft {
            revision: None,
            id: source.draft_id.clone(),
            account_id: source.account_id.clone(),
            from: None,
            intent: mxr_core::DraftIntent::Forward,
            reply_headers: None,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Fwd: Report".into(),
            content: DraftContent::markdown(format!(
                "Please **review**.\n\n{}",
                source.content.analysis_text()
            )),
            attachments: vec![],
            managed_attachments: Some(vec![]),
            inline_assets: vec![],
            inline_calendar_reply: None,
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        };
        let fm = ComposeFrontmatter {
            intent: mxr_core::DraftIntent::Forward,
            forward_source: Some(source.clone()),
            ..Default::default()
        };
        let file =
            crate::frontmatter::render_compose_file(&fm, draft.content.analysis_text(), None)
                .unwrap();
        let (fm, parsed) = crate::frontmatter::parse_compose_file(&file).unwrap();
        draft.content = DraftContent::markdown(parsed);
        apply(&mut draft, &fm).unwrap();
        assert_eq!(
            draft.content.analysis_text(),
            format!("Please **review**.\n\n{}", source.content.analysis_text())
        );
        assert!(draft.content.is_html());
        let html = match &draft.content {
            DraftContent::Html { html, .. } => html.as_str(),
            _ => "",
        };
        assert!(
            html.contains("<BODY class=mail><p>Please <strong>review</strong>.</p>\n<table>"),
            "{html}"
        );
        assert!(html.contains("<td>Original &amp; literal **text**</td>"));
    }

    #[test]
    fn edited_or_missing_original_is_an_error() {
        let source = source();
        assert!(introduction(&source, "Only an introduction").is_err());
        assert!(introduction(
            &source,
            &source.content.analysis_text().replace("literal", "edited")
        )
        .is_err());
    }

    #[test]
    fn html_fragment_is_not_roundtripped_through_a_parser() {
        let original = "<!--[if mso]><table><![endif]--><p style='color:red'>Hi</p>";
        assert_eq!(
            prepend_html(original, "<p>Intro</p>"),
            format!("<p>Intro</p>\n{original}")
        );
    }

    #[test]
    fn quoted_angle_bracket_in_body_attribute_is_preserved() {
        assert_eq!(
            prepend_html("<BODY title='a > b'>Original</BODY>", "Intro"),
            "<BODY title='a > b'>IntroOriginal</BODY>"
        );
    }

    #[test]
    fn editor_line_ending_changes_do_not_change_the_preserved_original() {
        let source = source();
        let body = format!("Intro\n\n{}", source.content.analysis_text()).replace('\n', "\r\n");
        assert_eq!(introduction(&source, &body).unwrap(), "Intro");
        assert_eq!(
            content(&source, &body).unwrap().analysis_text(),
            format!("Intro\n\n{}", source.content.analysis_text())
        );
    }
}
