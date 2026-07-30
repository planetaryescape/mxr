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
#[cfg(all(test, unix))]
mod stub;
mod template;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use manifest::{FailureReason, Manifest, RecordEntry, RecordStatus};
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

    let (campaign_id, mut manifest) = campaign_state(
        args.campaign_id.as_deref(),
        &default_campaign_id(),
        &args.state_dir,
        &args.account,
    )?;

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
        println!(
            "\nWould create {} draft(s). Nothing was sent.",
            rendered.len() - already
        );
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

    // Built from every record, not the one being drafted: mxr's stderr can
    // quote back a body rendered for somebody else.
    let redactor = records::Redactor::for_batch(&records);

    // A private (0700) directory that is removed when this binding drops, so an
    // early return cannot leave a personalised body behind in shared /tmp.
    let workdir = tempfile::Builder::new()
        .prefix("mxr-mailmerge-")
        .tempdir()
        .context("creating the work dir for rendered bodies")?;
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

        let html_path = workdir.path().join(format!("{hash}.html"));
        std::fs::write(&html_path, &html)?;
        let text_path = match &text {
            Some(body) => {
                let path = workdir.path().join(format!("{hash}.txt"));
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
                    // A fixed reason: mxr's own words stay out of a file that
                    // ends up in the working tree.
                    error: Some(FailureReason::DraftRefused),
                    status: RecordStatus::Failed,
                });
                failed += 1;
                eprintln!(
                    "draft failed for {}: {}",
                    record.to,
                    redactor.scrub(&error.to_string())
                );
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

    println!("Campaign {campaign_id}");
    println!("  drafts created: {created}");
    if skipped > 0 {
        println!("  already existed: {skipped}");
    }
    if failed > 0 {
        println!("  failed:         {failed}");
    }
    println!("\nNothing was sent. Review with:");
    // `--account`/`--format` belong to the parent `drafts` command; `list` is
    // the default action and rejects them.
    println!("  mxr drafts --account {}", args.account);
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
                    error: Some(FailureReason::SendRefused),
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
            "  {status:<8} {} {} {}",
            entry.to,
            entry.draft_id.as_deref().unwrap_or("-"),
            entry.error.map_or("", FailureReason::as_str)
        );
    }
    Ok(())
}

fn default_campaign_id() -> String {
    format!("campaign-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"))
}

