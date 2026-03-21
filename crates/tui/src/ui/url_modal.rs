use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct UrlEntry {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct UrlModalState {
    pub urls: Vec<UrlEntry>,
    pub selected: usize,
}

impl UrlModalState {
    pub fn new(urls: Vec<UrlEntry>) -> Self {
        Self { urls, selected: 0 }
    }

    pub fn next(&mut self) {
        if !self.urls.is_empty() {
            self.selected = (self.selected + 1).min(self.urls.len() - 1);
        }
    }

    pub fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_url(&self) -> Option<&str> {
        self.urls.get(self.selected).map(|e| e.url.as_str())
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: Option<&UrlModalState>, theme: &Theme) {
    let Some(state) = state else {
        return;
    };

    let popup = centered_rect(60, 55, area);
    frame.render_widget(Clear, popup);

    let title = format!(" Links ({}) ", state.urls.len());
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    let items: Vec<ListItem> = state
        .urls
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == state.selected;
            if entry.label == entry.url {
                let style = if is_selected {
                    Style::default()
                        .fg(theme.link_fg)
                        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(theme.link_fg)
                        .add_modifier(Modifier::UNDERLINED)
                };
                ListItem::new(Line::from(Span::styled(entry.url.clone(), style)))
            } else {
                let label_style = if is_selected {
                    Style::default().fg(theme.text_primary).bold()
                } else {
                    Style::default().fg(theme.text_secondary)
                };
                let url_style = if is_selected {
                    Style::default()
                        .fg(theme.link_fg)
                        .add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default().fg(theme.text_muted)
                };
                ListItem::new(vec![
                    Line::from(Span::styled(entry.label.clone(), label_style)),
                    Line::from(Span::styled(format!("  {}", entry.url), url_style)),
                ])
            }
        })
        .collect();

    let list = List::new(items).highlight_style(theme.highlight_style());
    let mut list_state = ListState::default().with_selected(Some(state.selected));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    let list_height = chunks[0].height as usize;
    if state.urls.len() > list_height {
        let mut scrollbar_state = ScrollbarState::new(state.urls.len().saturating_sub(list_height))
            .position(state.selected);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.accent)),
            chunks[0],
            &mut scrollbar_state,
        );
    }

    let footer = "Enter/o open  j/k move  y copy  Esc close";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme.text_secondary)),
        chunks[1],
    );
}

/// Extract URLs from message body text (plain text and/or HTML).
/// HTML anchor tags get label extraction; plain-text URLs are also captured.
/// Deduplicates by URL, preferring labeled entries from HTML anchors.
pub fn extract_urls(text_plain: Option<&str>, text_html: Option<&str>) -> Vec<UrlEntry> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();

    // Extract from HTML anchor tags first (they have labels)
    if let Some(html) = text_html {
        let mut rest = html;
        while let Some(href_start) = rest.find("href=\"") {
            let after_href = &rest[href_start + 6..];
            if let Some(href_end) = after_href.find('"') {
                let url = &after_href[..href_end];
                let after_tag = &after_href[href_end..];
                let label = if let Some(gt) = after_tag.find('>') {
                    let after_gt = &after_tag[gt + 1..];
                    if let Some(close) = after_gt.find("</a>") {
                        let label_text = after_gt[..close].trim();
                        let clean = strip_html_tags(label_text);
                        if clean.is_empty() {
                            url.to_string()
                        } else {
                            clean
                        }
                    } else {
                        url.to_string()
                    }
                } else {
                    url.to_string()
                };

                if url.starts_with("http") && seen.insert(url.to_string()) {
                    urls.push(UrlEntry {
                        label,
                        url: url.to_string(),
                    });
                }
            }
            rest = &rest[href_start + 6..];
        }
    }

    // Extract plain text URLs from both plain text and HTML (for URLs not in anchors)
    for text in [text_plain, text_html].into_iter().flatten() {
        extract_plain_urls(text, &mut urls, &mut seen);
    }

    urls
}

fn extract_plain_urls(text: &str, urls: &mut Vec<UrlEntry>, seen: &mut HashSet<String>) {
    let mut rest = text;
    while let Some(start) = next_url_start(rest) {
        let url_rest = &rest[start..];
        let end = url_rest
            .find(|c: char| {
                c.is_whitespace()
                    || c == '>'
                    || c == ')'
                    || c == ']'
                    || c == '"'
                    || c == '<'
                    || c == '\''
            })
            .unwrap_or(url_rest.len());
        let url = url_rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);

        if seen.insert(url.to_string()) {
            urls.push(UrlEntry {
                label: url.to_string(),
                url: url.to_string(),
            });
        }
        rest = &rest[start + end..];
    }
}

fn next_url_start(text: &str) -> Option<usize> {
    match (text.find("https://"), text.find("http://")) {
        (Some(https), Some(http)) => Some(https.min(http)),
        (Some(https), None) => Some(https),
        (None, Some(http)) => Some(http),
        (None, None) => None,
    }
}

fn strip_html_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result.trim().to_string()
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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

pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_text_urls() {
        let urls = extract_urls(
            Some("Check out https://example.com and http://test.org/page"),
            None,
        );
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].url, "https://example.com");
        assert_eq!(urls[1].url, "http://test.org/page");
    }

    #[test]
    fn extract_html_anchor_urls() {
        let html = r#"<a href="https://example.com">Example Site</a>"#;
        let urls = extract_urls(None, Some(html));
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://example.com");
        assert_eq!(urls[0].label, "Example Site");
    }

    #[test]
    fn deduplicates_urls() {
        let plain = "Visit https://example.com for more";
        let html = r#"<a href="https://example.com">Example</a>"#;
        let urls = extract_urls(Some(plain), Some(html));
        assert_eq!(urls.len(), 1);
        // HTML anchor version wins (has label)
        assert_eq!(urls[0].label, "Example");
    }

    #[test]
    fn strips_trailing_punctuation() {
        let urls = extract_urls(Some("See https://example.com."), None);
        assert_eq!(urls[0].url, "https://example.com");
    }

    #[test]
    fn modal_state_navigation() {
        let mut state = UrlModalState::new(vec![
            UrlEntry {
                label: "A".into(),
                url: "https://a.com".into(),
            },
            UrlEntry {
                label: "B".into(),
                url: "https://b.com".into(),
            },
            UrlEntry {
                label: "C".into(),
                url: "https://c.com".into(),
            },
        ]);
        assert_eq!(state.selected, 0);
        state.next();
        assert_eq!(state.selected, 1);
        state.next();
        assert_eq!(state.selected, 2);
        state.next();
        assert_eq!(state.selected, 2); // clamped
        state.prev();
        assert_eq!(state.selected, 1);
        state.prev();
        assert_eq!(state.selected, 0);
        state.prev();
        assert_eq!(state.selected, 0); // clamped
    }
}
