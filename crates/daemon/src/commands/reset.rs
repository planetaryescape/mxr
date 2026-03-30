#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::server::{inspect_socket_state, shutdown_daemon_for_maintenance, SocketState};
use anyhow::Context;
use std::collections::BTreeSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CONFIRMATION_PHRASE: &str = "DELETE MY MXR DATA";
pub const NON_INTERACTIVE_OVERRIDE_FLAG: &str = "--yes-i-understand-this-destroys-local-state";

const SHUTDOWN_WAIT_TIMEOUT: Duration = Duration::from_secs(3);

pub struct ResetOptions {
    pub require_hard: bool,
    pub hard: bool,
    pub dry_run: bool,
    pub yes_i_understand_this_destroys_local_state: bool,
}

#[derive(Debug, Clone)]
struct ResetContext {
    data_dir: PathBuf,
    socket_path: PathBuf,
    config_path: PathBuf,
    attachment_dir: PathBuf,
    config_read_error: Option<String>,
}

#[derive(Debug, Clone)]
struct ResetPlan {
    data_dir: PathBuf,
    socket_path: PathBuf,
    config_path: PathBuf,
    daemon_state: SocketState,
    shutdown_required: bool,
    config_read_error: Option<String>,
    targets: Vec<ResetTarget>,
    preserved: Vec<String>,
}

