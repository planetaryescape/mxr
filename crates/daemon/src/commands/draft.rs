use crate::cli::{DraftAction, DraftEvalSetArg, DraftLengthArg, OutputFormat, VoiceRegisterArg};
use crate::commands::draft_output::{
    draft_suggestion_json, eprint_draft_notes, length_data, register_data, DraftSuggestionView,
};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::DraftId;
use mxr_core::types::Address;
use mxr_protocol::*;
use std::str::FromStr;

pub async fn run(
    action: Option<DraftAction>,
    to: Option<String>,
    purpose: Option<String>,
    account: Option<String>,
    register: Option<VoiceRegisterArg>,
    length: Option<DraftLengthArg>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let fmt = resolve_format(format);
    match action {
        Some(DraftAction::Refine {
            draft_id,
            shorter,
            warmer,
            more_formal,
            less_emoji,
            add_context,
        }) => {
            let draft_id = DraftId::from_str(&draft_id)?;
            let resp = client
                .request(Request::DraftRefine {
                    draft_id,
                    knobs: DraftRefineKnobsData {
                        shorter,
                        warmer,
                        more_formal,
                        less_emoji,
                        add_context,
                    },
                    body: None,
                })
                .await?;
            print_draft_response(resp, fmt)
        }
        Some(DraftAction::Eval { limit, set }) => {
            let set = match set {
                DraftEvalSetArg::Mine => DraftEvalSet::Mine,
                DraftEvalSetArg::Quick => DraftEvalSet::Quick,
            };
            let account_id = match (set, account.as_deref()) {
                (DraftEvalSet::Mine, Some(account)) => {
                    Some(resolve_account(&mut client, Some(account)).await?)
                }
                _ => None,
            };
            let request = Request::DraftEval {
                account_id,
                limit,
                set,
            };
            // Quick makes one model call per case, so the whole run can take
            // minutes: it takes the no-deadline path that long operations use
            // (`request_with_events`, which passes no timeout). Mine keeps the
            // default deadline, as before.
            let resp = match set {
                DraftEvalSet::Quick => client.request_with_events(request, |_| {}).await?,
                DraftEvalSet::Mine => client.request(request).await?,
            };
            match resp {
                Response::Ok {
                    data: ResponseData::DraftEval { cases, summary },
                } => print_eval(&cases, &summary, fmt),
                Response::Ok {
                    data: ResponseData::DraftEvalQuick { cases, summary },
                } => print_eval_quick(&cases, &summary, fmt),
                Response::Ok { .. } => anyhow::bail!("Unexpected response"),
                Response::Error { message, .. } => anyhow::bail!(message),
            }
        }
        None => {
            let to = to.ok_or_else(|| anyhow::anyhow!("Pass --to for a new draft"))?;
            let purpose =
                purpose.ok_or_else(|| anyhow::anyhow!("Pass --purpose for a new draft"))?;
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::DraftCompose {
                    account_id: Some(account_id),
                    to: Some(Address {
                        name: None,
                        email: to,
                    }),
                    instruction: purpose,
                    source_message_id: None,
                    thread_id: None,
                    register: register.map(register_data),
                    length_hint: length.map(length_data),
                })
                .await?;
            print_draft_response(resp, fmt)
        }
    }
}

fn print_draft_response(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    let view = match resp {
        Response::Ok { data } => DraftSuggestionView::from_response(data)
            .ok_or_else(|| anyhow::anyhow!("Unexpected response"))?,
        Response::Error { message, .. } => anyhow::bail!(message),
    };
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&draft_suggestion_json(&view))?
            );
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&draft_suggestion_json(&view))?);
        }
        OutputFormat::Ids => {}
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["model", "rewrite_iterations", "body"])?;
            writer.write_record([
                view.model.clone(),
                view.rewrite_iterations.to_string(),
                view.body.clone(),
            ])?;
            print!("{}", String::from_utf8(writer.into_inner()?)?);
        }
        OutputFormat::Table => {
            println!("{}", view.body);
            eprint_draft_notes(&view);
        }
    }
    Ok(())
}

fn print_eval(
    cases: &[DraftEvalCaseData],
    summary: &DraftEvalSummaryData,
    fmt: OutputFormat,
) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "cases": cases, "summary": summary })
                )?
            );
        }
        OutputFormat::Jsonl => {
            for case in cases {
                println!("{}", serde_json::to_string(case)?);
            }
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({ "summary": summary }))?
            );
        }
        _ => {
            for case in cases {
                match &case.error {
                    Some(error) => println!("✗ {}: {error}", case.counterparty),
                    None => {
                        println!(
                            "── to {} · you wrote {} words, draft {} · greeting {} · sign-off {}{}",
                            case.counterparty,
                            case.actual_words,
                            case.draft_words,
                            if case.greeting_match {
                                "match"
                            } else {
                                "differs"
                            },
                            if case.sign_off_match {
                                "match"
                            } else {
                                "differs"
                            },
                            if case.invented_numbers.is_empty() {
                                String::new()
                            } else {
                                format!(" · invented {}", case.invented_numbers.join(", "))
                            }
                        );
                        println!("  you:   {}", case.actual.replace('\n', " / "));
                        println!("  draft: {}", case.draft.replace('\n', " / "));
                    }
                }
            }
            println!();
            println!(
                "{} replies ({} failed) with {}: length ratio {:.2} (1.00 is yours), greeting match {:.0}%, sign-off match {:.0}%, drafts with invented numbers {:.0}%",
                summary.cases,
                summary.failed,
                if summary.model.is_empty() { "the configured model" } else { summary.model.as_str() },
                summary.median_length_ratio,
                summary.greeting_match_rate * 100.0,
                summary.sign_off_match_rate * 100.0,
                summary.invented_number_rate * 100.0
            );
        }
    }
    Ok(())
}

fn print_eval_quick(
    cases: &[DraftEvalQuickCaseData],
    summary: &DraftEvalQuickSummaryData,
    fmt: OutputFormat,
) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "cases": cases, "summary": summary })
                )?
            );
        }
        OutputFormat::Jsonl => {
            for case in cases {
                println!("{}", serde_json::to_string(case)?);
            }
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({ "summary": summary }))?
            );
        }
        _ => {
            for case in cases {
                match &case.error {
                    Some(error) => println!("{} did not draft: {error}", case.id),
                    None => {
                        println!(
                            "── {} · {} words · {}",
                            case.id, case.words, case.description
                        );
                        for check in &case.checks {
                            println!(
                                "  {:<4} {:<9} {}; {}",
                                if check.pass { "pass" } else { "FAIL" },
                                check.check,
                                check.requirement,
                                check.detail
                            );
                        }
                        println!("       draft: {}", case.draft.replace('\n', " / "));
                    }
                }
            }
            println!();
            println!(
                "{} cases ({} did not draft) with {}; style score mean {:.0} (humanizer score: 100 reads plain, lower reads more like AI; reported, not graded)",
                summary.cases,
                summary.errors,
                if summary.model.is_empty() { "the configured model" } else { summary.model.as_str() },
                summary.style_mean
            );
            for count in &summary.checks {
                println!(
                    "{:<9} {}/{} passed",
                    count.check, count.passed, count.evaluated
                );
            }
        }
    }
    Ok(())
}
