//! `mxr classify`: measure how well mxr sorts mail (blueprint 32). Today
//! only `eval`, which grades the mail in this mailbox against labelled
//! scenarios. No MCP, TUI or web surface, as for `mxr draft eval`: it is a
//! development measurement, not a mailbox action.

use crate::cli::{ClassifyAction, ClassifySetArg, OutputFormat};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use mxr_protocol::{
    ClassifyEvalMismatchData, ClassifyEvalSetData, ClassifyEvalSummaryData, Request, Response,
    ResponseData,
};
use std::fmt::Write as _;

const fn set_data(set: ClassifySetArg) -> ClassifyEvalSetData {
    match set {
        ClassifySetArg::Quick => ClassifyEvalSetData::Quick,
    }
}

pub async fn run(
    action: ClassifyAction,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    match action {
        ClassifyAction::Eval { set } => {
            let (summary, mismatches) = expect_response(
                client
                    .request(Request::ClassifyEval {
                        account_id,
                        set: set_data(set),
                    })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data:
                            ResponseData::ClassifyEval {
                                summary,
                                mismatches,
                            },
                    } => Some((summary, mismatches)),
                    _ => None,
                },
            )?;
            print!("{}", render_eval(&summary, &mismatches, &format)?);
        }
    }
    Ok(())
}

fn render_eval(
    summary: &ClassifyEvalSummaryData,
    mismatches: &[ClassifyEvalMismatchData],
    format: &OutputFormat,
) -> anyhow::Result<String> {
    let mut out = String::new();
    match format {
        OutputFormat::Json => {
            out.push_str(&serde_json::to_string_pretty(
                &serde_json::json!({ "summary": summary, "mismatches": mismatches }),
            )?);
            out.push('\n');
        }
        OutputFormat::Jsonl => {
            for mismatch in mismatches {
                out.push_str(&serde_json::to_string(mismatch)?);
                out.push('\n');
            }
            out.push_str(&serde_json::to_string(
                &serde_json::json!({ "summary": summary }),
            )?);
            out.push('\n');
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "scenario",
                "provider_id",
                "question",
                "expected",
                "actual",
                "reason",
            ])?;
            for m in mismatches {
                writer.write_record([
                    &m.scenario,
                    &m.provider_id,
                    &m.question,
                    &m.expected,
                    &m.actual,
                    &m.reason,
                ])?;
            }
            out.push_str(&String::from_utf8(writer.into_inner()?)?);
        }
        OutputFormat::Ids => {
            let mut ids: Vec<&str> = mismatches.iter().map(|m| m.provider_id.as_str()).collect();
            ids.dedup();
            for id in ids {
                let _ = writeln!(out, "{id}");
            }
        }
        OutputFormat::Table => render_table(&mut out, summary, mismatches),
    }
    Ok(out)
}

