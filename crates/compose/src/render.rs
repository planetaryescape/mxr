pub use mxr_outbound::render::*;

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
