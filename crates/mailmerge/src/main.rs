//! `mxr-mailmerge` — a companion to mxr, not a plugin.
//!
//! It renders one personalised message per record and asks mxr to create a
//! draft for each, through the ordinary `mxr` CLI. It holds no credentials,
//! never opens mxr's database, and never talks to a mail provider. Everything
//! it does, a shell script could do; it exists to do it carefully.
//!
//! Drafting and sending are separate commands. The default operation creates
//! drafts and stops.

mod manifest;
mod mxr;
mod records;
mod template;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use manifest::{Manifest, RecordEntry, RecordStatus};
use std::path::{Path, PathBuf};
use template::Templates;

#[derive(Parser)]
#[command(
    name = "mxr-mailmerge",
    about = "Create personalised mxr drafts from a template and a list of records",
    long_about = "Renders one message per record and asks the mxr CLI to save each as a local \
                  draft. Never sends by itself: `mxr-mailmerge send` is a separate, explicitly \
                  confirmed step that hands each draft back to mxr.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render every record and create one local mxr draft each.
    Draft(DraftArgs),
    /// Send the drafts of a campaign. Separate from drafting, and confirmed.
    Send(SendArgs),
    /// Show per-record status for a campaign.
    Status(StatusArgs),
}

#[derive(clap::Args)]
struct DraftArgs {
    /// mxr account to draft from.
    #[arg(long)]
    account: String,
    /// Template for the subject line. Not HTML-escaped.
    #[arg(long, value_name = "PATH")]
    subject_template: PathBuf,
    /// HTML body template. Interpolated values are HTML-escaped.
    #[arg(long, value_name = "PATH")]
    html_template: PathBuf,
    /// Optional plain-text template. Without it mxr generates the text
    /// alternative from the rendered HTML.
    #[arg(long, value_name = "PATH")]
    text_template: Option<PathBuf>,
    /// Recipient records: CSV, JSON, or JSONL. Each needs a `to` property.
    #[arg(long, value_name = "PATH")]
    data: PathBuf,
    /// Inline image as CID=PATH, shared by every message. Repeatable.
    #[arg(long, value_name = "CID=PATH", action = clap::ArgAction::Append)]
    inline: Vec<String>,
    /// Attachment shared by every message. Repeatable.
    #[arg(long, value_name = "PATH", action = clap::ArgAction::Append)]
    attach: Vec<PathBuf>,
    /// Campaign id. Defaults to a timestamp; reuse it to resume a run.
    #[arg(long)]
    campaign_id: Option<String>,
    /// Where campaign manifests live.
    #[arg(long, default_value = ".mxr-mailmerge")]
    state_dir: PathBuf,
    /// Render and validate everything, create nothing.
    #[arg(long, conflicts_with = "yes")]
    dry_run: bool,
    /// Actually create the drafts.
    #[arg(long, conflicts_with = "dry_run")]
    yes: bool,
    /// Override the mxr binary (testing).
    #[arg(long, hide = true)]
    mxr_binary: Option<String>,
}

#[derive(clap::Args)]
struct SendArgs {
    campaign_id: String,
    #[arg(long, default_value = ".mxr-mailmerge")]
    state_dir: PathBuf,
    /// Show what would be sent, send nothing.
    #[arg(long, conflicts_with = "yes")]
    dry_run: bool,
    /// Confirm the send.
    #[arg(long, conflicts_with = "dry_run")]
    yes: bool,
    /// Send only records that previously failed.
    #[arg(long)]
    retry_failed: bool,
    #[arg(long, hide = true)]
    mxr_binary: Option<String>,
}

#[derive(clap::Args)]
struct StatusArgs {
    campaign_id: String,
    #[arg(long, default_value = ".mxr-mailmerge")]
    state_dir: PathBuf,
    /// Emit JSON instead of a table.
    #[arg(long)]
    json: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Draft(args) => run_draft(args),
        Command::Send(args) => run_send(args),
        Command::Status(args) => run_status(args),
    }
}

