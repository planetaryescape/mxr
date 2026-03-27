use mail_parser::MessageParser;
use mxr_reader::{clean, ReaderConfig};
use mxr_test_support::{fixture_stem, standards_fixture_bytes, standards_fixture_names};
use serde_json::json;

#[test]
fn standards_fixture_reader_snapshot() {
    let raw = standards_fixture_bytes("multipart-calendar.eml");
    let message = MessageParser::default().parse(&raw).unwrap();
    let text = message.body_text(0);
    let html = message.body_html(0);
    let output = clean(text.as_deref(), html.as_deref(), &ReaderConfig::default());
    assert!(output.cleaned_lines <= output.original_lines);
    assert!(output.content.trim().len() > 0);
    assert_eq!(output.quoted_messages.len(), 0);

    insta::assert_yaml_snapshot!(
        "reader_fixture_output",
        json!({
            "content": output.content,
            "quoted_blocks": output.quoted_messages.len(),
            "signature": output.signature,
            "original_lines": output.original_lines,
            "cleaned_lines": output.cleaned_lines,
        })
    );
}

#[test]
fn standards_fixture_reader_matrix_snapshots() {
    for fixture in standards_fixture_names() {
        let raw = standards_fixture_bytes(fixture);
        let message = MessageParser::default().parse(&raw).unwrap();
        let text = message.body_text(0);
        let html = message.body_html(0);
        let output = clean(text.as_deref(), html.as_deref(), &ReaderConfig::default());
        assert!(output.cleaned_lines <= output.original_lines, "fixture={fixture}");
        assert!(output.content.trim().len() > 0, "fixture={fixture}");

        insta::assert_yaml_snapshot!(
            format!("reader_fixture__{}", fixture_stem(fixture)),
            json!({
                "content_preview": output.content.lines().take(6).collect::<Vec<_>>(),
                "quoted_blocks": output.quoted_messages.len(),
                "signature": output.signature,
                "original_lines": output.original_lines,
                "cleaned_lines": output.cleaned_lines,
            })
        );
    }
}
