#![expect(
    clippy::panic,
    reason = "test support panics with captured command output to make fixture failures actionable"
)]

pub mod daemon;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::{Path, PathBuf};

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

pub fn render_to_string<F>(width: u16, height: u16, draw: F) -> String
where
    F: FnOnce(&mut ratatui::Frame<'_>),
{
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal.draw(draw).expect("test draw should succeed");
    format!("{}", terminal.backend())
}