fn run_draft(args: DraftArgs) -> anyhow::Result<()> {
    let subject_source = read_template(&args.subject_template, "subject template")?;
    let html_source = read_template(&args.html_template, "HTML template")?;
    let text_source = args
        .text_template
        .as_ref()
        .map(|path| read_template(path, "text template"))
        .transpose()?;

    let mut templates = Templates::new();
    templates.add(template::SUBJECT, subject_source)?;
    templates.add(template::HTML, html_source)?;
    if let Some(source) = text_source {
        templates.add(template::TEXT, source)?;
    }

    let format = records::DataFormat::from_path(&args.data);
    let records = records::load_records(&args.data, format)?;

    let inline = parse_inline(&args.inline)?;
    for (_, path) in &inline {
        if !path.exists() {
            bail!("inline asset not found: {}", path.display());
        }
    }
    for attachment in &args.attach {
        if !attachment.exists() {
            bail!("attachment not found: {}", attachment.display());
        }
    }

    let campaign_id = args
        .campaign_id
        .clone()
        .unwrap_or_else(|| format!("campaign-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S")));

    let mut manifest = Manifest::load(&args.state_dir, &campaign_id)
        .unwrap_or_else(|_| Manifest::new(campaign_id.clone(), args.account.clone()));

    // Render everything before creating anything. A template error on record
    // 40 should not leave 39 drafts behind.
    let mut rendered = Vec::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        let position = index + 1;
        let subject = templates
            .render(template::SUBJECT, &record.properties)
            .with_context(|| format!("record {position} ({})", record.to))?;
        records::validate_subject(&subject)
            .with_context(|| format!("record {position} ({})", record.to))?;

        let html = templates
            .render(template::HTML, &record.properties)
            .with_context(|| format!("record {position} ({})", record.to))?;
        let text = if args.text_template.is_some() {
            Some(
                templates
                    .render(template::TEXT, &record.properties)
                    .with_context(|| format!("record {position} ({})", record.to))?,
            )
        } else {
            None
        };

        rendered.push((record, subject, html, text));
    }

    if args.dry_run {
        println!("Campaign {campaign_id} — dry run, nothing created");
        println!("  account:   {}", args.account);
        println!("  records:   {}", rendered.len());
        println!("  inline:    {}", inline.len());
        println!("  attach:    {}", args.attach.len());
        let already = rendered
            .iter()
            .filter(|(record, ..)| {
                manifest
                    .entry(&record.hash())
                    .is_some_and(|entry| entry.draft_id.is_some())
            })
            .count();
        if already > 0 {
            println!("  {already} record(s) already have drafts and would be skipped");
        }
        println!("\nWould create {} draft(s). Nothing was sent.", rendered.len() - already);
        for (record, subject, ..) in rendered.iter().take(5) {
            // Subject and address only — property values stay out of output.
            println!("  → {} — {subject}", record.to);
        }
        if rendered.len() > 5 {
            println!("  … and {} more", rendered.len() - 5);
        }
        return Ok(());
    }

    if !args.yes {
        bail!(
            "refusing to create {} draft(s) without --yes. Preview with --dry-run first.",
            rendered.len()
        );
    }

    let client = mxr::Mxr::new(args.mxr_binary.clone());
    client.preflight()?;

    let workdir = tempdir_for(&campaign_id)?;
    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for (record, subject, html, text) in rendered {
        let hash = record.hash();

        // Idempotency: a record that already produced a draft is never
        // drafted again, so rerunning after a partial failure cannot
        // duplicate anyone.
        if let Some(entry) = manifest.entry(&hash) {
            if entry.draft_id.is_some() {
                skipped += 1;
                continue;
            }
        }

        let html_path = workdir.join(format!("{hash}.html"));
        std::fs::write(&html_path, &html)?;
        let text_path = match &text {
            Some(body) => {
                let path = workdir.join(format!("{hash}.txt"));
                std::fs::write(&path, body)?;
                Some(path)
            }
            None => None,
        };

        let result = client.compose_draft(
            &args.account,
            &record.to,
            &subject,
            &html_path,
            text_path.as_deref(),
            &inline,
            &args.attach,
        );

        match result {
            Ok(draft) => {
                manifest.upsert(RecordEntry {
                    record_hash: hash,
                    to: record.to.clone(),
                    draft_id: Some(draft.draft_id),
                    status: RecordStatus::Drafted,
                    error: None,
                });
                created += 1;
            }
            Err(error) => {
                manifest.upsert(RecordEntry {
                    record_hash: hash,
                    to: record.to.clone(),
                    draft_id: None,
                    status: RecordStatus::Failed,
                    error: Some(error.to_string()),
                });
                failed += 1;
                eprintln!("draft failed for {}: {error}", record.to);
            }
        }

        // Saved after every record so an interrupted run still knows which
        // drafts exist.
        manifest.save(&args.state_dir)?;

        // Remove the rendered bodies as soon as mxr has them; they contain
        // personalised links.
        let _ = std::fs::remove_file(&html_path);
        if let Some(path) = text_path {
            let _ = std::fs::remove_file(path);
        }
    }

    let _ = std::fs::remove_dir(&workdir);

    println!("Campaign {campaign_id}");
    println!("  drafts created: {created}");
    if skipped > 0 {
        println!("  already existed: {skipped}");
    }
    if failed > 0 {
        println!("  failed:         {failed}");
    }
    println!("\nNothing was sent. Review with:");
    println!("  mxr drafts list --account {}", args.account);
    println!("Send with:");
    println!("  mxr-mailmerge send {campaign_id} --dry-run");
    Ok(())
}

