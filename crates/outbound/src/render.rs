use comrak::{markdown_to_html, Options};

const HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 14px; line-height: 1.5; color: #333; max-width: 600px;">
{content}
</body>
</html>"#;

pub struct RenderedMessage {
    pub plain: String,
    pub html: String,
}

pub fn render_markdown(markdown: &str) -> RenderedMessage {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.unsafe_ = false;

    let html_content = markdown_to_html(markdown, &options);
    let html = HTML_TEMPLATE.replace("{content}", &html_content);

    RenderedMessage {
        plain: markdown.to_string(),
        html,
    }
}
