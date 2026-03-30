#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::ipc_client::IpcClient;
use mxr_protocol::{Request, Response, ResponseData};
use regex::Regex;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULT_LOG_LINES: usize = 100;
const VERBOSE_LOG_LINES: usize = 500;
const GITHUB_ISSUE_URL: &str =
    "https://github.com/planetaryescape/mxr/issues/new?template=bug_report.yml";
const GITHUB_BODY_LIMIT: usize = 8_000;

#[derive(Debug, Clone)]
pub struct BugReportOptions {
    pub edit: bool,
    pub stdout: bool,
    pub clipboard: bool,
    pub github: bool,
    pub output: Option<PathBuf>,
    pub verbose: bool,
    pub full_logs: bool,
    pub no_sanitize: bool,
    pub since: Option<String>,
}

#[derive(Debug, Clone)]
struct BugReport {
    system_lines: Vec<String>,
    config_lines: Vec<String>,
    daemon_lines: Vec<String>,
    health_lines: Vec<String>,
    sync_history: Vec<String>,
    error_history: Vec<String>,
    log_lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct AccountSummary {
    name: String,
    sync: String,
    send: String,
    unread_count: u32,
    total_messages: u32,
}

#[derive(Debug, Clone)]
struct DaemonSummary {
    running: bool,
    uptime_secs: Option<u64>,
    total_messages: Option<u32>,
    accounts: Vec<String>,
}

pub async fn run(options: BugReportOptions) -> anyhow::Result<()> {
    let report = generate_report_markdown(&options).await?;

    if options.stdout {
        print!("{report}");
    }

    let output_path = if options.stdout
        && options.output.is_none()
        && !options.edit
        && !options.clipboard
        && !options.github
    {
        None
    } else {
        let path = options.output.clone().unwrap_or_else(default_report_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &report)?;
        Some(path)
    };

    if options.edit {
        let path = output_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("bug report path missing for --edit"))?;
        open_in_editor(path)?;
    }

    if options.clipboard {
        copy_to_clipboard(&report)?;
    }

    if options.github {
        let url = build_issue_url(&report);
        open_url(&url)?;
        if url == GITHUB_ISSUE_URL {
            if let Some(path) = output_path.as_ref() {
                eprintln!(
                    "Report saved to {} — paste it into the issue.",
                    path.display()
                );
            }
        }
    }

    if let Some(path) = output_path {
        println!("Bug report saved to {}", path.display());
    }

    Ok(())
}

pub async fn generate_report_markdown(options: &BugReportOptions) -> anyhow::Result<String> {
    let report = generate_report(options).await?;
    Ok(if options.no_sanitize {
        report
    } else {
        sanitize(&report)
    })
}

