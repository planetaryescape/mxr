pub mod cli;
pub mod commands;
#[doc(hidden)]
pub mod handler;
pub mod ipc_client;
pub(crate) mod loops;
pub mod output;
pub(crate) mod provider_credentials;
pub mod reindex;
pub mod server;
pub mod snooze;
#[doc(hidden)]
pub mod state;
#[cfg(test)]
pub(crate) mod test_fixtures;
pub mod unsubscribe;

use clap::Parser;
use cli::{unsupported_command_guidance, Cli, Command};

pub async fn run_cli(args: Vec<String>) -> anyhow::Result<()> {
    if let Some(guidance) = unsupported_command_guidance(&args) {
        anyhow::bail!(guidance);
    }
    let cli = Cli::parse_from(&args);

    let is_foreground = matches!(
        cli.command,
        Some(Command::Daemon {
            foreground: true,
            ..
        })
    );
    init_tracing(is_foreground)?;

    match cli.command {
        Some(Command::Daemon { .. }) => {
            crate::server::run_daemon().await?;
        }
        Some(Command::Restart) => {
            crate::server::restart_daemon().await?;
        }

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
            reindex_semantic,
            check,
            semantic_status,
            verbose,
            index_stats,
            store_stats,
            rebuild_analytics,
            refresh_contacts,
            format,
        }) => {
            commands::doctor::run(commands::doctor::DoctorRunOptions {
                reindex,
                reindex_semantic,
                check,
                semantic_status,
                verbose,
                index_stats,
                store_stats,
                rebuild_analytics,
                refresh_contacts,
                format,
            })
            .await?;
        }
        Some(Command::Logs {
            no_follow,
            level,
            since,
            purge,
            format,
        }) => {
            commands::logs::run(no_follow, level, since, purge, format)?;
        }
        Some(Command::Reset {
            hard,
            dry_run,
            including_config,
            yes_i_understand_this_destroys_local_state,
        }) => {
            commands::reset::run(commands::reset::ResetOptions {
                require_hard: true,
                hard,
                dry_run,
                including_config,
                yes_i_understand_this_destroys_local_state,
            })
            .await?;
        }
        Some(Command::Burn {
            dry_run,
            including_config,
            yes_i_understand_this_destroys_local_state,
        }) => {
            commands::reset::run(commands::reset::ResetOptions {
                require_hard: false,
                hard: true,
                dry_run,
                including_config,
                yes_i_understand_this_destroys_local_state,
            })
            .await?;
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
        Some(Command::Accounts { action, format }) => {
            commands::accounts::run(action, format).await?;
        }

        Some(Command::Search {
            query,
            format,
            limit,
            mode,
            sort,
            explain,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::search::run(query, format, limit, mode, sort, explain).await?;
        }
        Some(Command::Count {
            query,
            mode,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::count::run(query, mode, format).await?;
        }
        Some(Command::Cat {
            message_id,
            view,
            assets,
            raw,
            html,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::cat::run(message_id, view, assets, raw, html, format).await?;
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
        Some(Command::Headers { message_id, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::headers::run(message_id, format).await?;
        }
        Some(Command::Saved { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::saved::run(action, format).await?;
        }
        Some(Command::Semantic { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::semantic::run(action, format).await?;
        }
        Some(Command::Subscriptions {
            limit,
            rank,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::subscriptions::run(limit, rank, format).await?;
        }
        Some(Command::Storage {
            by,
            limit,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::storage::run(by, limit, account, format).await?;
        }
        Some(Command::Stale {
            mine,
            theirs,
            older_than_days,
            limit,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::stale::run(mine, theirs, older_than_days, limit, account, format).await?;
        }
        Some(Command::Contacts { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::contacts::run(action, format).await?;
        }
        Some(Command::ResponseTime {
            theirs,
            counterparty,
            since_days,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::response_time::run(theirs, counterparty, since_days, account, format).await?;
        }
        Some(Command::Sync {
            account,
            status,
            wait,
            wait_timeout_secs,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::sync_cmd::run(account, status, wait, wait_timeout_secs, format).await?;
        }
        Some(Command::Status { format, watch }) => {
            crate::server::ensure_daemon_running().await?;
            commands::status::run(format, watch).await?;
        }
        Some(Command::Web {
            host,
            port,
            print_url,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::web::run(host, port, print_url).await?;
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
            crate::server::ensure_daemon_running().await?;
            commands::mutations::compose(commands::mutations::ComposeOptions {
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
        Some(Command::Drafts { format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::drafts(format).await?;
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
        Some(Command::ReadArchive {
            message_id,
            search,
            yes,
            dry_run,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::read_archive(message_id, search, yes, dry_run).await?;
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
        Some(Command::Snoozed { format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::snoozed(format).await?;
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
            crate::server::ensure_daemon_supports_tui().await?;
            mxr_tui::run().await?;
        }
    }

    Ok(())
}

pub fn init_tracing(foreground: bool) -> anyhow::Result<()> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "mxr=info"
            .parse()
            .expect("static mxr tracing filter should parse")
    });

    let log_dir = state::AppState::data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("mxr.log"))?;

    let file_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(file)
        .with_ansi(false);

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
