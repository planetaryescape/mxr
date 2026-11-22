use mxr_reader::{clean, ReaderConfig};

#[test]
fn newsletter_stripped_to_content() {
    let html = include_str!("fixtures/newsletter.html");
    let output = clean(None, Some(html), &ReaderConfig::default());
    assert!(output.cleaned_lines < output.original_lines);
    assert!(output.content.contains("Weekly Tech Roundup"));
    assert!(output
        .content
        .contains("Rust 2026 edition brings exciting new features"));
    assert!(!output.content.contains("<html"));
    assert!(!output.content.contains("<p"));
    assert!(!output.content.contains("</"));
    assert!(!output
        .content
        .to_lowercase()
        .contains("view this email in your browser"));
    assert!(!output
        .content
        .to_lowercase()
        .contains("click here to unsubscribe"));
}

#[test]
fn html_only_reader_never_passes_raw_markup_through() {
    let html = r#"<!doctype html>
<html>
  <body>
    <table role="presentation"><tr><td>
      <h1>Your order is confirmed</h1>
      <p>Thanks for shopping with us.</p>
      <a href="https://example.test/order">View order</a>
    </td></tr></table>
  </body>
</html>"#;
    let config = ReaderConfig {
        html_command: Some("cat".into()),
        ..Default::default()
    };

    let output = clean(None, Some(html), &config);

    assert!(output.content.contains("Your order is confirmed"));
    assert!(output.content.contains("Thanks for shopping with us."));
    assert!(!output.content.contains("<html"));
    assert!(!output.content.contains("<table"));
    assert!(!output.content.contains("</"));
}
