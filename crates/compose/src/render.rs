pub use mxr_outbound::render::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_with_links() {
        let md = "Check out https://example.com for more.";
        let result = render_markdown(md);
        assert!(result.html.contains("href"));
    }
}