fn render_table(
    out: &mut String,
    summary: &ClassifyEvalSummaryData,
    mismatches: &[ClassifyEvalMismatchData],
) {
    if let Some(message) = &summary.message {
        let _ = writeln!(out, "{message}");
        return;
    }
    let _ = writeln!(
        out,
        "Classifier eval, quick set (label schema v{})",
        summary.label_schema_version
    );
    let _ = writeln!(out, "home = {}", summary.home_projection);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<8} {:>9} {:>8} {:>9}",
        "question", "evaluated", "correct", "accuracy"
    );
    for q in &summary.questions {
        let _ = writeln!(
            out,
            "{:<8} {:>9} {:>8} {:>8.1}%",
            q.question, q.evaluated, q.correct, q.accuracy_percent
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{} of {} labelled emails are in this mailbox; {} stored emails have no label.",
        summary.present, summary.labelled_in_set, summary.unlabelled
    );
    if summary.placed_live > 0 {
        let _ = writeln!(
            out,
            "{} have no arrivals row (older than 30 days when the account first synced); their home was worked out live.",
            summary.placed_live
        );
    }
    if summary.not_placed > 0 {
        let _ = writeln!(
            out,
            "{} could not be placed at all and are not graded on home.",
            summary.not_placed
        );
    }
    if summary.present < summary.labelled_in_set {
        let _ = writeln!(
            out,
            "The rest are near-miss twins that are not part of the demo mailbox; the daemon test grades them."
        );
    }
    let _ = writeln!(out);
    if mismatches.is_empty() {
        let _ = writeln!(out, "No mismatches.");
        return;
    }
    let _ = writeln!(out, "{} mismatches", mismatches.len());
    let width =
        |f: fn(&ClassifyEvalMismatchData) -> usize| mismatches.iter().map(f).max().unwrap_or(0);
    let scenario_w = width(|m| m.scenario.chars().count());
    let id_w = width(|m| m.provider_id.chars().count());
    let expected_w = width(|m| m.expected.chars().count());
    let actual_w = width(|m| m.actual.chars().count());
    for m in mismatches {
        let _ = write!(
            out,
            "{:<scenario_w$}  {:<id_w$}  {:<6}  expected {:<expected_w$}  got {:<actual_w$}",
            terminal_text(&m.scenario),
            terminal_text(&m.provider_id),
            m.question,
            terminal_text(&m.expected),
            terminal_text(&m.actual),
        );
        if !m.reason.is_empty() {
            let _ = write!(out, "  ({})", terminal_text(&m.reason));
        }
        if m.label_review.is_some() {
            let _ = write!(out, "  [label is a judgement call]");
        }
        let _ = writeln!(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::ClassifyEvalQuestionData;

    fn summary(present: u32) -> ClassifyEvalSummaryData {
        ClassifyEvalSummaryData {
            set: ClassifyEvalSetData::Quick,
            label_schema_version: 1,
            labelled_in_set: 10,
            present,
            placed_live: 0,
            not_placed: 0,
            unlabelled: 4,
            questions: vec![ClassifyEvalQuestionData {
                question: "who".into(),
                evaluated: present,
                correct: present,
                accuracy_percent: 100.0,
            }],
            home_projection: "arrivals".into(),
            message: (present == 0).then(|| "No labelled mail here; run mxr demo".into()),
        }
    }

    fn mismatch() -> ClassifyEvalMismatchData {
        ClassifyEvalMismatchData {
            scenario: "s".into(),
            provider_id: "demo-msg-1".into(),
            question: "home".into(),
            expected: "updates".into(),
            actual: "messages".into(),
            reason: "possible personal mail".into(),
            twin_of: None,
            label_review: Some("a policy call".into()),
        }
    }

    #[test]
    fn json_carries_the_summary_and_every_mismatch() {
        let out = render_eval(&summary(3), &[mismatch()], &OutputFormat::Json).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["summary"]["present"], 3);
        assert_eq!(value["mismatches"][0]["provider_id"], "demo-msg-1");
        assert_eq!(value["mismatches"][0]["expected"], "updates");
    }

    #[test]
    fn the_table_lists_each_mismatch_with_its_reason() {
        let out = render_eval(&summary(3), &[mismatch()], &OutputFormat::Table).unwrap();
        assert!(out.contains("expected updates"), "{out}");
        assert!(out.contains("got messages"), "{out}");
        assert!(out.contains("(possible personal mail)"), "{out}");
        assert!(out.contains("[label is a judgement call]"), "{out}");
        assert!(out.contains("The rest are near-miss twins"), "{out}");
    }

    #[test]
    fn an_empty_mailbox_says_so_in_plain_words() {
        let out = render_eval(&summary(0), &[], &OutputFormat::Table).unwrap();
        assert_eq!(out, "No labelled mail here; run mxr demo\n");
    }

    #[test]
    fn jsonl_ends_with_the_summary() {
        let out = render_eval(&summary(3), &[mismatch()], &OutputFormat::Jsonl).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("\"summary\""));
    }
}
