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
    // Should not contain tracking junk
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
fn plain_email_with_signature() {
    let text = "Hey,\n\nCan we meet tomorrow at 3pm?\n\nThanks,\n-- \nAlice\nSenior Engineer\n+1 555-0123\nalice@company.com";
    let output = clean(Some(text), None, &ReaderConfig::default());
    assert_eq!(
        output.content.trim(),
        "Hey,\n\nCan we meet tomorrow at 3pm?\n\nThanks,"
    );
    assert_eq!(
        output.signature.as_deref(),
        Some("Alice\nSenior Engineer\n+1 555-0123\nalice@company.com")
    );
}

#[test]
fn reader_mode_stats_correct() {
    let text = "Content here.\n\nOn Mon, alice wrote:\n> Long quote\n> Another line\n> And more\n\n-- \nSig line\nPhone: 555-0123";
    let output = clean(Some(text), None, &ReaderConfig::default());
    assert!(output.original_lines > output.cleaned_lines);
}

#[test]
fn email_with_quotes_and_signature() {
    let text = "I agree with your plan.\n\nOn Tue, Mar 10, bob@example.com wrote:\n> We should deploy on Friday.\n> The staging environment is ready.\n\n-- \nAlice Smith\nalice@example.com\n+1 555-0100";
    let output = clean(Some(text), None, &ReaderConfig::default());
    assert!(output.content.contains("I agree"));
    assert!(output
        .content
        .contains("[previous message from bob@example.com]"));
    assert!(!output.content.contains("We should deploy"));
    assert_eq!(
        output.signature.as_deref(),
        Some("Alice Smith\nalice@example.com\n+1 555-0100")
    );
    assert_eq!(output.quoted_messages.len(), 1);
}

#[test]
fn corporate_email_with_boilerplate() {
    let text = "Please see attached report.\n\nRegards,\nJohn\n\nThis email is confidential and intended solely for the recipient.\nIf you have received this message in error, please delete it.";
    let output = clean(Some(text), None, &ReaderConfig::default());
    assert!(output.content.contains("attached report"));
    assert!(!output.content.to_lowercase().contains("confidential"));
}
