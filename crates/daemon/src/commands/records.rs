//! `mxr records`: Archive's records, the answer box and the export.
//!
//! The daemon files the records, builds the ledger, ranks answers and
//! writes the CSV; this prints them as text, or passes the JSON through
//! for scripts and agents.

use crate::cli::{OutputFormat, RecordFilterArgs, RecordsAction};
use crate::commands::selection::parse_message_id;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, print_json, resolve_format, terminal_block};
use chrono::Local;
use mxr_protocol::{
    archive_copy, RecordAnswerData, RecordChangeData, RecordData, RecordEditData, RecordExportData,
    RecordFilterData, RecordKindData, RecordLedgerData, Request, Response, ResponseData,
};
use std::fmt::Write as _;

macro_rules! expect_data {
    ($response:expr, $variant:ident, $field:ident) => {
        expect_response($response, |response| match response {
            Response::Ok {
                data: ResponseData::$variant { $field },
            } => Some($field),
            _ => None,
        })?
    };
}

pub async fn run(
    action: Option<RecordsAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let format = resolve_format(format);
    let action = action.unwrap_or_else(|| RecordsAction::List {
        filter: RecordFilterArgs::default(),
        limit: 200,
        offset: 0,
    });
    match action {
        RecordsAction::List {
            filter,
            limit,
            offset,
        } => {
            let ledger = expect_data!(
                client
                    .request(Request::ListRecords {
                        account_id,
                        filter: filter_data(&filter)?,
                        limit,
                        offset,
                    })
                    .await?,
                RecordLedger,
                ledger
            );
            match format {
                OutputFormat::Json => print_json(&ledger, format),
                OutputFormat::Jsonl => {
                    println!("{}", jsonl(&ledger.records)?);
                    Ok(())
                }
                OutputFormat::Ids => {
                    ledger.records.iter().for_each(|r| println!("{}", r.id));
                    Ok(())
                }
                _ => {
                    print!("{}", terminal_block(&ledger_text(&ledger)));
                    Ok(())
                }
            }
        }
        RecordsAction::Show { record_id } => {
            let record = expect_data!(
                client.request(Request::GetRecord { record_id }).await?,
                Record,
                record
            );
            match format {
                OutputFormat::Json | OutputFormat::Jsonl => print_json(&record, format),
                _ => {
                    print!("{}", terminal_block(&card_text(&record)));
                    Ok(())
                }
            }
        }
        RecordsAction::Ask { query, no_fallback } => {
            let answer = expect_data!(
                client
                    .request(Request::AnswerFromRecords {
                        query,
                        account_id,
                        fallback: !no_fallback,
                        limit: 4,
                    })
                    .await?,
                RecordAnswer,
                answer
            );
            match format {
                OutputFormat::Json | OutputFormat::Jsonl => print_json(&answer, format),
                OutputFormat::Ids => {
                    if let Some(card) = &answer.answer {
                        println!("{}", card.copy);
                    }
                    Ok(())
                }
                _ => {
                    print!("{}", terminal_block(&answer_text(&answer)));
                    Ok(())
                }
            }
        }
        RecordsAction::Fix {
            record_id,
            edit,
            confirm,
            confirm_all,
            clear,
            sender,
            dry_run,
        } => {
            let edit = match (edit, confirm, confirm_all, clear) {
                (Some(edit), _, _, _) => {
                    let (field, value) = edit.split_once('=').ok_or_else(|| {
                        anyhow::anyhow!(
                            "Write a fix as field=value, like amount=£12.50 or issuer=Amazon."
                        )
                    })?;
                    RecordEditData::Set {
                        field: field.trim().to_string(),
                        value: value.trim().to_string(),
                    }
                }
                (None, Some(field), _, _) => RecordEditData::Confirm { field },
                (None, None, true, _) => RecordEditData::ConfirmAll,
                (None, None, false, Some(field)) => RecordEditData::Clear { field },
                (None, None, false, None) => anyhow::bail!(
                    "Say what to fix: field=value, --confirm FIELD, --confirm-all or --clear FIELD."
                ),
            };
            let request = Request::SetRecordField {
                record_id,
                edit,
                apply_to_sender: sender,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        RecordsAction::Dismiss {
            record_ids,
            restore,
            dry_run,
        } => {
            let request = Request::DismissRecord {
                record_ids,
                restore,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        RecordsAction::File {
            message_id,
            kind,
            dry_run,
        } => {
            let request = Request::FileRecord {
                message_id: parse_message_id(&message_id)?,
                kind: kind.as_deref().map(parse_kind).transpose()?,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        RecordsAction::Sender {
            message_id,
            always,
            never,
            clear,
            kind,
            dry_run,
        } => {
            let verdict = match (always, never, clear) {
                (true, _, _) => Some("always".to_string()),
                (_, true, _) => Some("never".to_string()),
                (_, _, true) => None,
                _ => anyhow::bail!("Say --always, --never or --clear."),
            };
            let request = Request::SetRecordSender {
                message_id: parse_message_id(&message_id)?,
                verdict,
                kind: kind.as_deref().map(parse_kind).transpose()?,
                dry_run,
            };
            print_change(change(&mut client, request).await?, format)
        }
        RecordsAction::Export {
            csv: _,
            filter,
            out,
            pdfs,
            dry_run,
        } => {
            let attachments_dir = match pdfs {
                Some(dir) => Some(absolute(dir)?.display().to_string()),
                None => None,
            };
            let export = expect_data!(
                client
                    .request(Request::ExportRecords {
                        account_id,
                        filter: filter_data(&filter)?,
                        attachments_dir,
                        dry_run,
                    })
                    .await?,
                RecordExport,
                export
            );
            print_export(&export, out.as_deref(), format)
        }
    }
}

fn absolute(path: std::path::PathBuf) -> anyhow::Result<std::path::PathBuf> {
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn parse_kind(raw: &str) -> anyhow::Result<RecordKindData> {
    RecordKindData::parse(raw).ok_or_else(|| {
        anyhow::anyhow!(
            "{raw} is not a kind of record; use receipt, order, booking, invoice, statement, ticket, contract, warranty or account."
        )
    })
}

/// "99.50", "£1,200" to minor units.
fn parse_minor(raw: &str) -> anyhow::Result<i64> {
    mxr_todo::money::parse_minor(raw.trim().trim_start_matches(['£', '$', '€']))
        .ok_or_else(|| anyhow::anyhow!("{raw} is not an amount."))
}

fn filter_data(args: &RecordFilterArgs) -> anyhow::Result<RecordFilterData> {
    Ok(RecordFilterData {
        kinds: args
            .kind
            .iter()
            .map(|kind| parse_kind(kind))
            .collect::<anyhow::Result<Vec<_>>>()?,
        issuer: args.issuer.clone(),
        year: args.year,
        min_amount_minor: args.min.as_deref().map(parse_minor).transpose()?,
        max_amount_minor: args.max.as_deref().map(parse_minor).transpose()?,
        has_pdf: match (args.has_pdf, args.no_pdf) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        },
        checked: match (args.checked, args.unchecked) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        },
        group_id: args.group.clone(),
    })
}

async fn change(client: &mut IpcClient, request: Request) -> anyhow::Result<RecordChangeData> {
    Ok(expect_data!(
        client.request(request).await?,
        RecordChange,
        change
    ))
}

fn print_change(change: RecordChangeData, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json | OutputFormat::Jsonl => print_json(&change, format),
        OutputFormat::Ids => {
            change.records.iter().for_each(|r| println!("{}", r.id));
            Ok(())
        }
        _ => {
            let mut out = format!("{}\n", change.message);
            for record in &change.records {
                out.push_str(&row_line(record));
            }
            print!("{}", terminal_block(&out));
            Ok(())
        }
    }
}

fn print_export(
    export: &RecordExportData,
    out: Option<&std::path::Path>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    if export.dry_run {
        return match format {
            OutputFormat::Json | OutputFormat::Jsonl => print_json(export, format),
            _ => {
                let mut text = format!("Would export {}\n", export.summary);
                for kind in &export.by_kind {
                    let _ = writeln!(text, "  {:<10} {}", kind.label, kind.count);
                }
                print!("{}", terminal_block(&text));
                Ok(())
            }
        };
    }
    let csv = export.csv.as_deref().unwrap_or_default();
    match out {
        Some(path) => {
            std::fs::write(path, csv)?;
            eprintln!("Exported {} to {}", export.summary, path.display());
        }
        None => print!("{csv}"),
    }
    if let Some(dir) = &export.attachments_dir {
        eprintln!("Copied {} PDFs to {dir}.", export.pdfs_copied);
        for error in &export.pdf_errors {
            eprintln!("  not copied: {error}");
        }
    }
    Ok(())
}

fn day(at: chrono::DateTime<chrono::Utc>) -> String {
    at.with_timezone(&Local).format("%d %b").to_string()
}

fn row_line(record: &RecordData) -> String {
    let mut line = format!(
        "  {}  {:<16} {:<28} {:>12}  {:<12} {}  {}\n",
        record.date.map_or_else(|| "      ".to_string(), day),
        mxr_todo::text::clip(record.issuer.as_deref().unwrap_or("-"), 16),
        mxr_todo::text::clip(record.title.as_deref().unwrap_or(&record.kind_label), 28),
        record
            .amount
            .as_ref()
            .map(|a| {
                if record.unchecked_fields.iter().any(|f| f == "amount") {
                    format!("{} ?", a.display)
                } else {
                    a.display.clone()
                }
            })
            .unwrap_or_default(),
        mxr_todo::text::clip(record.reference.as_deref().unwrap_or(""), 12),
        if record.pdf.is_some() { "PDF" } else { "-  " },
        record.id,
    );
    for extra in [&record.stage_line, &record.detail_line]
        .into_iter()
        .flatten()
    {
        let _ = writeln!(line, "          {extra}");
    }
    line
}

fn ledger_text(ledger: &RecordLedgerData) -> String {
    let mut out = format!("Archive  {} records\n{}\n", ledger.total, ledger.header);
    if let Some(first_run) = &ledger.first_run {
        let _ = writeln!(out, "{}", first_run.line);
    }
    if let Some(empty) = &ledger.empty_state {
        let _ = writeln!(out, "\n{empty}");
        if ledger.total == 0 {
            let _ = writeln!(out, "{}", archive_copy::ADD_ONE_CLI);
        }
        return out;
    }
    if let Some(issuer) = &ledger.issuer {
        let totals: Vec<&str> = issuer.totals.iter().map(|t| t.display.as_str()).collect();
        let _ = writeln!(
            out,
            "\n{}: {} records, {}",
            issuer.name,
            issuer.count,
            totals.join(" and ")
        );
    }
    if !ledger.coming_up.is_empty() {
        out.push_str("\nComing up\n");
        for moment in &ledger.coming_up {
            let _ = writeln!(out, "  {}", moment.label);
        }
    }
    let month_of_row = |record: &RecordData| {
        record.date.map(|at| {
            let local = at.with_timezone(&Local);
            format!("{}-{:02}", local.format("%Y"), local.format("%m"))
        })
    };
    let mut current: Option<String> = None;
    for record in &ledger.records {
        let month = month_of_row(record);
        if month != current {
            if let Some(header) = ledger
                .months
                .iter()
                .find(|m| Some(&m.month) == month.as_ref())
            {
                let totals: Vec<&str> = header.totals.iter().map(|t| t.display.as_str()).collect();
                let _ = writeln!(
                    out,
                    "\n{}  {} · {}",
                    header.label,
                    header.count,
                    if totals.is_empty() {
                        "no amounts".to_string()
                    } else {
                        totals.join(" + ")
                    }
                );
            }
            current = month;
        }
        out.push_str(&row_line(record));
    }
    if ledger.matching as usize > ledger.records.len() {
        let _ = writeln!(
            out,
            "\n{} of {} shown. Page with --offset.",
            ledger.records.len(),
            ledger.matching
        );
    }
    out
}

fn card_text(record: &RecordData) -> String {
    let mut out = format!(
        "{} · {}\n",
        record.issuer.as_deref().unwrap_or("-"),
        record.kind_label
    );
    if let Some(title) = &record.title {
        let _ = writeln!(out, "{title}");
    }
    out.push('\n');
    for field in &record.fields {
        if field.field == "issuer" || field.field == "title" {
            continue;
        }
        let dot = if field.checked { " " } else { "○" };
        let _ = writeln!(
            out,
            "{dot} {:<12} {:<24} ({}{})",
            field.label,
            field.value,
            field.source_label,
            field
                .evidence
                .as_deref()
                .filter(|e| *e != field.value)
                .map(|e| format!(": \"{e}\""))
                .unwrap_or_default()
        );
    }
    if let Some(line) = &record.stage_line {
        let _ = writeln!(out, "\n{line}");
    }
    if let Some(group) = &record.group {
        let _ = writeln!(
            out,
            "Part of {} \"{}\" ({})",
            group.kind, group.title, group.count
        );
    }
    if !record.documents.is_empty() {
        out.push_str("\nDocuments\n");
        for document in &record.documents {
            let _ = writeln!(
                out,
                "  {}  {} KB{}",
                document.filename,
                document.size_bytes / 1024,
                if document.on_disk {
                    ""
                } else {
                    "  (downloads when opened)"
                }
            );
        }
    }
    if !record.sources.is_empty() {
        let _ = writeln!(out, "\nFrom {} emails", record.sources.len());
        for source in &record.sources {
            let _ = writeln!(
                out,
                "  {}  {:<12} {}  {}",
                day(source.date),
                source.stage,
                mxr_todo::text::clip(&source.subject, 50),
                source.message_id
            );
        }
    }
    if let Some(others) = record.issuer_records.filter(|n| *n > 0) {
        let _ = writeln!(
            out,
            "\nAlso from {}: {others} more records (mxr records --issuer \"{}\")",
            record.issuer.as_deref().unwrap_or("-"),
            record.issuer.as_deref().unwrap_or("")
        );
    }
    let _ = writeln!(out, "\n{}", record.why);
    out
}

fn answer_text(answer: &RecordAnswerData) -> String {
    let mut out = String::new();
    match (&answer.answer, &answer.fallback) {
        (Some(card), _) => {
            let _ = writeln!(out, "{}  {}", card.label, card.value);
            let record = &card.record;
            let mut line = Vec::new();
            line.extend(record.issuer.clone());
            line.extend(record.title.clone());
            line.extend(
                record
                    .date
                    .map(|at| at.with_timezone(&Local).format("%-d %b %Y").to_string()),
            );
            let _ = writeln!(out, "{}", line.join(" · "));
            if let Some(group) = &record.group {
                let _ = writeln!(
                    out,
                    "Part of {} \"{}\" ({})",
                    group.kind, group.title, group.count
                );
            }
            if let Some(provenance) = &card.provenance {
                let _ = writeln!(
                    out,
                    "from {} · {}",
                    provenance.source_label,
                    if provenance.checked {
                        "checked"
                    } else {
                        "unchecked"
                    }
                );
            }
            if let Some(pdf) = &record.pdf {
                let _ = writeln!(out, "PDF: {}", pdf.filename);
            }
            let _ = writeln!(out, "Record {}", record.id);
            if !answer.also.is_empty() {
                let also: Vec<String> = answer
                    .also
                    .iter()
                    .map(|r| {
                        let name = r
                            .title
                            .clone()
                            .or_else(|| r.issuer.clone())
                            .unwrap_or_default();
                        match &r.reference {
                            Some(reference) => format!("{name} ({reference})"),
                            None => name,
                        }
                    })
                    .collect();
                let _ = writeln!(out, "Also matching: {}", also.join(", "));
            }
        }
        (None, Some(fallback)) => {
            let _ = writeln!(out, "{}", fallback.note);
            if let Some(found) = &fallback.answer {
                let _ = writeln!(out, "\n{}", found.text);
                for citation in &found.citations {
                    let _ = writeln!(
                        out,
                        "  {}  {}  {}",
                        day(citation.date),
                        mxr_todo::text::clip(&citation.subject, 60),
                        citation.message_id
                    );
                }
            }
            if let Some(error) = &fallback.error {
                let _ = writeln!(out, "Searching all mail failed: {error}");
            }
        }
        (None, None) => out.push_str("No record matches.\n"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_parse_to_minor_units() {
        assert_eq!(parse_minor("99.50").ok(), Some(9950));
        assert_eq!(parse_minor("£1,200").ok(), Some(120_000));
        assert!(parse_minor("lots").is_err());
    }
}
