//! `mxr classify`: measure how well mxr sorts mail (blueprint 32). Today
//! only `eval`, which grades the mail in this mailbox against labelled
//! scenarios. No MCP, TUI or web surface, as for `mxr draft eval`: it is a
//! development measurement, not a mailbox action.

use crate::cli::{ClassifyAction, ClassifySetArg, OutputFormat};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use mxr_protocol::{
    ClassifyEvalMismatchData, ClassifyEvalQuestionData, ClassifyEvalSetData,
    ClassifyEvalSummaryData, Request, Response, ResponseData,
};
use std::fmt::Write as _;

const fn set_data(set: ClassifySetArg) -> ClassifyEvalSetData {
    match set {
        ClassifySetArg::Quick => ClassifyEvalSetData::Quick,
        ClassifySetArg::Full => ClassifyEvalSetData::Full,
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

const fn set_name(set: ClassifyEvalSetData) -> &'static str {
    match set {
        ClassifyEvalSetData::Quick => "quick",
        ClassifyEvalSetData::Full => "full",
    }
}

/// Mismatches listed in the table of the full set: the rest are grouped.
/// Every one is in `--format json`.
const CLUSTERS_SHOWN: usize = 25;

fn write_accuracy(out: &mut String, questions: &[ClassifyEvalQuestionData]) {
    let _ = writeln!(
        out,
        "{:<8} {:>9} {:>8} {:>9}",
        "question", "evaluated", "correct", "accuracy"
    );
    for q in questions {
        let _ = writeln!(
            out,
            "{:<8} {:>9} {:>8} {:>8.1}%",
            q.question, q.evaluated, q.correct, q.accuracy_percent
        );
    }
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
        "Classifier eval, {} set (label schema v{})",
        set_name(summary.set),
        summary.label_schema_version
    );
    let _ = writeln!(out, "home = {}", summary.home_projection);
    let _ = writeln!(out);
    write_accuracy(out, &summary.questions);
    if !summary.held_out_questions.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Held-out phrasings ({} emails; a judge that learned the generator scores lower here)",
            summary.held_out_present
        );
        write_accuracy(out, &summary.held_out_questions);
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
    if summary.set == ClassifyEvalSetData::Quick && summary.present < summary.labelled_in_set {
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
    match summary.set {
        ClassifyEvalSetData::Quick => write_mismatches(out, mismatches),
        ClassifyEvalSetData::Full => write_clusters(out, mismatches),
    }
}

fn write_mismatches(out: &mut String, mismatches: &[ClassifyEvalMismatchData]) {
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

/// The full set can be wrong thousands of times, so the table groups the
/// mismatches by what was expected and what was said, biggest first, with
/// one example each. `--format json` lists every one.
/// Scenario, question, expected and actual answer.
type ClusterKey<'a> = (&'a str, &'a str, &'a str, &'a str);

fn write_clusters(out: &mut String, mismatches: &[ClassifyEvalMismatchData]) {
    struct Cluster<'a> {
        count: usize,
        held_out: usize,
        example: &'a ClassifyEvalMismatchData,
    }
    let mut clusters: Vec<(ClusterKey, Cluster)> = Vec::new();
    for m in mismatches {
        let key = (
            m.scenario.as_str(),
            m.question.as_str(),
            m.expected.as_str(),
            m.actual.as_str(),
        );
        match clusters.iter_mut().find(|(k, _)| *k == key) {
            Some((_, cluster)) => {
                cluster.count += 1;
                cluster.held_out += usize::from(m.held_out);
            }
            None => clusters.push((
                key,
                Cluster {
                    count: 1,
                    held_out: usize::from(m.held_out),
                    example: m,
                },
            )),
        }
    }
    clusters.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(&b.0)));
    let _ = writeln!(
        out,
        "{} mismatches in {} clusters (same scenario, question, expected and actual answer); the largest {}:",
        mismatches.len(),
        clusters.len(),
        CLUSTERS_SHOWN.min(clusters.len())
    );
    for ((scenario, question, expected, actual), cluster) in clusters.iter().take(CLUSTERS_SHOWN) {
        let _ = write!(
            out,
            "{:>6}  {}  {}  expected {}  got {}",
            cluster.count,
            terminal_text(scenario),
            question,
            terminal_text(expected),
            terminal_text(actual),
        );
        if cluster.held_out > 0 {
            let _ = write!(out, "  ({} held out)", cluster.held_out);
        }
        if !cluster.example.reason.is_empty() {
            let _ = write!(out, "  ({})", terminal_text(&cluster.example.reason));
        }
        if cluster.example.label_review.is_some() {
            let _ = write!(out, "  [label is a judgement call]");
        }
        let _ = writeln!(
            out,
            "  e.g. {}",
            terminal_text(&cluster.example.provider_id)
        );
    }
    let _ = writeln!(out, "Use --format json for every mismatch.");
}

