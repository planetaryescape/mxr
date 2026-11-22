//! Loading and validating recipient property records.
//!
//! Records are data, never instructions. Property values are interpolated into
//! templates with HTML escaping on, and they are never logged, printed in
//! summaries, or written to the manifest — a property may well be an opaque
//! access token.

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub to: String,
    pub properties: BTreeMap<String, String>,
}

impl Record {
    /// Stable identity for idempotency, derived from the full property set.
    ///
    /// Hashed rather than stored: reruns must be able to recognise a record
    /// without the manifest ever holding a token value.
    pub fn hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for (key, value) in &self.properties {
            hasher.update(key.as_bytes());
            hasher.update(b"\x1f");
            hasher.update(value.as_bytes());
            hasher.update(b"\x1e");
        }
        hasher
            .finalize()
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Scrubs property values out of text that came back from `mxr`.
///
/// Scoped to the whole batch, not one record: `mxr`'s stderr is arbitrary text
/// and can quote back a body that was rendered for somebody else, so a scrubber
/// that only knew the current record would print one recipient's token while
/// reporting another recipient's failure.
///
/// Best-effort by construction — it can only match values verbatim, and a body
/// that reaches stderr HTML-escaped or percent-encoded will not match. That is
/// why it guards the operator's own terminal and nothing that gets written to
/// disk; see [`crate::manifest::FailureReason`].
pub struct Redactor {
    /// Longest first: redacting a short value that is a substring of a longer
    /// one would leave the tail of the longer one exposed.
    values: Vec<String>,
}

impl Redactor {
    pub fn for_batch(records: &[Record]) -> Self {
        let mut values: Vec<String> = records
            .iter()
            .flat_map(|record| record.properties.values())
            .filter(|value| value.chars().count() >= REDACT_MIN_CHARS)
            .cloned()
            .collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        Self { values }
    }

    pub fn scrub(&self, text: &str) -> String {
        let mut redacted = text.to_string();
        for value in &self.values {
            redacted = redacted.replace(value.as_str(), "[redacted]");
        }
        redacted
    }
}

/// Values shorter than this are left alone: they carry no secret worth the risk
/// of turning every error message into a row of `[redacted]`.
const REDACT_MIN_CHARS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Csv,
    Json,
    Jsonl,
}

impl DataFormat {
    /// Infer from the file extension, defaulting to JSON.
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("csv") => Self::Csv,
            Some("jsonl" | "ndjson") => Self::Jsonl,
            _ => Self::Json,
        }
    }
}

/// Read records from CSV, JSON, or JSONL and validate the whole batch.
///
/// Validation is all-or-nothing on purpose: a half-valid campaign that creates
/// drafts for the good rows and reports the bad ones later is how a partial,
/// inconsistent send happens.
pub fn load_records(path: &Path, format: DataFormat) -> anyhow::Result<Vec<Record>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading data file {}", path.display()))?;

    let rows: Vec<BTreeMap<String, String>> = match format {
        DataFormat::Csv => parse_csv(&raw)?,
        DataFormat::Json => serde_json::from_str::<Vec<BTreeMap<String, serde_json::Value>>>(&raw)
            .context("parsing JSON data file (expected an array of objects)")?
            .into_iter()
            .map(stringify_values)
            .collect::<anyhow::Result<Vec<_>>>()?,
        DataFormat::Jsonl => raw
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(index, line)| {
                let row = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(line)
                    .with_context(|| format!("parsing JSONL line {}", index + 1))?;
                stringify_values(row).with_context(|| format!("parsing JSONL line {}", index + 1))
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
    };

    if rows.is_empty() {
        bail!("data file {} contains no records", path.display());
    }

    let mut records = Vec::with_capacity(rows.len());
    let mut seen: HashSet<String> = HashSet::new();

    for (index, properties) in rows.into_iter().enumerate() {
        let position = index + 1;
        let to = properties
            .get("to")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("record {position} has no `to` property"))?;

        validate_address(&to, position)?;

        let lowered = to.to_ascii_lowercase();
        if !seen.insert(lowered) {
            bail!("record {position} duplicates recipient {to}");
        }

        records.push(Record { to, properties });
    }

    Ok(records)
}

fn parse_csv(raw: &str) -> anyhow::Result<Vec<BTreeMap<String, String>>> {
    let mut reader = csv::Reader::from_reader(raw.as_bytes());
    let headers = reader.headers().context("reading CSV header row")?.clone();

    reader
        .records()
        .enumerate()
        .map(|(index, row)| {
            let row = row.with_context(|| format!("reading CSV row {}", index + 1))?;
            Ok(headers
                .iter()
                .zip(row.iter())
                .map(|(key, value)| (key.trim().to_string(), value.to_string()))
                .collect())
        })
        .collect()
}

/// Flatten JSON scalars to strings; reject nested structures.
///
/// Templates interpolate text. Silently rendering an object as `[object]` or
/// its debug form would put junk in someone's email.
fn stringify_values(
    row: BTreeMap<String, serde_json::Value>,
) -> anyhow::Result<BTreeMap<String, String>> {
    row.into_iter()
        .map(|(key, value)| {
            let rendered = match value {
                serde_json::Value::String(text) => text,
                serde_json::Value::Null => String::new(),
                serde_json::Value::Bool(flag) => flag.to_string(),
                serde_json::Value::Number(number) => number.to_string(),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => bail!(
                    "property `{key}` is a nested value; template properties must be text, \
                     and rendering an object into someone's email is never what was meant"
                ),
            };
            Ok((key, rendered))
        })
        .collect()
}