async fn generate_report(options: &BugReportOptions) -> anyhow::Result<String> {
    let config = mxr_config::load_config().unwrap_or_default();
    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let log_path = data_dir.join("logs").join("mxr.log");
    let index_path = data_dir.join("search_index");
    let socket_path = crate::state::AppState::socket_path();

    let store = if db_path.exists() {
        Some(mxr_store::Store::new(&db_path).await?)
    } else {
        None
    };

    let daemon = collect_daemon_summary(socket_path.exists()).await;
    let account_summaries = if let Some(store) = store.as_ref() {
        collect_account_summaries(store).await?
    } else {
        Vec::new()
    };
    let snoozed_count = if let Some(store) = store.as_ref() {
        store
            .list_snoozed()
            .await
            .map(|items| items.len())
            .unwrap_or(0)
    } else {
        0
    };
    let sync_events = if let Some(store) = store.as_ref() {
        store
            .list_events(10, None, Some("sync"))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let error_events = if let Some(store) = store.as_ref() {
        store
            .list_events(20, Some("error"), None)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let logs = read_recent_logs(
        &log_path,
        options.full_logs,
        options.verbose,
        options.since.as_deref(),
    )?;

    let report = BugReport {
        system_lines: vec![
            format!("- mxr version: {}", env!("CARGO_PKG_VERSION")),
            format!("- OS: {}", std::env::consts::OS),
            format!("- Architecture: {}", std::env::consts::ARCH),
            format!(
                "- Terminal: {} (TERM={})",
                std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unknown".to_string()),
                std::env::var("TERM").unwrap_or_else(|_| "unknown".to_string())
            ),
            format!(
                "- Shell: {}",
                std::env::var("SHELL").unwrap_or_else(|_| "unknown".to_string())
            ),
            format!("- $EDITOR: {}", resolve_editor(&config)),
        ],
        config_lines: build_config_lines(
            &config,
            &account_summaries,
            &db_path,
            &index_path,
            socket_path.exists(),
            snoozed_count,
        ),
        daemon_lines: build_daemon_lines(&daemon),
        health_lines: account_summaries
            .iter()
            .map(|account| {
                format!(
                    "- {}: {} unread, {} messages, sync={}, send={}",
                    account.name,
                    account.unread_count,
                    account.total_messages,
                    account.sync,
                    account.send
                )
            })
            .collect(),
        sync_history: sync_events.iter().map(render_event_line).collect(),
        error_history: error_events.iter().map(render_event_line).collect(),
        log_lines: logs,
    };

    Ok(render_report(&report))
}

fn build_config_lines(
    config: &mxr_config::MxrConfig,
    accounts: &[AccountSummary],
    db_path: &Path,
    index_path: &Path,
    daemon_running: bool,
    snoozed_count: usize,
) -> Vec<String> {
    let mut lines = vec![
        format!("- Accounts: {} configured", config.accounts.len()),
        format!("- Sync interval: {}s", config.general.sync_interval),
        format!("- Hook timeout: {}s", config.general.hook_timeout),
        format!("- Reader mode: {}", config.render.reader_mode),
        format!("- Logging level: {}", config.logging.level),
        format!("- Daemon socket present: {}", daemon_running),
        format!("- Search index size: {} bytes", path_size(index_path)),
        format!("- Store size: {} bytes", file_size(db_path)),
        format!("- Snoozed messages: {snoozed_count}"),
    ];
    for account in accounts {
        lines.push(format!(
            "  - {}: sync={}, send={}",
            account.name, account.sync, account.send
        ));
    }
    lines
}

async fn collect_daemon_summary(socket_exists: bool) -> DaemonSummary {
    if !socket_exists {
        return DaemonSummary {
            running: false,
            uptime_secs: None,
            total_messages: None,
            accounts: Vec::new(),
        };
    }

    match IpcClient::connect().await {
        Ok(mut client) => match client.request(Request::GetStatus).await {
            Ok(Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs,
                        accounts,
                        total_messages,
                        ..
                    },
            }) => DaemonSummary {
                running: true,
                uptime_secs: Some(uptime_secs),
                total_messages: Some(total_messages),
                accounts,
            },
            _ => DaemonSummary {
                running: true,
                uptime_secs: None,
                total_messages: None,
                accounts: Vec::new(),
            },
        },
        Err(_) => DaemonSummary {
            running: false,
            uptime_secs: None,
            total_messages: None,
            accounts: Vec::new(),
        },
    }
}

async fn collect_account_summaries(
    store: &mxr_store::Store,
) -> anyhow::Result<Vec<AccountSummary>> {
    let accounts = store.list_accounts().await?;
    let mut summaries = Vec::with_capacity(accounts.len());
    for account in accounts {
        let labels = store
            .list_labels_by_account(&account.id)
            .await
            .unwrap_or_default();
        let unread_count = labels
            .iter()
            .find(|label| label.name == "INBOX")
            .map(|label| label.unread_count)
            .unwrap_or(0);
        let total_messages = store
            .count_messages_by_account(&account.id)
            .await
            .unwrap_or(0);
        summaries.push(AccountSummary {
            name: account.name,
            sync: account
                .sync_backend
                .as_ref()
                .map(|backend| format!("{:?}", backend.provider_kind).to_ascii_lowercase())
                .unwrap_or_else(|| "none".to_string()),
            send: account
                .send_backend
                .as_ref()
                .map(|backend| format!("{:?}", backend.provider_kind).to_ascii_lowercase())
                .unwrap_or_else(|| "none".to_string()),
            unread_count,
            total_messages,
        });
    }
    Ok(summaries)
}