fn run_send(args: SendArgs) -> anyhow::Result<()> {
    let mut manifest = Manifest::load(&args.state_dir, &args.campaign_id)?;

    let targets: Vec<(String, String)> = if args.retry_failed {
        manifest
            .failed()
            .into_iter()
            .filter_map(|entry| {
                entry
                    .draft_id
                    .as_ref()
                    .map(|id| (entry.record_hash.clone(), id.clone()))
            })
            .collect()
    } else {
        manifest
            .pending_send()
            .into_iter()
            .filter_map(|entry| {
                entry
                    .draft_id
                    .as_ref()
                    .map(|id| (entry.record_hash.clone(), id.clone()))
            })
            .collect()
    };

    if targets.is_empty() {
        println!("Campaign {} — nothing to send.", args.campaign_id);
        println!("  sent already: {}", manifest.count(RecordStatus::Sent));
        return Ok(());
    }

    if args.dry_run {
        println!("Campaign {} — dry run, nothing sent", args.campaign_id);
        println!("  would send: {} message(s)", targets.len());
        println!("  already sent: {}", manifest.count(RecordStatus::Sent));
        return Ok(());
    }

    if !args.yes {
        bail!(
            "refusing to send {} message(s) without --yes. This transmits real email. \
             Preview with --dry-run first.",
            targets.len()
        );
    }

    println!("Sending {} message(s)…", targets.len());

    let client = mxr::Mxr::new(args.mxr_binary.clone());
    client.preflight()?;

    let mut sent = 0usize;
    let mut failed = 0usize;

    for (hash, draft_id) in targets {
        let Some(entry) = manifest.entry(&hash).cloned() else {
            continue;
        };

        match client.send_draft(&draft_id) {
            Ok(()) => {
                manifest.upsert(RecordEntry {
                    status: RecordStatus::Sent,
                    error: None,
                    ..entry
                });
                sent += 1;
            }
            Err(error) => {
                manifest.upsert(RecordEntry {
                    status: RecordStatus::Failed,
                    error: Some(error.to_string()),
                    ..entry
                });
                failed += 1;
                eprintln!("send failed for draft {draft_id}: {error}");
            }
        }

        // Persisted per message: a crash mid-run must not re-send anyone.
        manifest.save(&args.state_dir)?;
    }

    println!("  sent:   {sent}");
    if failed > 0 {
        println!("  failed: {failed}");
        println!("Retry only the failures with:");
        println!(
            "  mxr-mailmerge send {} --retry-failed --yes",
            args.campaign_id
        );
    }
    Ok(())
}

fn run_status(args: StatusArgs) -> anyhow::Result<()> {
    let manifest = Manifest::load(&args.state_dir, &args.campaign_id)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }

    println!("Campaign {}", manifest.campaign_id);
    println!("  account:  {}", manifest.account);
    println!("  created:  {}", manifest.created_at);
    println!("  drafted:  {}", manifest.count(RecordStatus::Drafted));
    println!("  sent:     {}", manifest.count(RecordStatus::Sent));
    println!("  failed:   {}", manifest.count(RecordStatus::Failed));
    for entry in &manifest.records {
        let status = match entry.status {
            RecordStatus::Drafted => "drafted",
            RecordStatus::Sent => "sent",
            RecordStatus::Failed => "failed",
        };
        println!(
            "  {status:<8} {} {}",
            entry.to,
            entry.draft_id.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn read_template(path: &Path, label: &str) -> anyhow::Result<String> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("reading {label} {}", path.display()))?;
    template::reject_unsafe_constructs(&source, label)?;
    Ok(source)
}

fn parse_inline(raw: &[String]) -> anyhow::Result<Vec<(String, PathBuf)>> {
    raw.iter()
        .map(|entry| {
            let Some((cid, path)) = entry.split_once('=') else {
                bail!("--inline expects CID=PATH, got `{entry}`");
            };
            Ok((cid.trim().to_string(), PathBuf::from(path.trim())))
        })
        .collect()
}

fn tempdir_for(campaign_id: &str) -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("mxr-mailmerge-{campaign_id}"));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating work dir {}", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn draft_requires_yes_or_dry_run_to_be_meaningful() {
        // Neither flag => the run refuses rather than defaulting to acting.
        let cli = Cli::try_parse_from([
            "mxr-mailmerge",
            "draft",
            "--account",
            "notto",
            "--subject-template",
            "s.txt",
            "--html-template",
            "m.html",
            "--data",
            "r.json",
        ])
        .unwrap();
        match cli.command {
            Command::Draft(args) => {
                assert!(!args.yes);
                assert!(!args.dry_run);
            }
            _ => panic!("expected draft"),
        }
    }

    #[test]
    fn dry_run_and_yes_are_mutually_exclusive_on_both_commands() {
        assert!(Cli::try_parse_from([
            "mxr-mailmerge", "draft", "--account", "a", "--subject-template", "s",
            "--html-template", "h", "--data", "d", "--dry-run", "--yes",
        ])
        .is_err());
        assert!(
            Cli::try_parse_from(["mxr-mailmerge", "send", "c1", "--dry-run", "--yes"]).is_err()
        );
    }

    #[test]
    fn inline_pairs_parse() {
        let parsed = parse_inline(&["logo=/tmp/logo.png".to_string()]).unwrap();
        assert_eq!(parsed[0].0, "logo");
        assert_eq!(parsed[0].1, PathBuf::from("/tmp/logo.png"));
        assert!(parse_inline(&["logo".to_string()]).is_err());
    }
}