/// Reject header injection and obviously malformed addresses.
///
/// A CR or LF in an address is the classic way to forge extra headers; it is
/// rejected here, before the address reaches mxr.
fn validate_address(address: &str, position: usize) -> anyhow::Result<()> {
    if address.contains(['\r', '\n']) {
        bail!("record {position} recipient contains a line break (header injection)");
    }
    if address.contains([',', ';', '<', '>']) || address.chars().any(char::is_whitespace) {
        bail!(
            "record {position} recipient `{address}` must be a single bare address \
             (no list separators, display name, or spaces)"
        );
    }
    let Some((local, domain)) = address.rsplit_once('@') else {
        bail!("record {position} recipient `{address}` is not an email address");
    };
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        bail!("record {position} recipient `{address}` is not an email address");
    }
    Ok(())
}

/// Reject CR/LF in a rendered subject, which would forge headers.
pub fn validate_subject(subject: &str) -> anyhow::Result<()> {
    if subject.contains(['\r', '\n']) {
        bail!("rendered subject contains a line break (header injection)");
    }
    if subject.trim().is_empty() {
        bail!("rendered subject is empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::*;

    /// Each test gets its own directory, so a data file one test writes can
    /// never be the data file another test reads.
    fn data_file(contents: &str, name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    fn load(contents: &str, name: &str) -> anyhow::Result<Vec<Record>> {
        let (dir, path) = data_file(contents, name);
        let format = DataFormat::from_path(&path);
        let result = load_records(&path, format);
        drop(dir);
        result
    }

    fn record(pairs: &[(&str, &str)]) -> Record {
        let properties: BTreeMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        Record {
            to: properties.get("to").cloned().unwrap_or_default(),
            properties,
        }
    }

    #[test]
    fn the_three_formats_describe_the_same_records() {
        let csv = load(
            "to,first_name\na@example.com,Dumi\nb@example.com,Sam\n",
            "records.csv",
        )
        .unwrap();
        let json = load(
            r#"[{"to":"a@example.com","first_name":"Dumi"},
                {"to":"b@example.com","first_name":"Sam"}]"#,
            "records.json",
        )
        .unwrap();
        let jsonl = load(
            "{\"to\":\"a@example.com\",\"first_name\":\"Dumi\"}\n\n\
             {\"to\":\"b@example.com\",\"first_name\":\"Sam\"}\n",
            "records.jsonl",
        )
        .unwrap();

        for records in [&csv, &json, &jsonl] {
            assert_eq!(records.len(), 2);
            assert_eq!(records[0].to, "a@example.com");
            assert_eq!(records[0].properties["first_name"], "Dumi");
            assert_eq!(records[1].to, "b@example.com");
            assert_eq!(records[1].properties["first_name"], "Sam");
        }
        let hashes = |records: &[Record]| records.iter().map(Record::hash).collect::<Vec<_>>();
        assert_eq!(hashes(&csv), hashes(&json));
        assert_eq!(hashes(&csv), hashes(&jsonl));
    }

    #[test]
    fn a_missing_to_property_fails_the_whole_batch() {
        let err = load(
            r#"[{"to":"a@example.com"},{"first_name":"Dumi"}]"#,
            "records.json",
        )
        .unwrap_err();
        assert!(err.to_string().contains("record 2"), "{err}");
        assert!(err.to_string().contains("no `to` property"), "{err}");
    }

    #[test]
    fn duplicate_recipients_are_rejected_case_insensitively() {
        let err = load(
            r#"[{"to":"a@example.com"},{"to":"A@Example.COM"}]"#,
            "records.json",
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicates recipient"), "{err}");
    }

    #[test]
    fn a_line_break_in_a_recipient_is_rejected_as_header_injection() {
        for injected in [
            "a@example.com\\r\\nBcc: evil@example.com",
            "a@example.com\\nBcc: evil@example.com",
            "a@example.com\\rBcc: evil@example.com",
        ] {
            let contents = format!(r#"[{{"to":"{injected}"}}]"#);
            let err = load(&contents, "records.json").unwrap_err();
            assert!(
                err.to_string().contains("header injection"),
                "accepted `{injected}`: {err}"
            );
        }
    }

    #[test]
    fn record_hash_distinguishes_records_that_share_a_key_boundary() {
        assert_ne!(record(&[("a", "bc")]).hash(), record(&[("ab", "c")]).hash());
        assert_ne!(
            record(&[("to", "a@example.com")]).hash(),
            record(&[("to", "b@example.com")]).hash()
        );
        assert_ne!(
            record(&[("to", "a@example.com"), ("plan", "pro")]).hash(),
            record(&[("to", "a@example.com"), ("plan", "free")]).hash()
        );
        assert_ne!(
            record(&[("to", "a@example.com")]).hash(),
            record(&[("to", "a@example.com"), ("plan", "pro")]).hash()
        );
    }

    #[test]
    fn redaction_removes_property_values_from_error_text() {
        let batch = [record(&[
            ("to", "a@example.com"),
            ("url", "https://x.example/r?t=opaque-token-xyz"),
        ])];
        let leaked = "compose failed: <a href=\"https://x.example/r?t=opaque-token-xyz\">";
        let safe = Redactor::for_batch(&batch).scrub(leaked);
        assert!(!safe.contains("opaque-token-xyz"), "{safe}");
        assert!(safe.contains("compose failed"), "{safe}");
    }
}
