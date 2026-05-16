//! Conformance corpus runner.
//!
//! Loads every JSON fixture under `testdata/conformance/`, runs
//! `mail_query::parse_with` on the input, and asserts the output matches
//! `expected_ast` or `expected_error`.
//!
//! Three integrity checks (per lessons/04 + lessons/05):
//!
//! 1. Every fixture file appears in `testdata/coverage.md`.
//! 2. Every contract-critical fixture in `REQUIRED_FIXTURES` exists.
//! 3. Each fixture's actual parse output matches the expectation.

#![allow(clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use mail_query::{parse_with, ParseError, ParserOptions, QueryNode};
use serde::Deserialize;

const REQUIRED_FIXTURES: &[&str] = &[
    "text-bare-word",
    "exact-plus-word",
    "phrase-quoted",
    "phrase-escaped-inner-quote",
    "field-from",
    "filter-is-unread",
    "filter-custom-via-options",
    "filter-unknown-without-registration-errors",
    "label-bare",
    "date-specific-after",
    "date-relative-not-resolved-at-parse-time",
    "size-greater-than-megabytes",
    "boolean-precedence-implicit-and-binds-tighter-than-or",
    "parens-override-precedence",
    "negation-minus-and-not-keyword-equivalent",
];

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    spec: serde_json::Value,
    #[serde(default)]
    options: FixtureOptions,
    input: String,
    #[serde(default)]
    expected_ast: Option<QueryNode>,
    #[serde(default)]
    expected_error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureOptions {
    #[serde(default)]
    custom_filters: Vec<String>,
}

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/conformance")
}

fn coverage_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/coverage.md")
}

fn fixture_names() -> Vec<String> {
    let mut names = Vec::new();
    let entries = std::fs::read_dir(conformance_dir()).unwrap();
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        names.push(path.file_stem().unwrap().to_str().unwrap().to_string());
    }
    names.sort();
    names
}

#[test]
fn coverage_matrix_mentions_every_fixture() {
    let matrix = std::fs::read_to_string(coverage_path()).unwrap();
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
fn required_fixtures_exist() {
    let mut missing = Vec::new();
    for name in REQUIRED_FIXTURES {
        if !conformance_dir().join(format!("{name}.json")).exists() {
            missing.push(*name);
        }
    }
    assert!(
        missing.is_empty(),
        "required fixtures missing: {missing:?}"
    );
}

#[test]
fn conformance_fixtures_match_expected_outputs() {
    let mut failures = Vec::<String>::new();
    for name in fixture_names() {
        let path = conformance_dir().join(format!("{name}.json"));
        let raw = std::fs::read_to_string(&path).unwrap();
        let fixture: Fixture = serde_json::from_str(&raw)
            .unwrap_or_else(|err| panic!("parse fixture {name}: {err}"));

        let mut options = ParserOptions::new();
        options.register_custom_filters(&fixture.options.custom_filters);

        let actual = parse_with(&fixture.input, &options);

        match (&actual, &fixture.expected_ast, &fixture.expected_error) {
            (Ok(node), Some(expected), None) => {
                if node != expected {
                    failures.push(format!(
                        "{name}: AST mismatch\n  input:    {}\n  expected: {expected:?}\n  actual:   {node:?}",
                        fixture.input
                    ));
                }
            }
            (Err(err), None, Some(expected_variant)) => {
                let actual_variant = parse_error_variant(err);
                if actual_variant != expected_variant {
                    failures.push(format!(
                        "{name}: error variant mismatch\n  input:    {}\n  expected: {expected_variant}\n  actual:   {actual_variant}",
                        fixture.input
                    ));
                }
            }
            (Ok(node), None, Some(expected_variant)) => {
                failures.push(format!(
                    "{name}: expected error {expected_variant}, got AST {node:?}"
                ));
            }
            (Err(err), Some(expected), None) => {
                failures.push(format!(
                    "{name}: expected AST {expected:?}, got error {err:?}"
                ));
            }
            (_, None, None) => {
                failures.push(format!(
                    "{name}: fixture has neither expected_ast nor expected_error"
                ));
            }
            (_, Some(_), Some(_)) => {
                failures.push(format!(
                    "{name}: fixture has BOTH expected_ast AND expected_error"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} conformance fixture(s) failed:\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}

fn parse_error_variant(err: &ParseError) -> &'static str {
    match err {
        ParseError::UnexpectedEnd => "UnexpectedEnd",
        ParseError::UnexpectedToken(_) => "UnexpectedToken",
        ParseError::UnmatchedParen => "UnmatchedParen",
        ParseError::UnmatchedBrace => "UnmatchedBrace",
        ParseError::ExpectedValue => "ExpectedValue",
        ParseError::UnknownFilter(_) => "UnknownFilter",
        ParseError::InvalidSize(_) => "InvalidSize",
        ParseError::InvalidDate(_) => "InvalidDate",
        _ => "Unknown",
    }
}
