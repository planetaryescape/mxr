//! Records as CSV, with the preview that comes before writing: how many
//! rows, the total per currency, how many rows have an unchecked amount or
//! date, and how many have no PDF. The preview reads the same rows the
//! export writes, so what was previewed is what is written.

use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

/// One record as an export row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRow {
    pub record_id: String,
    pub date: Option<DateTime<Utc>>,
    pub kind: String,
    pub issuer: Option<String>,
    pub title: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub reference: Option<String>,
    pub checked: bool,
    /// The money and date fields still to confirm.
    pub unchecked_fields: Vec<String>,
    /// The record's best PDF, by filename.
    pub pdf: Option<String>,
    pub source_message_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExportPreview {
    pub rows: u32,
    /// Minor units per ISO currency.
    pub totals: BTreeMap<String, i64>,
    pub unchecked: u32,
    pub missing_pdfs: u32,
    pub by_kind: BTreeMap<String, u32>,
}

pub fn preview(rows: &[ExportRow]) -> ExportPreview {
    let mut out = ExportPreview::default();
    for row in rows {
        out.rows += 1;
        if let (Some(minor), Some(currency)) = (row.amount_minor, row.currency.as_ref()) {
            *out.totals.entry(currency.clone()).or_default() += minor;
        }
        if !row.checked {
            out.unchecked += 1;
        }
        if row.pdf.is_none() {
            out.missing_pdfs += 1;
        }
        *out.by_kind.entry(row.kind.clone()).or_default() += 1;
    }
    out
}

pub const CSV_HEADER: [&str; 12] = [
    "date",
    "kind",
    "issuer",
    "what",
    "amount",
    "currency",
    "reference",
    "checked",
    "unchecked_fields",
    "pdf",
    "record_id",
    "source_message_id",
];

/// A cell a spreadsheet would read as a formula (`=`, `+`, `-`, `@`, tab or
/// carriage return first) gets a leading quote, so mail text never runs as
/// one (CWE-1236).
fn neutralise(cell: String) -> String {
    if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{cell}")
    } else {
        cell
    }
}

/// The CSV text, header first, dates as the day in `tz`. Every text column
/// can hold words from mail, so each is neutralised; the date and amount
/// are written by mxr and stay as they are.
pub fn to_csv<Tz>(rows: &[ExportRow], tz: &Tz) -> anyhow::Result<String>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(CSV_HEADER)?;
    for row in rows {
        writer.write_record([
            row.date
                .map(|at| at.with_timezone(tz).format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            neutralise(row.kind.clone()),
            neutralise(row.issuer.clone().unwrap_or_default()),
            neutralise(row.title.clone().unwrap_or_default()),
            row.amount_minor
                .map(mxr_todo::money::plain_amount)
                .unwrap_or_default(),
            neutralise(row.currency.clone().unwrap_or_default()),
            neutralise(row.reference.clone().unwrap_or_default()),
            if row.checked { "yes" } else { "no" }.to_string(),
            neutralise(row.unchecked_fields.join(" ")),
            neutralise(row.pdf.clone().unwrap_or_default()),
            neutralise(row.record_id.clone()),
            neutralise(row.source_message_id.clone().unwrap_or_default()),
        ])?;
    }
    Ok(String::from_utf8(writer.into_inner()?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, minor: Option<i64>, checked: bool, pdf: Option<&str>) -> ExportRow {
        ExportRow {
            record_id: id.to_string(),
            date: Some(
                Utc.with_ymd_and_hms(2025, 3, 3, 12, 0, 0)
                    .single()
                    .expect("time"),
            ),
            kind: "receipt".to_string(),
            issuer: Some("Dell, Inc.".to_string()),
            title: Some("XPS \"14\"".to_string()),
            amount_minor: minor,
            currency: minor.map(|_| "GBP".to_string()),
            reference: Some("402-118".to_string()),
            checked,
            unchecked_fields: if checked {
                vec![]
            } else {
                vec!["amount".into()]
            },
            pdf: pdf.map(str::to_string),
            source_message_id: None,
        }
    }

    #[test]
    fn mail_text_that_looks_like_a_formula_is_quoted_so_a_spreadsheet_shows_it() {
        let mut evil = row("=HYPERLINK(\"http://x\")", Some(-500), true, Some("@pdf"));
        evil.issuer = Some("+cmd|' /C calc'!A0".to_string());
        evil.title = Some("-2+3".to_string());
        evil.reference = Some("\tREF".to_string());
        evil.kind = "receipt".to_string();
        evil.unchecked_fields = vec!["\rx".to_string()];
        evil.source_message_id = Some("@msg".to_string());
        let csv = to_csv(&[evil], &Utc).expect("csv");
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let cells: Vec<String> = reader
            .records()
            .next()
            .expect("a row")
            .expect("readable")
            .iter()
            .map(str::to_string)
            .collect();
        // date kind issuer what amount currency reference checked unchecked pdf record source
        assert_eq!(cells[2], "'+cmd|' /C calc'!A0");
        assert_eq!(cells[3], "'-2+3");
        assert_eq!(cells[6], "'\tREF");
        assert_eq!(cells[8], "'\rx");
        assert_eq!(cells[9], "'@pdf");
        assert_eq!(cells[10], "'=HYPERLINK(\"http://x\")");
        assert_eq!(cells[11], "'@msg");
        // A negative amount is a number mxr wrote, not mail text: it stays.
        assert_eq!(cells[4], "-5.00");
    }

    #[test]
    fn the_preview_counts_the_rows_the_csv_writes() {
        let rows = [
            row("a", Some(124_900), true, Some("invoice.pdf")),
            row("b", Some(1), false, None),
            row("c", None, true, None),
        ];
        let preview = preview(&rows);
        assert_eq!(preview.rows, 3);
        assert_eq!(preview.totals.get("GBP"), Some(&124_901));
        assert_eq!((preview.unchecked, preview.missing_pdfs), (1, 2));
        let csv = to_csv(&rows, &Utc).expect("csv");
        // Header plus one line per previewed row.
        assert_eq!(csv.lines().count(), 1 + preview.rows as usize);
        assert!(csv.contains(
            "2025-03-03,receipt,\"Dell, Inc.\",\"XPS \"\"14\"\"\",1249.00,GBP,402-118,yes"
        ));
        assert!(csv.contains(",no,amount,"));
    }
}
