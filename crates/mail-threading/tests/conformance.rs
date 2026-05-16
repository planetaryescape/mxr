use chrono::{DateTime, Utc};
use mail_threading::{thread_messages_with, Message, Thread, ThreadingOptions};
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::path::Path;

const REQUIRED_FIXTURES: &[&str] = &[
    "basic-references-chain",
    "canonical-root-preserved-with-earlier-child",
    "conflicting-references-beat-in-reply-to",
    "current-message-reparents-to-last-reference",
    "cycle-in-references",
    "duplicate-message-id-first-wins",
    "empty-input",
    "in-reply-to-only-parent-missing",
    "in-reply-to-only-parent-present",
    "invalid-references-fall-back-to-in-reply-to",
    "invalid-threading-headers-allow-subject-fallback",
    "localized-subject-prefixes",
    "message-id-case-sensitive",
    "message-id-quoted-local-normalization",
    "missing-top-reference",
    "missing-message-id-assigned-unique-id",
    "missing-message-id-reply-to-present-parent",
    "multi-level-missing-phantom-chain",
    "no-replies",
    "prune-phantoms-disabled",
    "references-chain-preserves-existing-parent",
    "reply-arrives-before-parent",
    "same-subject-header-threads-not-merged",
    "self-reference",
    "single-message",
    "stable-thread-ordering-by-date",
    "stable-thread-ordering-with-caller-ids",
    "subject-fallback-attaches-to-header-thread",
    "subject-fallback-groups-headerless",
    "subject-blob-normalization",
    "subject-blob-only-preserved",
    "subject-forward-wrapper-normalization",
    "subject-prefixes-custom",
    "subject-merge-disabled",
    "subject-trailer-fwd-normalization",
    "subject-whitespace-normalization",
    "two-independent-threads",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    name: String,
    description: String,
    spec: Spec,
    options: Option<FixtureOptions>,
    input: Vec<FixtureMessage>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    source: String,
    url: String,
    behavior: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureOptions {
    subject_merge: Option<bool>,
    prune_phantoms: Option<bool>,
    subject_prefixes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureMessage {
    id: String,
    message_id: Option<String>,
    in_reply_to: Option<String>,
    #[serde(default)]
    references: Vec<String>,
    subject: String,
    date: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    threads: Vec<ExpectedThread>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedThread {
    root: String,
    messages: Vec<String>,
}

#[test]
fn conformance_fixtures_match_expected_threads() -> Result<(), Box<dyn Error>> {
    let entries = fixture_entries()?;

    assert!(!entries.is_empty(), "conformance corpus is empty");

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let bytes = fs::read(&path)?;
        let fixture: Fixture = serde_json::from_slice(&bytes)?;

        assert!(
            !fixture.name.trim().is_empty(),
            "{} has an empty name",
            path.display()
        );
        assert!(
            !fixture.description.trim().is_empty(),
            "{} has an empty description",
            path.display()
        );
        assert!(
            !fixture.spec.source.trim().is_empty()
                && !fixture.spec.url.trim().is_empty()
                && !fixture.spec.behavior.trim().is_empty(),
            "{} must cite the behavior under test",
            path.display()
        );

        let options = fixture.options.unwrap_or_default().into_options();
        let messages = fixture
            .input
            .into_iter()
            .map(FixtureMessage::into_message)
            .collect::<Vec<_>>();
        let expected = fixture
            .expected
            .threads
            .into_iter()
            .map(ExpectedThread::into_thread)
            .collect::<Vec<_>>();

        assert_eq!(
            thread_messages_with(&messages, &options),
            expected,
            "fixture {} failed",
            fixture.name
        );
    }

    Ok(())
}

#[test]
fn conformance_corpus_contains_required_behavior_fixtures() -> Result<(), Box<dyn Error>> {
    let mut actual_names = std::collections::BTreeSet::new();
    for entry in fixture_entries()? {
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("json") {
            let bytes = fs::read(entry.path())?;
            let fixture: Fixture = serde_json::from_slice(&bytes)?;
            actual_names.insert(fixture.name);
        }
    }

    for required in REQUIRED_FIXTURES {
        assert!(
            actual_names.contains(*required),
            "missing required conformance fixture {required}"
        );
    }

    Ok(())
}

#[test]
fn coverage_matrix_mentions_every_fixture() -> Result<(), Box<dyn Error>> {
    let matrix = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/rfc5256-coverage.md"),
    )?;

    for entry in fixture_entries()? {
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("json") {
            let bytes = fs::read(entry.path())?;
            let fixture: Fixture = serde_json::from_slice(&bytes)?;
            assert!(
                matrix.contains(&format!("`{}`", fixture.name)),
                "coverage matrix does not mention fixture {}",
                fixture.name
            );
        }
    }

    Ok(())
}

fn fixture_entries() -> Result<Vec<fs::DirEntry>, Box<dyn Error>> {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/conformance");
    let mut entries = fs::read_dir(&fixture_dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    assert!(!entries.is_empty(), "conformance corpus is empty");
    Ok(entries)
}

impl FixtureOptions {
    fn into_options(self) -> ThreadingOptions {
        let defaults = ThreadingOptions::default();
        ThreadingOptions {
            subject_merge: self.subject_merge.unwrap_or(defaults.subject_merge),
            prune_phantoms: self.prune_phantoms.unwrap_or(defaults.prune_phantoms),
            subject_prefixes: self.subject_prefixes.unwrap_or(defaults.subject_prefixes),
        }
    }
}

impl FixtureMessage {
    fn into_message(self) -> Message {
        Message {
            id: self.id,
            message_id: self.message_id,
            in_reply_to: self.in_reply_to,
            references: self.references,
            date: self.date,
            subject: self.subject,
        }
    }
}

impl ExpectedThread {
    fn into_thread(self) -> Thread {
        Thread {
            root_message_id: self.root,
            messages: self.messages,
        }
    }
}