#[derive(Debug, Clone)]
struct ResetTarget {
    label: String,
    path: PathBuf,
    kind: ResetTargetKind,
    exists: bool,
    size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetTargetKind {
    Socket,
    File,
    Directory,
    Other,
}

#[derive(Debug, Default)]
struct ResetExecutionSummary {
    removed: Vec<PathBuf>,
    already_absent: Vec<PathBuf>,
    failed: Vec<(PathBuf, String)>,
    removed_empty_data_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmationStrategy {
    None,
    PromptForPhrase,
}

pub async fn run(options: ResetOptions) -> anyhow::Result<()> {
    if options.require_hard && !options.hard {
        anyhow::bail!("`mxr reset` requires --hard");
    }

    let context = discover_reset_context();
    let plan = build_reset_plan(&context).await?;
    println!("{}", render_reset_plan(&plan));

    let strategy = confirmation_strategy(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        options.dry_run,
        options.yes_i_understand_this_destroys_local_state,
    )?;

    if options.dry_run {
        println!("Dry run only. No local state was deleted.");
        return Ok(());
    }

    if matches!(strategy, ConfirmationStrategy::PromptForPhrase) {
        prompt_for_confirmation_phrase()?;
    }

    let post_shutdown_state = if plan.shutdown_required {
        shutdown_daemon_for_maintenance(&plan.socket_path, SHUTDOWN_WAIT_TIMEOUT).await?
    } else {
        plan.daemon_state
    };

    if matches!(post_shutdown_state, SocketState::Reachable) {
        anyhow::bail!(
            "Daemon is still running at {}. Refusing to destroy live local state.",
            plan.socket_path.display()
        );
    }

    let summary = execute_reset_plan(&plan);
    print_execution_summary(&summary);

    if summary.failed.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("Reset completed with {} failure(s).", summary.failed.len())
    }
}

fn discover_reset_context() -> ResetContext {
    let data_dir = mxr_config::data_dir();
    let config_path = mxr_config::config_file_path();
    let socket_path = mxr_config::socket_path();

    match mxr_config::load_config() {
        Ok(config) => ResetContext {
            data_dir: data_dir.clone(),
            socket_path,
            config_path,
            attachment_dir: config.general.attachment_dir,
            config_read_error: None,
        },
        Err(error) => ResetContext {
            data_dir: data_dir.clone(),
            socket_path,
            config_path,
            attachment_dir: data_dir.join("attachments"),
            config_read_error: Some(error.to_string()),
        },
    }
}

async fn build_reset_plan(context: &ResetContext) -> anyhow::Result<ResetPlan> {
    let daemon_state = inspect_socket_state(&context.socket_path).await;
    let mut targets = Vec::new();
    let mut preserved = vec![
        format!("config file: {}", context.config_path.display()),
        "credentials/keychain: system credential stores are preserved".to_string(),
    ];
    let mut known_top_level_paths = BTreeSet::new();

    add_target(
        &mut targets,
        "socket",
        context.socket_path.clone(),
        ResetTargetKind::Socket,
    );

    let database_path = context.data_dir.join("mxr.db");
    add_runtime_target(
        &mut targets,
        &mut known_top_level_paths,
        &context.data_dir,
        "database",
        database_path,
        ResetTargetKind::File,
    );

    let index_path = context.data_dir.join("search_index");
    add_runtime_target(
        &mut targets,
        &mut known_top_level_paths,
        &context.data_dir,
        "lexical index",
        index_path,
        ResetTargetKind::Directory,
    );

    let model_cache_path = context.data_dir.join("models");
    add_runtime_target(
        &mut targets,
        &mut known_top_level_paths,
        &context.data_dir,
        "semantic model cache",
        model_cache_path,
        ResetTargetKind::Directory,
    );

    let attachments_path = context.attachment_dir.clone();
    if is_path_within_dir(&attachments_path, &context.data_dir) {
        add_runtime_target(
            &mut targets,
            &mut known_top_level_paths,
            &context.data_dir,
            "attachments",
            attachments_path,
            ResetTargetKind::Directory,
        );
    } else {
        preserved.push(format!(
            "attachment dir outside MXR_DATA_DIR: {}",
            attachments_path.display()
        ));
    }

    let logs_path = context.data_dir.join("logs");
    add_runtime_target(
        &mut targets,
        &mut known_top_level_paths,
        &context.data_dir,
        "logs",
        logs_path,
        ResetTargetKind::Directory,
    );

    let source_path = context.data_dir.join("source");
    add_runtime_target(
        &mut targets,
        &mut known_top_level_paths,
        &context.data_dir,
        "source temp artifacts",
        source_path,
        ResetTargetKind::Directory,
    );

    if context.data_dir.exists() {
        let mut extras = std::fs::read_dir(&context.data_dir)
            .with_context(|| format!("read {}", context.data_dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        extras.sort();

        for path in extras {
            if known_top_level_paths.contains(&path) {
                continue;
            }

            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let kind = classify_existing_path_kind(&path);
            add_target(&mut targets, format!("runtime data: {name}"), path, kind);
        }
    }

    Ok(ResetPlan {
        data_dir: context.data_dir.clone(),
        socket_path: context.socket_path.clone(),
        config_path: context.config_path.clone(),
        daemon_state,
        shutdown_required: matches!(daemon_state, SocketState::Reachable),
        config_read_error: context.config_read_error.clone(),
        targets,
        preserved,
    })
}

fn add_runtime_target(
    targets: &mut Vec<ResetTarget>,
    known_top_level_paths: &mut BTreeSet<PathBuf>,
    data_dir: &Path,
    label: impl Into<String>,
    path: PathBuf,
    kind: ResetTargetKind,
) {
    if path.parent() == Some(data_dir) {
        known_top_level_paths.insert(path.clone());
    }
    add_target(targets, label, path, kind);
}

fn add_target(
    targets: &mut Vec<ResetTarget>,
    label: impl Into<String>,
    path: PathBuf,
    kind: ResetTargetKind,
) {
    targets.push(ResetTarget {
        label: label.into(),
        exists: path.exists(),
        size_bytes: path_size(&path),
        path,
        kind,
    });
}

fn render_reset_plan(plan: &ResetPlan) -> String {
    let mut lines = vec![
        "mxr local-state reset plan".to_string(),
        "Destructive scope: local mxr runtime state only.".to_string(),
        "Preserved by default: config.toml and system credentials/keychain.".to_string(),
        format!("Data dir: {}", plan.data_dir.display()),
        format!("Socket path: {}", plan.socket_path.display()),
        format!("Config path: {}", plan.config_path.display()),
        format!("Daemon state: {}", daemon_state_label(plan.daemon_state)),
        format!(
            "Shutdown attempt: {}",
            if plan.shutdown_required { "yes" } else { "no" }
        ),
    ];

    if let Some(error) = &plan.config_read_error {
        lines.push(format!(
            "Config note: could not read config, preserving it untouched and using default attachment path under MXR_DATA_DIR ({error})"
        ));
    }

    lines.push(String::new());
    lines.push(format!("Paths to remove ({})", plan.targets.len()));
    for target in &plan.targets {
        lines.push(format!(
            "- {}: {} [{}; {}; {} bytes]",
            target.label,
            target.path.display(),
            target.kind.as_str(),
            if target.exists { "present" } else { "absent" },
            target.size_bytes
        ));
    }

    lines.push(String::new());
    lines.push(format!("Preserved ({})", plan.preserved.len()));
    for entry in &plan.preserved {
        lines.push(format!("- {entry}"));
    }

    lines.join("\n")
}

fn confirmation_strategy(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
    dry_run: bool,
    destructive_override: bool,
) -> anyhow::Result<ConfirmationStrategy> {
    if dry_run {
        return Ok(ConfirmationStrategy::None);
    }

    if stdin_is_terminal && stdout_is_terminal {
        return Ok(ConfirmationStrategy::PromptForPhrase);
    }

    if destructive_override {
        return Ok(ConfirmationStrategy::None);
    }

    anyhow::bail!(
        "Refusing destructive reset in non-interactive mode without {}. Re-run with --dry-run to preview.",
        NON_INTERACTIVE_OVERRIDE_FLAG
    )
}

fn prompt_for_confirmation_phrase() -> anyhow::Result<()> {
    print!("\nType {} to continue: ", CONFIRMATION_PHRASE);
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if confirmation_phrase_matches(input.trim_end_matches(['\n', '\r'])) {
        return Ok(());
    }

    anyhow::bail!("Aborted. Confirmation phrase did not match exactly.")
}

fn confirmation_phrase_matches(input: &str) -> bool {
    input == CONFIRMATION_PHRASE
}

fn execute_reset_plan(plan: &ResetPlan) -> ResetExecutionSummary {
    let mut summary = ResetExecutionSummary::default();
    let mut ordered_targets = plan.targets.iter().collect::<Vec<_>>();
    ordered_targets.sort_by_key(|target| std::cmp::Reverse(path_depth(&target.path)));

    for target in ordered_targets {
        match remove_path(&target.path) {
            Ok(RemovalOutcome::Removed) => summary.removed.push(target.path.clone()),
            Ok(RemovalOutcome::AlreadyAbsent) => {
                summary.already_absent.push(target.path.clone());
            }
            Err(error) => summary
                .failed
                .push((target.path.clone(), error.to_string())),
        }
    }

    match remove_dir_if_empty(&plan.data_dir) {
        Ok(true) => summary.removed_empty_data_dir = true,
        Ok(false) => {}
        Err(error) => summary
            .failed
            .push((plan.data_dir.clone(), error.to_string())),
    }

    summary
}

fn print_execution_summary(summary: &ResetExecutionSummary) {
    println!("\nReset summary");
    println!("- removed: {}", summary.removed.len());
    println!("- already absent: {}", summary.already_absent.len());
    println!("- failed: {}", summary.failed.len());

    if summary.removed_empty_data_dir {
        println!("- removed empty MXR_DATA_DIR");
    }

    if !summary.failed.is_empty() {
        println!("Failures:");
        for (path, error) in &summary.failed {
            println!("- {}: {}", path.display(), error);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemovalOutcome {
    Removed,
    AlreadyAbsent,
}

fn remove_path(path: &Path) -> anyhow::Result<RemovalOutcome> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RemovalOutcome::AlreadyAbsent);
        }
        Err(error) => return Err(error.into()),
    };

    if metadata.file_type().is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }

    Ok(RemovalOutcome::Removed)
}

fn remove_dir_if_empty(path: &Path) -> anyhow::Result<bool> {
    let mut entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };

