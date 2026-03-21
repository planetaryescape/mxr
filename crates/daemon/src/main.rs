mod cli;
mod commands;
mod handler;
mod ipc_client;
mod loops;
mod output;
pub mod reindex;
mod server;
pub mod snooze;
mod state;
pub mod unsubscribe;

use clap::Parser;
use cli::{unsupported_command_guidance, Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(guidance) = unsupported_command_guidance(&args) {
        anyhow::bail!(guidance);
    }
    let cli = Cli::parse_from(&args);

    let is_foreground = matches!(cli.command, Some(Command::Daemon { foreground: true }));
    init_tracing(is_foreground)?;

    match cli.command {
        Some(Command::Daemon { .. }) => {
            crate::server::run_daemon().await?;
        }

        // Local commands (no daemon needed)
        Some(Command::Version) => {
            commands::version::run();
        }
        Some(Command::Config { action }) => {
            commands::config::run(action)?;
        }
        Some(Command::Completions { shell }) => {
            commands::completions::run(shell)?;
        }
        Some(Command::Doctor {
            reindex,
            check,
            verbose,
            index_stats,
            store_stats,
            format,
        }) => {
            commands::doctor::run(reindex, check, verbose, index_stats, store_stats, format)
                .await?;
        }
        Some(Command::Logs {
            no_follow,
            level,
            since,
            purge,
        }) => {
            commands::logs::run(no_follow, level, since, purge)?;
        }
        Some(Command::BugReport {
            edit,
            stdout,
            clipboard,
            github,
            output,
            verbose,
            full_logs,
            no_sanitize,
            since,
        }) => {
            commands::bug_report::run(commands::bug_report::BugReportOptions {
                edit,
                stdout,
                clipboard,
                github,
                output,
                verbose,
                full_logs,
                no_sanitize,
                since,
            })
            .await?;
        }
        Some(Command::Accounts { action }) => {
            commands::accounts::run(action).await?;
        }

        // Daemon-backed commands
        Some(Command::Search {
            query,
            format,
            limit,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::search::run(query, format, limit).await?;
        }
        Some(Command::Count { query }) => {
            crate::server::ensure_daemon_running().await?;
            commands::count::run(query).await?;
        }
        Some(Command::Cat {
            message_id,
            raw,
            html,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::cat::run(message_id, raw, html, format).await?;
        }
        Some(Command::Thread { thread_id, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::thread::run(thread_id, format).await?;
        }
        Some(Command::Export {
            thread_id,
            search,
            format,
            output,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::export::run(thread_id, search, format, output).await?;
        }
        Some(Command::Headers { message_id }) => {
            crate::server::ensure_daemon_running().await?;
            commands::headers::run(message_id).await?;
        }
        Some(Command::Saved { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::saved::run(action, format).await?;
        }
        Some(Command::Subscriptions { limit, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::subscriptions::run(limit, format).await?;
        }
        Some(Command::Sync {
            account,
            status,
            history,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::sync_cmd::run(account, status, history).await?;
        }
        Some(Command::Status { format, watch }) => {
            crate::server::ensure_daemon_running().await?;
            commands::status::run(format, watch).await?;
        }
        Some(Command::Events { event_type, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::events::run(event_type, format).await?;
        }
        Some(Command::History {
            category,
            level,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::history::run(category, level, limit, format).await?;
        }
        Some(Command::Notify { format, watch }) => {
            crate::server::ensure_daemon_running().await?;
            commands::notify::run(format, watch).await?;
        }
        Some(Command::Labels { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::labels::run(action, format).await?;
        }
        Some(Command::Rules { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::rules::run(action, format).await?;
        }

        // Phase 2: Compose + mutations (daemon-backed)
        Some(Command::Compose {
            to,
            cc,
            bcc,
            subject,
            body,
            body_stdin,
            attach,
            from,
            yes,
            dry_run,
        }) => {
            let _ = yes;
            commands::mutations::compose(commands::mutations::ComposeOptions {
                to,
                cc,
                bcc,
                subject,
                body,
                body_stdin,
                attach,
                from,
                dry_run,
            })
            .await?;
        }
        Some(Command::Reply {
            message_id,
            body,
            body_stdin,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::reply(message_id, body, body_stdin, yes, dry_run).await?;
        }
        Some(Command::ReplyAll {
            message_id,
            body,
            body_stdin,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::reply_all(message_id, body, body_stdin, yes, dry_run).await?;
        }
        Some(Command::Forward {
            message_id,
            to,
            body,
            body_stdin,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::forward(message_id, to, body, body_stdin, yes, dry_run).await?;
        }
        Some(Command::Drafts) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::drafts().await?;
        }
        Some(Command::Send { draft_id }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::send_draft(draft_id).await?;
        }
        Some(Command::Archive {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::archive(message_id, search, yes, dry_run).await?;
        }
        Some(Command::Trash {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::trash(message_id, search, yes, dry_run).await?;
        }
        Some(Command::Spam {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::spam(message_id, search, yes, dry_run).await?;
        }
        Some(Command::Star {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::star(message_id, search, yes, dry_run).await?;
        }
        Some(Command::Unstar {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unstar(message_id, search, yes, dry_run).await?;
        }
        Some(Command::MarkRead {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::mark_read(message_id, search, yes, dry_run).await?;
        }
        Some(Command::Unread {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unread(message_id, search, yes, dry_run).await?;
        }
        Some(Command::Label {
            name,
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::label(name, message_id, search, yes, dry_run).await?;
        }
        Some(Command::Unlabel {
            name,
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unlabel(name, message_id, search, yes, dry_run).await?;
        }
        Some(Command::MoveMsg {
            label,
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::move_msg(label, message_id, search, yes, dry_run).await?;
        }
        Some(Command::Snooze {
            message_id,
            until,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::snooze(message_id, until, search, yes, dry_run).await?;
        }
        Some(Command::Unsnooze { message_id, all }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unsnooze(message_id, all).await?;
        }
        Some(Command::Snoozed) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::snoozed().await?;
        }
        Some(Command::Unsubscribe {
            message_id,
            yes,
            search,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unsubscribe(message_id, yes, search, dry_run).await?;
        }
        Some(Command::Open { message_id }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::open_in_browser(message_id).await?;
        }
        Some(Command::Attachments { action }) => {
            crate::server::ensure_daemon_running().await?;
            match action {
                cli::AttachmentAction::List { message_id } => {
                    commands::mutations::attachments_list(message_id).await?;
                }
                cli::AttachmentAction::Download {
                    message_id,
                    index,
                    dir,
                } => {
                    commands::mutations::attachments_download(message_id, index, dir).await?;
                }
                cli::AttachmentAction::OpenAttachment { message_id, index } => {
                    commands::mutations::attachments_open(message_id, index).await?;
                }
            }
        }

        None => {
            crate::server::ensure_daemon_running().await?;
            mxr_tui::run().await?;
        }
    }

    Ok(())
}

fn init_tracing(foreground: bool) -> anyhow::Result<()> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "mxr=info".parse().unwrap());

    let log_dir = state::AppState::data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("mxr.log"))?;

    let file_layer = fmt::layer().with_writer(file).with_ansi(false);

    if foreground {
        let stdout_layer = fmt::layer().with_writer(std::io::stdout);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(stdout_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .init();
    }

    Ok(())
}
