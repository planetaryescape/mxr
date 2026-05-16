//! Conformance corpus runner.
//!
//! Loads every JSON fixture under `testdata/conformance/`, runs
//! `list_unsubscribe::parse_with_post` on the inputs, and asserts the
//! output matches `expected`. Also enforces:
//!
//! - Every fixture file appears in `testdata/coverage.md`.
//! - Every contract-critical fixture in `REQUIRED_FIXTURES` exists.

#![allow(clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use list_unsubscribe::{parse_with_post, UnsubscribeMethod};

const REQUIRED_FIXTURES: &[&str] = &[
    "rfc2369-mailto-only",
    "rfc2369-https-only",
    "rfc2369-both-prefer-mailto",
    "rfc8058-one-click-basic",
    "rfc8058-one-click-case-insensitive",
    "rfc8058-post-without-https-falls-back",
    "mailto-with-subject",
    "mailto-with-subject-and-body-drops-body",
    "multiple-https-returns-first",
    "malformed-url-returns-none",
    "empty-header",
    "angle-bracket-whitespace-quirks",
    "http-scheme-case-insensitive",
    "http-fallback-when-no-mailto",
];

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/conformance")
}

fn coverage_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/coverage.md")
}

fn fixture_names() -> Vec<String> {
    let mut names = Vec::new();
    let dir = conformance_dir();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("utf-8 stem")
            .to_string();
        names.push(stem);
    }
    names.sort();
    names
}

#[test]
fn coverage_matrix_mentions_every_fixture() {
    let matrix = std::fs::read_to_string(coverage_path()).expect("read coverage.md");
    let mut missing = Vec::new();
    for fixture in fixture_names() {
        if !matrix.contains(&format!("`{fixture}`")) {
            missing.push(fixture);
        }
    }
    assert!(
        missing.is_empty(),
        "coverage.md does not mention these fixtures: {missing:?}"
    );
}

#[test]
fn conformance_corpus_contains_required_behavior_fixtures() {
    let mut missing = Vec::new();
    for name in REQUIRED_FIXTURES {
        let path = conformance_dir().join(format!("{name}.json"));
        if !path.exists() {
            missing.push(*name);
        }
    }
    assert!(missing.is_empty(), "required fixtures missing: {missing:?}");
}

#[test]
fn conformance_fixtures_match_expected_outputs() {
    let dir = conformance_dir();
    let mut failures = Vec::<String>::new();
    for name in fixture_names() {
        let path = dir.join(format!("{name}.json"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let fixture: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));

        let header = fixture["input"]["list_unsubscribe"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: input.list_unsubscribe must be string"));
        let post = fixture["input"]["list_unsubscribe_post"].as_str();

        let actual = parse_with_post(header, post);
        if let Err(msg) = compare(&actual, &fixture["expected"]) {
            failures.push(format!("{name}: {msg}"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} conformance fixture(s) failed:\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}

fn compare(actual: &UnsubscribeMethod, expected: &serde_json::Value) -> Result<(), String> {
    let expected_kind = expected["kind"]
        .as_str()
        .ok_or_else(|| "expected.kind must be string".to_string())?;
    match (actual, expected_kind) {
        (UnsubscribeMethod::OneClick { url }, "OneClick")
        | (UnsubscribeMethod::HttpLink { url }, "HttpLink") => {
            let expected_url = expected["url"]
                .as_str()
                .ok_or_else(|| "expected.url missing for HTTP variant".to_string())?;
            if url.as_str() != expected_url {
                return Err(format!("url: got {url}, want {expected_url}"));
            }
            Ok(())
        }
        (UnsubscribeMethod::Mailto { address, subject }, "Mailto") => {
            let expected_address = expected["address"]
                .as_str()
                .ok_or_else(|| "expected.address missing for Mailto".to_string())?;
            if address != expected_address {
                return Err(format!("address: got {address}, want {expected_address}"));
            }
            let expected_subject = expected["subject"].as_str();
            if subject.as_deref() != expected_subject {
                return Err(format!(
                    "subject: got {subject:?}, want {expected_subject:?}"
                ));
            }
            Ok(())
        }
        (UnsubscribeMethod::None, "None") => Ok(()),
        (got, want) => Err(format!("variant: got {got:?}, want {want}")),
    }
}