    if entries.next().is_some() {
        return Ok(false);
    }

    std::fs::remove_dir(path)?;
    Ok(true)
}

fn classify_existing_path_kind(path: &Path) -> ResetTargetKind {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => ResetTargetKind::Directory,
        Ok(metadata) if metadata.file_type().is_file() => ResetTargetKind::File,
        Ok(_) => ResetTargetKind::Other,
        Err(_) => ResetTargetKind::Other,
    }
}

fn path_size(path: &Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => dir_size(path),
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    }
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let child_path = entry.path();
            match std::fs::symlink_metadata(&child_path) {
                Ok(metadata) if metadata.file_type().is_dir() => dir_size(&child_path),
                Ok(metadata) => metadata.len(),
                Err(_) => 0,
            }
        })
        .sum()
}

fn is_path_within_dir(path: &Path, dir: &Path) -> bool {
    path.is_absolute() && path.starts_with(dir)
}

fn path_depth(path: &Path) -> usize {
    path.components().count()
}

fn daemon_state_label(state: SocketState) -> &'static str {
    match state {
        SocketState::Reachable => "running",
        SocketState::Stale => "stale socket",
        SocketState::Missing => "missing",
    }
}

impl ResetTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Socket => "socket",
            Self::File => "file",
            Self::Directory => "directory",
            Self::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn target_path_set(plan: &ResetPlan) -> BTreeSet<PathBuf> {
        plan.targets
            .iter()
            .map(|target| target.path.clone())
            .collect()
    }

    #[tokio::test]
    async fn reset_plan_includes_known_runtime_targets() {
        let temp = TempDir::new().unwrap();
        let data_dir = temp.path().join("data");
        let config_path = temp.path().join("config").join("config.toml");
        let socket_path = temp.path().join("mxr.sock");
        let attachments_path = data_dir.join("attachments");
        let extra_path = data_dir.join("cache");

        std::fs::create_dir_all(data_dir.join("search_index")).unwrap();
        std::fs::create_dir_all(data_dir.join("models")).unwrap();
        std::fs::create_dir_all(data_dir.join("logs")).unwrap();
        std::fs::create_dir_all(data_dir.join("source")).unwrap();
        std::fs::create_dir_all(&attachments_path).unwrap();
        std::fs::create_dir_all(&extra_path).unwrap();
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(data_dir.join("mxr.db"), "db").unwrap();
        std::fs::write(data_dir.join("search_index").join("seg"), "idx").unwrap();
        std::fs::write(data_dir.join("models").join("model.bin"), "model").unwrap();
        std::fs::write(data_dir.join("logs").join("mxr.log"), "log").unwrap();
        std::fs::write(data_dir.join("source").join("message.txt"), "source").unwrap();
        std::fs::write(attachments_path.join("a.txt"), "attachment").unwrap();
        std::fs::write(extra_path.join("scratch.tmp"), "scratch").unwrap();
        std::fs::write(&socket_path, "").unwrap();

        let plan = build_reset_plan(&ResetContext {
            data_dir: data_dir.clone(),
            socket_path: socket_path.clone(),
            config_path: config_path.clone(),
            attachment_dir: attachments_path.clone(),
            config_read_error: None,
        })
        .await
        .unwrap();

        let targets = target_path_set(&plan);
        assert!(targets.contains(&socket_path));
        assert!(targets.contains(&data_dir.join("mxr.db")));
        assert!(targets.contains(&data_dir.join("search_index")));
        assert!(targets.contains(&data_dir.join("models")));
        assert!(targets.contains(&data_dir.join("logs")));
        assert!(targets.contains(&data_dir.join("source")));
        assert!(targets.contains(&attachments_path));
        assert!(targets.contains(&extra_path));
        assert_eq!(plan.daemon_state, SocketState::Stale);
        assert!(plan
            .preserved
            .iter()
            .any(|entry| entry.contains(&config_path.display().to_string())));
        assert!(plan
            .preserved
            .iter()
            .any(|entry| entry.contains("credentials/keychain")));
    }

    #[tokio::test]
    async fn reset_plan_preserves_external_attachment_dir() {
        let temp = TempDir::new().unwrap();
        let data_dir = temp.path().join("data");
        let config_path = temp.path().join("config.toml");
        let socket_path = temp.path().join("mxr.sock");
        let external_attachments = temp.path().join("external-attachments");

        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&external_attachments).unwrap();

        let plan = build_reset_plan(&ResetContext {
            data_dir: data_dir.clone(),
            socket_path,
            config_path,
            attachment_dir: external_attachments.clone(),
            config_read_error: None,
        })
        .await
        .unwrap();

        let targets = target_path_set(&plan);
        assert!(!targets.contains(&external_attachments));
        assert!(plan
            .preserved
            .iter()
            .any(|entry| entry.contains(&external_attachments.display().to_string())));
    }

    #[test]
    fn discover_reset_context_uses_default_attachment_dir_when_config_is_unreadable() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().join("config");
        let data_dir = temp.path().join("data");
        let socket_path = temp.path().join("runtime").join("mxr.sock");
        let config_path = config_dir.join("config.toml");

        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&config_path, "not valid toml = [").unwrap();

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir.as_os_str())),
                ("MXR_DATA_DIR", Some(data_dir.as_os_str())),
                ("MXR_SOCKET_PATH", Some(socket_path.as_os_str())),
            ],
            || {
                let context = discover_reset_context();
                assert_eq!(context.attachment_dir, data_dir.join("attachments"));
                assert!(context.config_read_error.is_some());
            },
        );
    }

    #[test]
    fn confirmation_policy_respects_tty_and_override_rules() {
        assert_eq!(
            confirmation_strategy(true, true, false, false).unwrap(),
            ConfirmationStrategy::PromptForPhrase
        );
        assert_eq!(
            confirmation_strategy(true, true, false, true).unwrap(),
            ConfirmationStrategy::PromptForPhrase
        );
        assert_eq!(
            confirmation_strategy(false, false, true, false).unwrap(),
            ConfirmationStrategy::None
        );
        assert_eq!(
            confirmation_strategy(false, false, false, true).unwrap(),
            ConfirmationStrategy::None
        );
        assert!(confirmation_strategy(false, false, false, false).is_err());
    }

    #[test]
    fn confirmation_phrase_must_match_exactly() {
        assert!(confirmation_phrase_matches(CONFIRMATION_PHRASE));
        assert!(!confirmation_phrase_matches("delete my mxr data"));
        assert!(!confirmation_phrase_matches("DELETE MY MXR DATA "));
    }

    #[tokio::test]
    async fn execute_reset_plan_is_idempotent_and_handles_missing_paths() {
        let temp = TempDir::new().unwrap();
        let data_dir = temp.path().join("data");
        let config_path = temp.path().join("config.toml");
        let socket_path = temp.path().join("mxr.sock");
        let attachments_path = data_dir.join("attachments");

        std::fs::create_dir_all(data_dir.join("search_index")).unwrap();
        std::fs::create_dir_all(&attachments_path).unwrap();
        std::fs::write(data_dir.join("mxr.db"), "db").unwrap();
        std::fs::write(socket_path.clone(), "").unwrap();
        std::fs::write(attachments_path.join("a.txt"), "attachment").unwrap();

        let plan = build_reset_plan(&ResetContext {
            data_dir: data_dir.clone(),
            socket_path: socket_path.clone(),
            config_path,
            attachment_dir: attachments_path,
            config_read_error: None,
        })
        .await
        .unwrap();

        let first = execute_reset_plan(&plan);
        assert!(first.failed.is_empty());
        assert!(first.removed.contains(&socket_path));
        assert!(first.removed_empty_data_dir);
        assert!(!data_dir.exists());

        let second = execute_reset_plan(&plan);
        assert!(second.failed.is_empty());
        assert!(!second.already_absent.is_empty());
    }
}
