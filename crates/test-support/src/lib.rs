use ratatui::backend::TestBackend;
use ratatui::Terminal;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const STANDARDS_FIXTURES: &[&str] = &[
    "alternative-html-first.eml",
    "duplicate-singletons.eml",
    "encoded-words.eml",
    "folded-flowed.eml",
    "malformed-minimal.eml",
    "missing-content-type.eml",
    "missing-message-id.eml",
    "multipart-calendar.eml",
    "nested-multipart.eml",
    "quoted-local-group.eml",
    "rfc2231-attachment.eml",
    "unsubscribe-oneclick.eml",
];

pub fn standards_fixture_names() -> &'static [&'static str] {
    STANDARDS_FIXTURES
}

pub fn fixture_stem(name: &str) -> &str {
    name.strip_suffix(".eml").unwrap_or(name)
}

pub fn standards_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("standards")
        .join(name)
}

pub fn standards_fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(standards_fixture_path(name))
        .expect("test fixture bytes should exist and be readable")
}

pub fn standards_fixture_string(name: &str) -> String {
    std::fs::read_to_string(standards_fixture_path(name))
        .expect("test fixture string should exist and be readable")
}

pub fn redact_rfc822(raw: &str) -> String {
    static MESSAGE_ID_RE: OnceLock<Regex> = OnceLock::new();
    static DATE_RE: OnceLock<Regex> = OnceLock::new();
    let message_id_re = MESSAGE_ID_RE.get_or_init(|| {
        Regex::new(r"(?m)^Message-ID:\s*<[^>\r\n]+>\r?$")
            .expect("message-id redaction regex should compile")
    });
    let date_re = DATE_RE.get_or_init(|| {
        Regex::new(r"(?m)^Date:\s*[^\r\n]+\r?$").expect("date redaction regex should compile")
    });

    let mut redacted = message_id_re
        .replace_all(raw, "Message-ID: <redacted@example.com>")
        .to_string();
    redacted = date_re
        .replace_all(&redacted, "Date: Fri, 20 Mar 2026 00:00:00 +0000")
        .to_string();

    let boundary_re =
        Regex::new(r#"boundary="([^"]+)""#).expect("boundary redaction regex should compile");
    let boundaries = boundary_re
        .captures_iter(raw)
        .enumerate()
        .map(|(index, caps)| {
            (
                caps[1].to_string(),
                format!("boundary=\"BOUNDARY_{index}\""),
            )
        })
        .collect::<Vec<_>>();

    for (index, (boundary, replacement)) in boundaries.iter().enumerate() {
        redacted = redacted.replace(&format!("boundary=\"{boundary}\""), replacement);
        redacted = redacted.replace(boundary, &format!("BOUNDARY_{index}"));
    }

    redacted
}

pub fn render_to_string<F>(width: u16, height: u16, draw: F) -> String
where
    F: FnOnce(&mut ratatui::Frame<'_>),
{
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal.draw(draw).expect("test draw should succeed");
    format!("{}", terminal.backend())
}