/// Resolve which campaign this run belongs to, and the state it starts from.
///
/// An explicit `--campaign-id` means "resume that campaign": already-drafted
/// records are skipped instead of drafted twice. A generated id never means
/// that. Two runs started in the same second generate the same id, and silently
/// adopting the earlier run's manifest would put two campaigns' drafts behind
/// one `send`.
fn campaign_state(
    requested: Option<&str>,
    generated: &str,
    state_dir: &Path,
    account: &str,
) -> anyhow::Result<(String, Manifest)> {
    let campaign_id = requested.unwrap_or(generated).to_string();

    // A manifest that exists but cannot be read must stop the run. Starting a
    // fresh one would re-draft every record and then overwrite the only record
    // of the drafts that already exist.
    match Manifest::load_if_present(state_dir, &campaign_id)? {
        None => {
            let manifest = Manifest::new(campaign_id.clone(), account.to_string());
            Ok((campaign_id, manifest))
        }
        Some(_) if requested.is_none() => bail!(
            "campaign {campaign_id} already has a manifest, and this run did not ask to resume \
             it. Pass --campaign-id {campaign_id} to continue that campaign deliberately."
        ),
        Some(existing) if existing.account != account => bail!(
            "campaign {campaign_id} was drafted from {}, not {account}. Resuming it from another \
             account would send the rest of the campaign from the wrong address.",
            existing.account
        ),
        Some(existing) => Ok((campaign_id, existing)),
    }
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

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn neither_dry_run_nor_yes_means_the_command_does_not_act() {
        // Parsing must succeed — the refusal is a runtime one, with a count in
        // it — but neither flag may be set by default.
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
        let Command::Draft(args) = cli.command else {
            unreachable!("expected draft")
        };
        assert!(!args.yes);
        assert!(!args.dry_run);

        let cli = Cli::try_parse_from(["mxr-mailmerge", "send", "c1"]).unwrap();
        let Command::Send(args) = cli.command else {
            unreachable!("expected send")
        };
        assert!(!args.yes);
        assert!(!args.dry_run);
        assert!(!args.retry_failed);
    }

    #[test]
    fn dry_run_and_yes_are_mutually_exclusive_on_both_commands() {
        assert!(Cli::try_parse_from([
            "mxr-mailmerge",
            "draft",
            "--account",
            "a",
            "--subject-template",
            "s",
            "--html-template",
            "h",
            "--data",
            "d",
            "--dry-run",
            "--yes",
        ])
        .is_err());
        assert!(
            Cli::try_parse_from(["mxr-mailmerge", "send", "c1", "--dry-run", "--yes"]).is_err()
        );
        // Each flag alone still parses, or the conflict would be vacuous.
        assert!(Cli::try_parse_from(["mxr-mailmerge", "send", "c1", "--yes"]).is_ok());
        assert!(Cli::try_parse_from(["mxr-mailmerge", "send", "c1", "--dry-run"]).is_ok());
    }

    #[test]
    fn inline_pairs_parse() {
        let parsed = parse_inline(&[
            "logo=/tmp/logo.png".to_string(),
            " hero = /tmp/a=b.png ".to_string(),
        ])
        .unwrap();
        assert_eq!(parsed[0].0, "logo");
        assert_eq!(parsed[0].1, PathBuf::from("/tmp/logo.png"));
        // Split on the first `=` only: a path may contain one.
        assert_eq!(parsed[1].0, "hero");
        assert_eq!(parsed[1].1, PathBuf::from("/tmp/a=b.png"));

        assert!(parse_inline(&[]).unwrap().is_empty());
        let err = parse_inline(&["logo".to_string()]).unwrap_err();
        assert!(err.to_string().contains("CID=PATH"), "{err}");
    }

    #[test]
    fn an_explicit_campaign_id_resumes_that_campaign() {
        let dir = tempfile::tempdir().unwrap();
        let mut existing = Manifest::new("c1".into(), "notto@example.com".into());
        existing.upsert(RecordEntry {
            record_hash: "h1".into(),
            to: "a@example.com".into(),
            draft_id: Some("d1".into()),
            status: RecordStatus::Drafted,
            error: None,
        });
        existing.save(dir.path()).unwrap();

        let (id, manifest) =
            campaign_state(Some("c1"), "generated", dir.path(), "notto@example.com").unwrap();
        assert_eq!(id, "c1");
        assert_eq!(manifest.records.len(), 1, "the resume lost the drafts");

        // A fresh campaign under an id nobody has used starts empty.
        let (id, manifest) =
            campaign_state(Some("c2"), "generated", dir.path(), "notto@example.com").unwrap();
        assert_eq!(id, "c2");
        assert!(manifest.records.is_empty());
        assert_eq!(manifest.account, "notto@example.com");
    }

    #[test]
    fn a_generated_campaign_id_never_adopts_an_existing_manifest() {
        // Two runs started in the same second generate the same id. Adopting
        // the earlier run's manifest would merge two campaigns, and one `send`
        // would then transmit both.
        let dir = tempfile::tempdir().unwrap();
        Manifest::new("campaign-x".into(), "notto@example.com".into())
            .save(dir.path())
            .unwrap();

        let err = campaign_state(None, "campaign-x", dir.path(), "notto@example.com").unwrap_err();
        assert!(err.to_string().contains("did not ask to resume"), "{err}");
        assert!(err.to_string().contains("campaign-x"), "{err}");

        // Without a collision, a generated id is used as-is.
        let (id, _) = campaign_state(None, "campaign-y", dir.path(), "notto@example.com").unwrap();
        assert_eq!(id, "campaign-y");
    }

    #[test]
    fn a_campaign_is_only_resumed_from_the_account_that_started_it() {
        let dir = tempfile::tempdir().unwrap();
        Manifest::new("c1".into(), "notto@example.com".into())
            .save(dir.path())
            .unwrap();

        let err = campaign_state(Some("c1"), "generated", dir.path(), "other@example.com")
            .unwrap_err()
            .to_string();
        assert!(err.contains("notto@example.com"), "{err}");
        assert!(err.contains("other@example.com"), "{err}");
    }

    #[test]
    fn the_generated_campaign_id_says_what_it_is() {
        let id = default_campaign_id();
        assert!(id.starts_with("campaign-"), "{id}");
        // Second resolution only, so two runs in the same second collide — the
        // reason `campaign_state` refuses to adopt an existing manifest.
        assert_eq!(id.len(), "campaign-YYYYmmdd-HHMMSS".len(), "{id}");
    }

    #[cfg(unix)]
    mod end_to_end {
        use super::*;
        use crate::stub::Stub;
        use std::collections::BTreeSet;

        const TOKEN_A: &str = "opaque-token-aaaaaaaa";
        const TOKEN_B: &str = "opaque-token-bbbbbbbb";
        const ACCOUNT: &str = "notto@example.com";

        /// A campaign directory with two records carrying distinct tokens.
        struct Campaign {
            dir: tempfile::TempDir,
            stub: Stub,
        }

        impl Campaign {
            fn new(stub: Stub) -> Self {
                let dir = tempfile::tempdir().unwrap();
                std::fs::write(
                    dir.path().join("subject.txt"),
                    "Digest for {{ first_name }}",
                )
                .unwrap();
                std::fs::write(
                    dir.path().join("body.html"),
                    r#"<p>Hi {{ first_name }},</p><a href="{{ url }}">read</a>"#,
                )
                .unwrap();
                std::fs::write(
                    dir.path().join("records.json"),
                    format!(
                        r#"[{{"to":"a@example.com","first_name":"Dumi","url":"https://x.example/?t={TOKEN_A}"}},
                            {{"to":"b@example.com","first_name":"Sam","url":"https://x.example/?t={TOKEN_B}"}}]"#
                    ),
                )
                .unwrap();
                Self { dir, stub }
            }

            fn path(&self, name: &str) -> String {
                self.dir.path().join(name).display().to_string()
            }

            fn draft(&self, extra: &[&str]) -> anyhow::Result<()> {
                self.draft_as(ACCOUNT, extra)
            }

            fn draft_as(&self, account: &str, extra: &[&str]) -> anyhow::Result<()> {
                let mut argv = vec![
                    "draft".to_string(),
                    "--account".to_string(),
                    account.to_string(),
                    "--subject-template".to_string(),
                    self.path("subject.txt"),
                    "--html-template".to_string(),
                    self.path("body.html"),
                    "--data".to_string(),
                    self.path("records.json"),
                    "--campaign-id".to_string(),
                    "c1".to_string(),
                    "--state-dir".to_string(),
                    self.path("state"),
                    "--mxr-binary".to_string(),
                    self.stub.binary(),
                ];
                argv.extend(extra.iter().map(|arg| (*arg).to_string()));
                run(&argv)
            }

            fn send(&self, extra: &[&str]) -> anyhow::Result<()> {
                let mut argv = vec![
                    "send".to_string(),
                    "c1".to_string(),
                    "--state-dir".to_string(),
                    self.path("state"),
                    "--mxr-binary".to_string(),
                    self.stub.binary(),
                ];
                argv.extend(extra.iter().map(|arg| (*arg).to_string()));
                run(&argv)
            }

            fn manifest(&self) -> Manifest {
                Manifest::load(&self.dir.path().join("state"), "c1").unwrap()
            }
        }

        fn run(argv: &[String]) -> anyhow::Result<()> {
            let cli = Cli::try_parse_from(
                std::iter::once("mxr-mailmerge".to_string()).chain(argv.iter().cloned()),
            )?;
            match cli.command {
                Command::Draft(args) => run_draft(args),
                Command::Send(args) => run_send(args),
                Command::Status(args) => run_status(args),
            }
        }

        #[test]
        fn drafting_without_yes_refuses_and_touches_nothing() {
            let campaign = Campaign::new(Stub::new());
            let err = campaign.draft(&[]).unwrap_err();

            assert!(err.to_string().contains("without --yes"), "{err}");
            assert!(err.to_string().contains("2 draft(s)"), "{err}");
            assert!(campaign.stub.calls().is_empty(), "mxr was invoked anyway");
            assert!(!campaign.dir.path().join("state").exists());
        }

        #[test]
        fn a_dry_run_validates_everything_and_creates_nothing() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--dry-run"]).unwrap();

            assert!(campaign.stub.calls().is_empty(), "mxr was invoked anyway");
            assert!(
                !campaign.dir.path().join("state").exists(),
                "a dry run wrote campaign state"
            );
        }

        #[test]
        fn drafting_creates_one_draft_per_record_and_records_it() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let composes = campaign.stub.calls_to("compose");
            assert_eq!(composes.len(), 2);
            let addressed: BTreeSet<&str> = composes
                .iter()
                .filter_map(|argv| campaign.stub.value_after(argv, "--to"))
                .collect();
            assert_eq!(
                addressed,
                BTreeSet::from(["a@example.com", "b@example.com"])
            );
            assert_eq!(
                campaign.stub.value_after(&composes[0], "--subject"),
                Some("Digest for Dumi")
            );

            let manifest = campaign.manifest();
            assert_eq!(manifest.count(RecordStatus::Drafted), 2);
            assert_eq!(manifest.count(RecordStatus::Sent), 0);
            assert!(manifest
                .records
                .iter()
                .all(|entry| entry.draft_id.is_some()));
            // Drafting never sends.
            assert!(campaign.stub.calls_to("send").is_empty());
        }

        #[test]
        fn one_records_token_never_appears_in_another_records_body() {
            // The privacy property the whole feature turns on, checked on the
            // bytes mxr was actually handed.
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let bodies = campaign.stub.captured_bodies();
            assert_eq!(bodies.len(), 2);
            let for_a = bodies
                .iter()
                .find(|body| body.contains("Dumi"))
                .expect("no body for Dumi");
            let for_b = bodies
                .iter()
                .find(|body| body.contains("Sam"))
                .expect("no body for Sam");

            assert!(for_a.contains(TOKEN_A), "{for_a}");
            assert!(!for_a.contains(TOKEN_B), "Dumi got Sam's token: {for_a}");
            assert!(for_b.contains(TOKEN_B), "{for_b}");
            assert!(!for_b.contains(TOKEN_A), "Sam got Dumi's token: {for_b}");
        }

        #[test]
        fn rendered_bodies_are_deleted_once_mxr_has_them() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let handed_over: Vec<PathBuf> = campaign
                .stub
                .calls_to("compose")
                .iter()
                .filter_map(|argv| {
                    campaign
                        .stub
                        .value_after(argv, "--html-file")
                        .map(PathBuf::from)
                })
                .collect();
            assert_eq!(handed_over.len(), 2);
            for path in handed_over {
                assert!(!path.exists(), "personalised body left behind at {path:?}");
                assert!(
                    !path.parent().unwrap().exists(),
                    "work dir left behind at {:?}",
                    path.parent()
                );
            }
        }

        #[test]
        fn a_rendered_body_is_deleted_before_the_next_one_is_written() {
            // Stronger than "nothing is left when the run ends": across 500
            // records, every body that outlives its own compose is one more
            // personalised link sitting on disk for the rest of the run.
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let listings = campaign.stub.workdir_at_each_compose();
            assert_eq!(listings.len(), 2, "{listings:?}");
            for (index, files) in listings.iter().enumerate() {
                assert_eq!(
                    files.len(),
                    1,
                    "compose {} found an earlier record's body still on disk: {files:?}",
                    index + 1
                );
            }
        }

        #[test]
        fn a_rerun_skips_records_that_already_have_a_draft() {
            // The idempotency contract: rerunning after a partial failure must
            // not draft anyone twice.
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();
            assert_eq!(campaign.stub.calls_to("compose").len(), 2);

            campaign.draft(&["--yes"]).unwrap();
            assert_eq!(
                campaign.stub.calls_to("compose").len(),
                2,
                "the rerun drafted the same records again"
            );
            assert_eq!(campaign.manifest().records.len(), 2);
        }

        #[test]
        fn a_rerun_retries_a_record_whose_draft_failed() {
            let campaign = Campaign::new(Stub::compose_fails("mxr said no"));
            campaign.draft(&["--yes"]).unwrap();

            let manifest = campaign.manifest();
            assert_eq!(manifest.count(RecordStatus::Failed), 2);
            assert!(manifest
                .records
                .iter()
                .all(|entry| entry.draft_id.is_none()));
            // Nothing to send: a failed draft has no draft id to hand back.
            assert!(manifest.pending_send().is_empty());

            let retry = Campaign {
                dir: campaign.dir,
                stub: Stub::new(),
            };
            retry.draft(&["--yes"]).unwrap();
            assert_eq!(retry.stub.calls_to("compose").len(), 2);
            assert_eq!(retry.manifest().count(RecordStatus::Drafted), 2);
        }

        #[test]
        fn a_failed_draft_never_puts_a_property_value_in_the_manifest() {
            // mxr's stderr can quote the rendered body back, and a property may
            // be an opaque access token. Note whose token: the stub quotes
            // record A's body while failing both records, which is what a
            // per-record scrubber gets wrong.
            let campaign = Campaign::new(Stub::compose_fails(&format!(
                "refusing body: <a href=\"https://x.example/?t={TOKEN_A}\">read</a>"
            )));
            campaign.draft(&["--yes"]).unwrap();

            let raw = std::fs::read_to_string(Manifest::path_in(
                &campaign.dir.path().join("state"),
                "c1",
            ))
            .unwrap();
            assert!(
                !raw.contains(TOKEN_A),
                "token leaked into the manifest: {raw}"
            );
            assert!(!raw.contains(TOKEN_B), "token leaked: {raw}");
            // Not "the token happened to be scrubbed": none of mxr's output is
            // in the file at all.
            assert!(!raw.contains("refusing"), "subprocess text in state: {raw}");
            assert!(raw.contains("draft_refused"), "{raw}");
            assert_eq!(campaign.manifest().failed().len(), 2);
        }

        #[test]
        fn resuming_a_campaign_from_another_account_is_refused() {
            // The manifest would still name the first account while the drafts
            // went out from the second — a campaign sent from the wrong address.
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let err = campaign
                .draft_as("someone-else@example.com", &["--yes"])
                .unwrap_err();
            assert!(err.to_string().contains(ACCOUNT), "{err}");
            assert!(err.to_string().contains("someone-else"), "{err}");
            assert_eq!(campaign.manifest().account, ACCOUNT);
            // The second account never got a draft of its own.
            assert_eq!(campaign.stub.calls_to("compose").len(), 2);
        }

        #[test]
        fn an_unreadable_manifest_stops_the_run_instead_of_redrafting() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let path = Manifest::path_in(&campaign.dir.path().join("state"), "c1");
            std::fs::write(&path, "{ truncated").unwrap();

            let err = campaign.draft(&["--yes"]).unwrap_err();
            assert!(format!("{err:#}").contains("manifest"), "{err:#}");
            assert_eq!(
                campaign.stub.calls_to("compose").len(),
                2,
                "the run re-drafted every record over an unreadable manifest"
            );
        }

        #[test]
        fn a_template_error_on_a_later_record_creates_no_drafts_at_all() {
            let campaign = Campaign::new(Stub::new());
            std::fs::write(
                campaign.dir.path().join("body.html"),
                "<p>Hi {{ first_name }}, plan {{ plan }}</p>",
            )
            .unwrap();

            let err = campaign.draft(&["--yes"]).unwrap_err();
            assert!(format!("{err:#}").contains("plan"), "{err:#}");
            assert!(
                campaign.stub.calls().is_empty(),
                "drafts were created before the batch failed"
            );
        }

        #[test]
        fn sending_without_yes_refuses_and_sends_nothing() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            let err = campaign.send(&[]).unwrap_err();
            assert!(err.to_string().contains("without --yes"), "{err}");
            assert!(err.to_string().contains("2 message(s)"), "{err}");
            assert!(campaign.stub.calls_to("send").is_empty());
            assert_eq!(campaign.manifest().count(RecordStatus::Sent), 0);
        }

        #[test]
        fn a_send_dry_run_sends_nothing() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();
            campaign.send(&["--dry-run"]).unwrap();

            assert!(campaign.stub.calls_to("send").is_empty());
            assert_eq!(campaign.manifest().count(RecordStatus::Sent), 0);
        }

        #[test]
        fn a_confirmed_send_sends_each_draft_once_and_never_again() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();
            campaign.send(&["--yes"]).unwrap();

            let sends = campaign.stub.calls_to("send");
            assert_eq!(sends.len(), 2);
            let ids: BTreeSet<&str> = sends.iter().map(|argv| argv[1].as_str()).collect();
            assert_eq!(ids.len(), 2, "the same draft was sent twice: {sends:?}");
            assert_eq!(campaign.manifest().count(RecordStatus::Sent), 2);

            // Rerunning the send is the double-send this must never do.
            campaign.send(&["--yes"]).unwrap();
            assert_eq!(campaign.stub.calls_to("send").len(), 2);
            campaign.send(&["--retry-failed", "--yes"]).unwrap();
            assert_eq!(
                campaign.stub.calls_to("send").len(),
                2,
                "--retry-failed resent a message that was already sent"
            );
        }

        #[test]
        fn retry_failed_sends_only_the_failures() {
            let campaign = Campaign::new(Stub::new());
            campaign.draft(&["--yes"]).unwrap();

            // Mark one record sent and one failed, as a partial run would.
            let state = campaign.dir.path().join("state");
            let mut manifest = campaign.manifest();
            let hashes: Vec<String> = manifest
                .records
                .iter()
                .map(|entry| entry.record_hash.clone())
                .collect();
            let sent = manifest.entry(&hashes[0]).unwrap().clone();
            manifest.upsert(RecordEntry {
                status: RecordStatus::Sent,
                ..sent
            });
            let failed = manifest.entry(&hashes[1]).unwrap().clone();
            let failed_draft = failed.draft_id.clone().unwrap();
            manifest.upsert(RecordEntry {
                status: RecordStatus::Failed,
                error: Some(FailureReason::SendRefused),
                ..failed
            });
            manifest.save(&state).unwrap();

            let retry = Campaign {
                dir: campaign.dir,
                stub: Stub::new(),
            };
            retry.send(&["--retry-failed", "--yes"]).unwrap();

            let sends = retry.stub.calls_to("send");
            assert_eq!(sends.len(), 1, "{sends:?}");
            assert_eq!(sends[0][1], failed_draft);
            assert_eq!(retry.manifest().count(RecordStatus::Sent), 2);
        }

        #[test]
        fn a_failed_send_is_recorded_and_not_counted_as_sent() {
            let campaign = Campaign::new(Stub::send_fails("smtp said no"));
            campaign.draft(&["--yes"]).unwrap();
            campaign.send(&["--yes"]).unwrap();

            let manifest = campaign.manifest();
            assert_eq!(manifest.count(RecordStatus::Sent), 0);
            assert_eq!(manifest.failed().len(), 2);
            // The draft id survives so the retry has something to send.
            assert!(manifest
                .failed()
                .iter()
                .all(|entry| entry.draft_id.is_some()));
        }

        #[test]
        fn a_campaign_with_no_manifest_cannot_be_sent_or_inspected() {
            let campaign = Campaign::new(Stub::new());
            assert!(campaign.send(&["--yes"]).is_err());
            assert!(run(&[
                "status".to_string(),
                "c1".to_string(),
                "--state-dir".to_string(),
                campaign.path("state"),
            ])
            .is_err());
            assert!(campaign.stub.calls().is_empty());
        }

        #[test]
        fn a_missing_inline_asset_stops_the_run_before_anything_is_drafted() {
            let campaign = Campaign::new(Stub::new());
            let err = campaign
                .draft(&["--yes", "--inline", "logo=/nope/logo.png"])
                .unwrap_err();
            assert!(err.to_string().contains("inline asset not found"), "{err}");
            assert!(campaign.stub.calls().is_empty());

            let err = campaign
                .draft(&["--yes", "--attach", "/nope/brief.pdf"])
                .unwrap_err();
            assert!(err.to_string().contains("attachment not found"), "{err}");
        }

        #[test]
        fn a_template_that_loads_a_file_is_refused_before_any_record_is_read() {
            let campaign = Campaign::new(Stub::new());
            std::fs::write(
                campaign.dir.path().join("body.html"),
                "{% include '/etc/passwd' %}",
            )
            .unwrap();

            let err = campaign.draft(&["--dry-run"]).unwrap_err();
            assert!(err.to_string().contains("data templates"), "{err}");
            assert!(err.to_string().contains("HTML template"), "{err}");
        }

        #[test]
        fn a_rendered_subject_that_forges_a_header_fails_the_batch() {
            let campaign = Campaign::new(Stub::new());
            std::fs::write(
                campaign.dir.path().join("subject.txt"),
                "Digest\nBcc: evil@example.com",
            )
            .unwrap();

            let err = campaign.draft(&["--yes"]).unwrap_err();
            assert!(format!("{err:#}").contains("line break"), "{err:#}");
            assert!(campaign.stub.calls().is_empty());
        }
    }
}