#[cfg(test)]
mod tests {
    use super::*;

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
            held_out_present: 0,
            held_out_questions: Vec::new(),
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
            held_out: false,
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

    fn full_summary() -> ClassifyEvalSummaryData {
        let question =
            |name: &str, evaluated, correct, accuracy_percent| ClassifyEvalQuestionData {
                question: name.into(),
                evaluated,
                correct,
                accuracy_percent,
            };
        ClassifyEvalSummaryData {
            set: ClassifyEvalSetData::Full,
            labelled_in_set: 40,
            present: 40,
            held_out_present: 10,
            held_out_questions: vec![question("who", 10, 7, 70.0)],
            questions: vec![question("who", 40, 36, 90.0)],
            ..summary(40)
        }
    }

    fn mismatch_of(scenario: &str, id: &str, held_out: bool) -> ClassifyEvalMismatchData {
        ClassifyEvalMismatchData {
            scenario: scenario.into(),
            provider_id: id.into(),
            held_out,
            ..mismatch()
        }
    }

    #[test]
    fn the_full_table_reports_held_out_accuracy_on_its_own_line() {
        let out = render_eval(&full_summary(), &[], &OutputFormat::Table).unwrap();
        assert!(out.contains("Classifier eval, full set"), "{out}");
        assert!(out.contains("Held-out phrasings (10 emails"), "{out}");
        assert!(out.contains("70.0%") && out.contains("90.0%"), "{out}");
        assert!(!out.contains("near-miss twins"), "{out}");
    }

    #[test]
    fn the_quick_table_has_no_held_out_line() {
        let out = render_eval(&summary(3), &[], &OutputFormat::Table).unwrap();
        assert!(out.contains("Classifier eval, quick set"), "{out}");
        assert!(!out.contains("Held-out"), "{out}");
    }

    /// Thousands of wrong answers are grouped, biggest first, with one
    /// example each; JSON still lists every one.
    #[test]
    fn the_full_table_groups_mismatches_into_clusters_and_json_keeps_them_all() {
        let mut mismatches = Vec::new();
        for n in 0..5 {
            mismatches.push(mismatch_of(
                "bulk-order-survey",
                &format!("demo-msg-{n}"),
                n < 2,
            ));
        }
        mismatches.push(mismatch_of("bulk-newsletter-issue", "demo-msg-90", false));
        let out = render_eval(&full_summary(), &mismatches, &OutputFormat::Table).unwrap();
        assert!(out.contains("6 mismatches in 2 clusters"), "{out}");
        let survey = out.find("bulk-order-survey").expect("the big cluster");
        let newsletter = out.find("bulk-newsletter-issue").expect("the small one");
        assert!(survey < newsletter, "biggest first: {out}");
        assert!(out.contains("     5  bulk-order-survey"), "{out}");
        assert!(out.contains("(2 held out)"), "{out}");
        assert_eq!(out.matches("e.g. ").count(), 2, "one example each: {out}");
        assert!(
            !out.contains("demo-msg-3"),
            "only the first of a cluster is shown: {out}"
        );

        let json = render_eval(&full_summary(), &mismatches, &OutputFormat::Json).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["mismatches"].as_array().unwrap().len(), 6);
        assert_eq!(value["mismatches"][0]["held_out"], true);
        assert_eq!(value["summary"]["held_out_present"], 10);
        assert_eq!(value["summary"]["set"], "full");
    }

    #[test]
    fn the_set_argument_maps_to_the_protocol_set() {
        assert_eq!(set_data(ClassifySetArg::Quick), ClassifyEvalSetData::Quick);
        assert_eq!(set_data(ClassifySetArg::Full), ClassifyEvalSetData::Full);
    }

    #[test]
    fn jsonl_ends_with_the_summary() {
        let out = render_eval(&summary(3), &[mismatch()], &OutputFormat::Jsonl).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("\"summary\""));
    }
}
