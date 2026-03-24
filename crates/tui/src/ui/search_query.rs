use crate::mxr_tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::style::Modifier;

const SEARCH_FIELDS: &[&str] = &[
    "from", "to", "cc", "bcc", "subject", "body", "filename", "label", "is", "after", "before",
    "on", "size", "newer", "older",
];

pub fn highlight_search_query(query: &str, theme: &Theme) -> Line<'static> {
    if query.is_empty() {
        return Line::default();
    }

    let mut spans = Vec::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            spans.push(Span::raw(chars[start..i].iter().collect::<String>()));
            continue;
        }

        if ch == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                let current = chars[i];
                i += 1;
                if current == '"' {
                    break;
                }
            }
            spans.push(Span::styled(
                chars[start..i].iter().collect::<String>(),
                Style::default().fg(theme.success),
            ));
            continue;
        }

        if matches!(ch, '(' | ')') {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(theme.accent_dim)
                    .add_modifier(Modifier::BOLD),
            ));
            i += 1;
            continue;
        }

        if ch == '-' {
            spans.push(Span::styled(
                "-",
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ));
            i += 1;
            continue;
        }

        let start = i;
        while i < chars.len()
            && !chars[i].is_whitespace()
            && chars[i] != '"'
            && chars[i] != '('
            && chars[i] != ')'
        {
            i += 1;
        }
        let token = chars[start..i].iter().collect::<String>();
        spans.extend(highlight_token(&token, theme));
    }

    Line::from(spans)
}

fn highlight_token(token: &str, theme: &Theme) -> Vec<Span<'static>> {
    if token.eq_ignore_ascii_case("and")
        || token.eq_ignore_ascii_case("or")
        || token.eq_ignore_ascii_case("not")
    {
        return vec![Span::styled(
            token.to_string(),
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        )];
    }

    if let Some((field, value)) = token.split_once(':') {
        if SEARCH_FIELDS.contains(&field.to_ascii_lowercase().as_str()) {
            let mut spans = vec![Span::styled(
                format!("{field}:"),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )];
            if !value.is_empty() {
                spans.push(Span::styled(
                    value.to_string(),
                    Style::default().fg(theme.link_fg),
                ));
            }
            return spans;
        }
    }

    vec![Span::styled(
        token.to_string(),
        Style::default().fg(theme.text_primary),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn highlights_fields_and_boolean_operators() {
        let theme = Theme::default();
        let line = highlight_search_query(
            r#"from:alice@example.com AND "release notes" -label:spam"#,
            &theme,
        );
        let spans = line.spans;

        assert_eq!(spans[0].content.as_ref(), "from:");
        assert_eq!(spans[0].style.fg, Some(theme.accent));

        assert_eq!(spans[1].content.as_ref(), "alice@example.com");
        assert_eq!(spans[1].style.fg, Some(theme.link_fg));

        assert!(spans
            .iter()
            .any(|span| span.content.as_ref() == "AND" && span.style.fg == Some(theme.warning)));
        assert!(spans
            .iter()
            .any(|span| span.content.as_ref() == "\"release notes\""
                && span.style.fg == Some(theme.success)));
        assert!(spans
            .iter()
            .any(|span| span.content.as_ref() == "-" && span.style.fg == Some(theme.warning)));
        assert!(spans
            .iter()
            .any(|span| span.content.as_ref() == "label:" && span.style.fg == Some(theme.accent)));
        assert!(!spans.iter().any(|span| span.style.fg == Some(Color::Reset)));
    }
}