fn build_daemon_lines(daemon: &DaemonSummary) -> Vec<String> {
    vec![
        format!(
            "- Status: {}",
            if daemon.running { "running" } else { "stopped" }
        ),
        format!(
            "- Uptime: {}",
            daemon
                .uptime_secs
                .map(format_duration)
                .unwrap_or_else(|| "unknown".to_string())
        ),
        format!(
            "- Total messages: {}",
            daemon
                .total_messages
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        format!(
            "- Accounts: {}",
            if daemon.accounts.is_empty() {
                "unknown".to_string()
            } else {
                daemon.accounts.join(", ")
            }
        ),
    ]
}

fn render_report(report: &BugReport) -> String {
    let mut out = String::new();
    out.push_str("# mxr Bug Report\n\n");
    push_section(&mut out, "System", &report.system_lines);
    push_section(&mut out, "Configuration", &report.config_lines);
    push_section(&mut out, "Daemon Status", &report.daemon_lines);
    push_section(&mut out, "Account Health", &report.health_lines);
    push_section(&mut out, "Recent Sync History", &report.sync_history);
    push_section(&mut out, "Recent Errors", &report.error_history);
    out.push_str("## Recent Logs\n\n```text\n");
    for line in &report.log_lines {
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str("```\n\n");
    out.push_str("## User Description\n\n[Please describe the bug here]\n\n");
    out.push_str("## Steps to Reproduce\n\n[Please describe how to reproduce]\n\n");
    out.push_str("## Expected Behavior\n\n[What did you expect to happen?]\n\n");
    out.push_str("## Actual Behavior\n\n[What actually happened?]\n");
    out
}

fn push_section(out: &mut String, title: &str, lines: &[String]) {
    out.push_str("## ");
    out.push_str(title);
    out.push_str("\n\n");
    if lines.is_empty() {
        out.push_str("- none\n\n");
        return;
    }
    for line in lines {
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push('\n');
}

fn render_event_line(entry: &mxr_store::EventLogEntry) -> String {
    let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(entry.timestamp, 0)
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| entry.timestamp.to_string());
    format!(
        "- {} [{}:{}] {}",
        timestamp, entry.level, entry.category, entry.summary
    )
}

fn sanitize(input: &str) -> String {
    let mut output = input.to_string();

    let email_re = Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b")
        .expect("email redaction regex should compile");
    output = email_re
        .replace_all(&output, "[REDACTED_EMAIL]")
        .into_owned();

    let token_re = Regex::new(
        r#"(?im)^(\s*[- ]*(client_secret|token_ref|password_ref|access_token|refresh_token|api[_-]?key|authorization)\s*[:=]\s*).*$"#,
    )
    .expect("secret redaction regex should compile");
    output = token_re
        .replace_all(&output, "$1[REDACTED_SECRET]")
        .into_owned();

    let subject_re = Regex::new(r#"(?im)^(\s*[- ]*subject\s*[:=]\s*).*$"#)
        .expect("subject redaction regex should compile");
    output = subject_re
        .replace_all(&output, "$1[REDACTED_SUBJECT]")
        .into_owned();

    let body_re = Regex::new(r#"(?im)^(\s*[- ]*body(_text)?\s*[:=]\s*).*$"#)
        .expect("body redaction regex should compile");
    output = body_re
        .replace_all(&output, "$1[REDACTED_BODY]")
        .into_owned();

    let ip_re =
        Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("IP redaction regex should compile");
    output = ip_re.replace_all(&output, "[REDACTED_IP]").into_owned();

    if let Some(home) = dirs::home_dir() {
        let home = home.display().to_string();
        output = output.replace(&home, "~");
    }

    output
}

fn read_recent_logs(
    path: &Path,
    full_logs: bool,
    verbose: bool,
    since: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if !path.exists() {
        return Ok(vec!["(no log file found)".to_string()]);
    }

    let content = std::fs::read_to_string(path)?;
    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();

    if let Some(spec) = since {
        let cutoff = parse_since(spec)?;
        lines.retain(|line| line_timestamp(line).map(|ts| ts >= cutoff).unwrap_or(true));
    } else if full_logs {
        let today = chrono::Utc::now().date_naive();
        lines.retain(|line| {
            line_timestamp(line)
                .map(|ts| ts.date_naive() == today)
                .unwrap_or(true)
        });
    } else {
        let keep = if verbose {
            VERBOSE_LOG_LINES
        } else {
            DEFAULT_LOG_LINES
        };
        if lines.len() > keep {
            lines = lines.split_off(lines.len() - keep);
        }
    }

    if lines.is_empty() {
        return Ok(vec!["(no recent logs)".to_string()]);
    }

    Ok(lines)
}

fn parse_since(value: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    let (count, unit) = value
        .chars()
        .partition::<String, _>(|ch| ch.is_ascii_digit());
    let count = count.parse::<i64>()?;
    let duration = match unit.as_str() {
        "m" => chrono::Duration::minutes(count),
        "h" => chrono::Duration::hours(count),
        "d" => chrono::Duration::days(count),
        _ => anyhow::bail!("unsupported --since value: {value}"),
    };
    Ok(chrono::Utc::now() - duration)
}

fn line_timestamp(line: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let timestamp = line.split_whitespace().next()?;
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
}

fn resolve_editor(config: &mxr_config::MxrConfig) -> String {
    config
        .general
        .editor
        .clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "vi".to_string())
}

fn open_in_editor(path: &Path) -> anyhow::Result<()> {
    let config = mxr_config::load_config().unwrap_or_default();
    let editor = resolve_editor(&config);
    let quoted = path.display().to_string().replace('\'', r"'\''");
    let status = Command::new("sh")
        .arg("-lc")
        .arg(format!("{editor} '{quoted}'"))
        .status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("editor exited with status {status}")
    }
}

fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    for program in ["pbcopy", "wl-copy", "xclip"] {
        if which::which(program).is_err() {
            continue;
        }
        let mut command = Command::new(program);
        if program == "xclip" {
            command.args(["-selection", "clipboard"]);
        }
        let mut child = command.stdin(Stdio::piped()).spawn()?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if status.success() {
            return Ok(());
        }
    }
    anyhow::bail!("no supported clipboard command found")
}

fn build_issue_url(report: &str) -> String {
    let mut url = url::Url::parse(GITHUB_ISSUE_URL).expect("valid base issue url");
    url.query_pairs_mut().append_pair("body", report);
    if url.as_str().len() > GITHUB_BODY_LIMIT {
        return GITHUB_ISSUE_URL.to_string();
    }
    url.to_string()
}

fn open_url(url: &str) -> anyhow::Result<()> {
    for program in ["open", "xdg-open"] {
        if which::which(program).is_err() {
            continue;
        }
        let status = Command::new(program).arg(url).status()?;
        if status.success() {
            return Ok(());
        }
    }
    anyhow::bail!("no supported browser opener found")
}

fn default_report_path() -> PathBuf {
    let stamp = chrono::Utc::now().format("%Y-%m-%d");
    let suffix = uuid::Uuid::now_v7().to_string();
    std::env::temp_dir().join(format!("mxr-bug-report-{stamp}-{}.md", &suffix[..8]))
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn path_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    if path.is_file() {
        return file_size(path);
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| path_size(&entry.path()))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_redacts_common_sensitive_values() {
        let sanitized = sanitize(
            "email=user@example.com\npassword_ref=secret\nsubject: hi\nbody_text: hello\nip=10.0.0.1",
        );
        assert!(sanitized.contains("[REDACTED_EMAIL]"));
        assert!(sanitized.contains("[REDACTED_SECRET]"));
        assert!(sanitized.contains("[REDACTED_SUBJECT]"));
        assert!(sanitized.contains("[REDACTED_BODY]"));
        assert!(sanitized.contains("[REDACTED_IP]"));
    }

    #[test]
    fn parse_since_supports_hours() {
        let cutoff = parse_since("2h").unwrap();
        assert!(cutoff <= chrono::Utc::now() - chrono::Duration::hours(1));
    }

    #[test]
    fn build_issue_url_falls_back_for_large_reports() {
        let url = build_issue_url(&"x".repeat(GITHUB_BODY_LIMIT + 10));
        assert_eq!(url, GITHUB_ISSUE_URL);
    }

    #[test]
    fn render_report_includes_expected_sections() {
        let report = BugReport {
            system_lines: vec!["- one".to_string()],
            config_lines: vec!["- two".to_string()],
            daemon_lines: vec!["- three".to_string()],
            health_lines: vec!["- four".to_string()],
            sync_history: vec!["- five".to_string()],
            error_history: vec!["- six".to_string()],
            log_lines: vec!["log".to_string()],
        };
        let rendered = render_report(&report);
        assert!(rendered.contains("## System"));
        assert!(rendered.contains("## Configuration"));
        assert!(rendered.contains("## Recent Logs"));
    }
}
