use comrak::{markdown_to_html, Options};

/// Minimal HTML template wrapping rendered markdown.
const HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 14px; line-height: 1.5; color: #333; max-width: 600px;">
{content}
</body>
</html>"#;

/// Rendered email parts from markdown source.
pub struct RenderedMessage {
    /// Raw markdown as text/plain part.
    pub plain: String,
    /// Rendered HTML as text/html part.
    pub html: String,
}

/// Convert markdown body to multipart-ready text/plain + text/html.
pub fn render_markdown(markdown: &str) -> RenderedMessage {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.r#unsafe = false;

    let html_content = markdown_to_html(markdown, &options);
    let html = HTML_TEMPLATE.replace("{content}", &html_content);

    RenderedMessage {
        plain: markdown.to_string(),
        html,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_basic_markdown() {
        let md = "Hello **world**!";
        let result = render_markdown(md);
        assert_eq!(result.plain, md);
        assert!(result.html.contains("<strong>world</strong>"));
        assert!(result.html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn render_with_links() {
        let md = "Check out https://example.com for more.";
        let result = render_markdown(md);
        assert!(result.html.contains("href"));
    }

    #[test]
    fn render_empty() {
        let result = render_markdown("");
        assert!(result.plain.is_empty());
        assert!(result.html.contains("<!DOCTYPE html>"));
    }
}
