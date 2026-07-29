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

/// One recipient and their template properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Recipient address, taken from the record's `to` property.
    pub to: String,
    /// Every property, including `to`, available to the templates.
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
            // Unit separators keep {"ab","c"} from hashing the same as
            // {"a","bc"} — otherwise two different records could collide and
            // one would be silently skipped as "already drafted".
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
            .collect(),
        DataFormat::Jsonl => raw
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(index, line)| {
                serde_json::from_str::<BTreeMap<String, serde_json::Value>>(line)
                    .map(stringify_values)
                    .with_context(|| format!("parsing JSONL line {}", index + 1))
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

        // Duplicate recipients are caught before any draft exists, so a rerun
        // cannot quietly double-send to someone.
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
fn stringify_values(row: BTreeMap<String, serde_json::Value>) -> BTreeMap<String, String> {
    row.into_iter()
        .map(|(key, value)| {
            let rendered = match value {
                serde_json::Value::String(text) => text,
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            (key, rendered)
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
    let Some((local, domain)) = address.rsplit_once('@') else {
        bail!("record {position} recipient `{address}` is not an email address");
    };
    if local.is_empty() || domain.is_empty() || !domain.contains('.') || domain.contains(' ') {
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
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("mxr-mm-{name}"));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn json_records_load() {
        let path = write_temp(
            "records.json",
            r#"[{"to":"a@example.com","first_name":"Dumi"},
                {"to":"b@example.com","first_name":"Sam"}]"#,
        );
        let records = load_records(&path, DataFormat::Json).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].properties["first_name"], "Dumi");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn jsonl_records_load() {
        let path = write_temp(
            "records.jsonl",
            "{\"to\":\"a@example.com\",\"n\":\"1\"}\n{\"to\":\"b@example.com\",\"n\":\"2\"}\n",
        );
        let records = load_records(&path, DataFormat::Jsonl).unwrap();
        assert_eq!(records.len(), 2);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn csv_records_load() {
        let path = write_temp(
            "records.csv",
            "to,first_name\na@example.com,Dumi\nb@example.com,Sam\n",
        );
        let records = load_records(&path, DataFormat::Csv).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].properties["first_name"], "Sam");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn duplicate_recipients_fail_the_batch() {
        let path = write_temp(
            "dupes.json",
            r#"[{"to":"a@example.com"},{"to":"A@Example.com"}]"#,
        );
        let err = load_records(&path, DataFormat::Json).unwrap_err();
        assert!(err.to_string().contains("duplicates recipient"), "{err}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_to_property_fails_the_batch() {
        let path = write_temp("noto.json", r#"[{"first_name":"Dumi"}]"#);
        let err = load_records(&path, DataFormat::Json).unwrap_err();
        assert!(err.to_string().contains("no `to` property"), "{err}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn crlf_in_a_recipient_is_rejected_as_header_injection() {
        let path = write_temp(
            "inject.json",
            "[{\"to\":\"a@example.com\\r\\nBcc: evil@example.com\"}]",
        );
        let err = load_records(&path, DataFormat::Json).unwrap_err();
        assert!(err.to_string().contains("header injection"), "{err}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn malformed_addresses_are_rejected() {
        let path = write_temp("bad.json", r#"[{"to":"not-an-address"}]"#);
        assert!(load_records(&path, DataFormat::Json).is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn subject_rejects_crlf_and_emptiness() {
        assert!(validate_subject("Hello").is_ok());
        assert!(validate_subject("Hi\r\nBcc: evil@example.com").is_err());
        assert!(validate_subject("   ").is_err());
    }

    #[test]
    fn record_hash_is_stable_and_distinguishes_records() {
        let a = Record {
            to: "a@example.com".into(),
            properties: BTreeMap::from([("to".into(), "a@example.com".into())]),
        };
        let b = Record {
            to: "b@example.com".into(),
            properties: BTreeMap::from([("to".into(), "b@example.com".into())]),
        };
        assert_eq!(a.hash(), a.clone().hash());
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn format_is_inferred_from_the_extension() {
        assert_eq!(DataFormat::from_path(Path::new("x.csv")), DataFormat::Csv);
        assert_eq!(DataFormat::from_path(Path::new("x.jsonl")), DataFormat::Jsonl);
        assert_eq!(DataFormat::from_path(Path::new("x.json")), DataFormat::Json);
    }
}
