#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "unit tests unwrap daemon fixtures and JSON fields for direct failures"
    )
)]

#[doc(hidden)]
pub mod activity;
pub(crate) mod bridge;
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
use cli::{unsupported_command_guidance, Cli, Command, DemoAction};

pub async fn run_cli(args: Vec<String>) -> anyhow::Result<()> {
    if let Some(guidance) = unsupported_command_guidance(&args) {
        anyhow::bail!(guidance);
    }
    let cli = Cli::parse_from(&args);

    // Sticky demo: when `mxr demo` has been started in a prior invocation,
    // it leaves an active-marker behind so every subsequent CLI command
    // operates on the demo profile until `mxr demo stop`. Apply demo env
    // vars up front for any non-Demo command so the rest of dispatch sees
    // the demo paths transparently.
    //
    // The Demo command handles its own env setup below (Start re-seeds with
    // the requested message count; Stop/Status/Reset call apply_active_environment
    // themselves so they target the demo daemon, not the real one).
    let is_demo_command = matches!(cli.command, Some(Command::Demo { .. }));
    if !is_demo_command {
        commands::demo::apply_active_environment()?;
    }

    if let Some(Command::Demo {
        action,
        reset,
        messages,
        ..
    }) = &cli.command
    {
        // Stop/Status/Reset must run inside the demo env so they target the
        // demo profile rather than the user's real one. Start re-applies env
        // anyway via demo::run.
        if action.is_some() {
            commands::demo::apply_active_environment()?;
        } else {
            commands::demo::prepare_environment(*messages)?;
            if *reset {
                commands::demo::reset_profile().await?;
            }
        }
    }

    let is_foreground = matches!(
        cli.command,
        Some(Command::Daemon {
            foreground: true,
            ..
        })
    );
    init_tracing(is_foreground)?;

    match cli.command {
        Some(Command::Daemon {
            no_bridge,
            bridge_port,
            ..
        }) => {
            crate::server::run_daemon_with_overrides(crate::server::BridgeOverrides {
                disabled: no_bridge,
                port: bridge_port,
            })
            .await?;
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
            backfill_semantic,
            check,
            semantic_status,
            verbose,
            index_stats,
            store_stats,
            rebuild_analytics,
            refresh_contacts,
            recompute_link_counts,
            format,
        }) => {
            commands::doctor::run(commands::doctor::DoctorRunOptions {
                reindex,
                reindex_semantic,
                backfill_semantic,
                check,
                semantic_status,
                recompute_link_counts,
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
            search,
            limit,
            purge,
            format,
        }) => {
            commands::logs::run(commands::logs::LogRunOptions {
                no_follow,
                level,
                since,
                search,
                limit,
                purge,
                format,
            })?;
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
            crate::server::ensure_daemon_running().await?;
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
            search,
            first,
            limit,
            view,
            assets,
            raw,
            html,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::cat::run(commands::cat::CatRunOptions {
                message_id,
                search,
                first,
                limit,
                view,
                assets,
                raw,
                html,
                format,
            })
            .await?;
        }
        Some(Command::Thread {
            thread_id,
            search,
            first,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::thread::run(thread_id, search, first, limit, format).await?;
        }
        Some(Command::Threads {
            account,
            label,
            limit,
            offset,
            sort,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::threads::run(account, label, limit, offset, sort, format).await?;
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
        Some(Command::Headers {
            message_id,
            search,
            first,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::headers::run(message_id, search, first, limit, format).await?;
        }
        Some(Command::Saved { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::saved::run(action, format).await?;
        }
        Some(Command::Replies { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::replies::run(action, format).await?;
        }
        Some(Command::Snippets { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::snippets::run(action, format).await?;
        }
        Some(Command::Deliveries { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::deliveries::run(action, format).await?;
        }
        Some(Command::Signatures { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::signatures::run(action, format).await?;
        }
        Some(Command::Sender {
            email,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::sender::run(email, account, format).await?;
        }
        Some(Command::Profile {
            email,
            account,
            rebuild,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::profile::run(email, account, rebuild, format).await?;
        }
        Some(Command::Commitments {
            action,
            contact,
            status,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::commitments::run(action, contact, status, account, format).await?;
        }
        Some(Command::Owed {
            account,
            older_than_days,
            within_days,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::owed::run(account, older_than_days, within_days, limit, format).await?;
        }
        Some(Command::SuggestRecipients {
            draft,
            subject,
            body_stdin,
            account,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::suggest_recipients::run(draft, subject, body_stdin, account, limit, format)
                .await?;
        }
        Some(Command::Expert {
            message_id,
            query,
            include_self,
            account,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::expert::run(message_id, query, include_self, account, limit, format).await?;
        }
        Some(Command::Whois {
            query,
            account,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::whois::run(query, account, limit, format).await?;
        }
        Some(Command::Briefing { action }) => {
            crate::server::ensure_daemon_running().await?;
            commands::briefing::run(action).await?;
        }
        Some(Command::Cadence { action }) => {
            crate::server::ensure_daemon_running().await?;
            commands::cadence::run(action).await?;
        }
        Some(Command::SendTime {
            recipients,
            account,
            at,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::send_time::run(recipients, account, at, format).await?;
        }
        Some(Command::Decisions {
            action,
            account,
            topic,
            since_days,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::decisions::run(action, account, topic, since_days, limit, format).await?;
        }
        Some(Command::Ask {
            question,
            account,
            from,
            to,
            after,
            before,
            mode,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::ask::run(commands::ask::ArchiveAskRunOptions {
                question,
                account,
                from,
                to,
                after,
                before,
                mode,
                limit,
                format,
            })
            .await?;
        }
        Some(Command::Voice {
            action,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::voice::run(action, account, format).await?;
        }
        Some(Command::Humanize { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::humanize::run(action, format).await?;
        }
        Some(Command::Screener {
            action,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::screener::run(action, account, format).await?;
        }
        Some(Command::Setup { demo, key, force }) => {
            // Setup writes config; doesn't need a running daemon.
            commands::setup::run(demo, key, force).await?;
        }
        Some(Command::Demo {
            action,
            messages,
            no_tui,
            ..
        }) => match action {
            None => commands::demo::run(messages, no_tui).await?,
            Some(DemoAction::Stop) => commands::demo::stop().await?,
            Some(DemoAction::Status) => commands::demo::status()?,
            Some(DemoAction::Reset) => {
                commands::demo::reset_profile().await?;
                println!("Demo profile wiped. Run `mxr demo` to re-seed.");
            }
        },
        Some(Command::Summarize {
            thread_id,
            search,
            first,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::summarize::run(thread_id, search, first, limit, format).await?;
        }
        Some(Command::DraftAssist {
            thread_id,
            instruction,
            search,
            first,
            limit,
            instruct,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            // Either source of the instruction is fine; --instruct wins
            // when both are supplied so the long form is the canonical
            // override surface.
            let instruction_text = instruct.or(instruction).ok_or_else(|| {
                anyhow::anyhow!(
                    "Provide an instruction: a positional argument or `--instruct \"...\"`"
                )
            })?;
            commands::draft_assist::run(commands::draft_assist::DraftAssistRunOptions {
                thread_id,
                search,
                first,
                limit,
                instruction: instruction_text,
                format,
            })
            .await?;
        }
        Some(Command::Draft {
            action,
            to,
            purpose,
            account,
            register,
            length,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::draft::run(action, to, purpose, account, register, length, format).await?;
        }
        Some(Command::Remind {
            message_id,
            when,
            cancel,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::remind::run(message_id, when, cancel).await?;
        }
        Some(Command::Semantic { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::semantic::run(action, format).await?;
        }
        Some(Command::Llm { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::llm::run(action, format).await?;
        }
        Some(Command::Subscriptions {
            limit,
            rank,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::subscriptions::run(limit, rank, format).await?;
        }
        Some(Command::Senders { top, since, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::senders::run(top, since, format).await?;
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
            within_days,
            limit,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::stale::run(
                mine,
                theirs,
                older_than_days,
                within_days,
                limit,
                account,
                format,
            )
            .await?;
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
        Some(Command::Wrapped {
            ytd,
            year,
            since_days,
            account,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::wrapped::run(ytd, year, since_days, account, format).await?;
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
            action,
            host,
            port,
            print_url,
            no_open,
            auto_port,
            strict_port,
            remote_host,
            foreground,
            detached_child,
        }) => {
            if remote_host.is_none() && !matches!(action, Some(cli::WebAction::Stop)) {
                crate::server::ensure_daemon_running().await?;
            }
            commands::web::run(commands::web::Args {
                action,
                host,
                port,
                print_url,
                no_open,
                auto_port,
                strict_port,
                remote_host,
                foreground,
                detached_child,
            })
            .await?;
        }
        Some(Command::Events { event_type, format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::events::run(event_type, format).await?;
        }
        Some(Command::Activity { action }) => {
            crate::server::ensure_daemon_running().await?;
            commands::activity::run(action).await?;
        }
        Some(Command::History {
            category,
            category_prefix,
            level,
            search,
            since,
            until,
            offset,
            limit,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::history::run(commands::history::HistoryRunOptions {
                category,
                category_prefix,
                level,
                search,
                since,
                until,
                offset,
                limit,
                format,
            })
            .await?;
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
            signature,
            no_signature,
            yes,
            dry_run,
            format,
            check,
            no_llm,
        }) => {
            crate::server::ensure_daemon_running().await?;
            let options = commands::mutations::ComposeOptions {
                to,
                cc,
                bcc,
                subject,
                body,
                body_stdin,
                attach,
                from,
                signature,
                no_signature,
                yes,
                dry_run,
                format,
            };
            if check {
                commands::mutations::compose_check(options, no_llm).await?;
            } else {
                commands::mutations::compose(options).await?;
            }
        }
        Some(Command::Reply {
            message_id,
            body,
            body_stdin,
            signature,
            no_signature,
            yes,
            dry_run,
            remind_after,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::reply(commands::mutations::ReplyCommand {
                message_id,
                body,
                body_stdin,
                signature,
                no_signature,
                yes,
                dry_run,
                remind_after,
                format,
            })
            .await?;
        }
        Some(Command::ReplyAll {
            message_id,
            body,
            body_stdin,
            signature,
            no_signature,
            yes,
            dry_run,
            remind_after,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::reply_all(commands::mutations::ReplyCommand {
                message_id,
                body,
                body_stdin,
                signature,
                no_signature,
                yes,
                dry_run,
                remind_after,
                format,
            })
            .await?;
        }
        Some(Command::Forward {
            message_id,
            to,
            body,
            body_stdin,
            signature,
            no_signature,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::forward(commands::mutations::ForwardCommand {
                message_id,
                to,
                body,
                body_stdin,
                signature,
                no_signature,
                yes,
                dry_run,
                format,
            })
            .await?;
        }
        Some(Command::Drafts { action, format }) => {
            crate::server::ensure_daemon_running().await?;
            match action {
                None | Some(crate::cli::DraftsAction::List) => {
                    commands::mutations::drafts(format).await?;
                }
                Some(crate::cli::DraftsAction::Recover) => {
                    commands::mutations::drafts_recover(format).await?;
                }
                Some(crate::cli::DraftsAction::Resume { draft_id }) => {
                    commands::mutations::drafts_resume(draft_id).await?;
                }
                Some(crate::cli::DraftsAction::Discard { draft_id }) => {
                    commands::mutations::drafts_discard(draft_id).await?;
                }
            }
        }
        Some(Command::Send {
            draft_id,
            dry_run,
            format,
            at,
            remind_after,
            check,
            override_safety,
            no_llm,
        }) => {
            crate::server::ensure_daemon_running().await?;
            if check {
                commands::mutations::check_send(draft_id, format, no_llm).await?;
            } else if let Some(when) = at {
                commands::mutations::schedule_send(draft_id, when).await?;
            } else {
                commands::mutations::send_draft(
                    draft_id,
                    dry_run,
                    format,
                    override_safety,
                    remind_after,
                )
                .await?;
            }
        }
        Some(Command::Unsend { draft_id }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::cancel_scheduled_send(draft_id).await?;
        }
        Some(Command::Archive {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::archive(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Undo {
            mutation_id,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::undo(mutation_id, dry_run, format).await?;
        }
        Some(Command::ReadArchive {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::read_archive(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Trash {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::trash(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Spam {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::spam(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Star {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::star(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Unstar {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unstar(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::MarkRead {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::mark_read(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Unread {
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unread(message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Label {
            name,
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::label(name, message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Unlabel {
            name,
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unlabel(name, message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::MoveMsg {
            label,
            message_ids,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::move_msg(label, message_ids, search, yes, dry_run, format).await?;
        }
        Some(Command::Snooze {
            message_ids,
            until,
            search,
            yes,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::snooze(message_ids, until, search, yes, dry_run, format).await?;
        }
        Some(Command::Unsnooze {
            message_ids,
            all,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unsnooze(message_ids, all, dry_run, format).await?;
        }
        Some(Command::Snoozed { format }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::snoozed(format).await?;
        }
        Some(Command::Unsubscribe {
            message_ids,
            yes,
            search,
            dry_run,
            format,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::unsubscribe(message_ids, yes, search, dry_run, format).await?;
        }
        Some(Command::Open {
            message_id,
            search,
            first,
            limit,
            yes,
        }) => {
            crate::server::ensure_daemon_running().await?;
            commands::mutations::open_in_browser(message_id, search, first, limit, yes).await?;
        }
        Some(Command::Attachments { action }) => {
            crate::server::ensure_daemon_running().await?;
            match action {
                cli::AttachmentAction::List {
                    message_id,
                    search,
                    first,
                    limit,
                    format,
                } => {
                    commands::mutations::attachments_list(message_id, search, first, limit, format)
                        .await?;
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
        Some(Command::Invite { action }) => {
            crate::server::ensure_daemon_running().await?;
            match action {
                cli::InviteAction::Show { message_id, format } => {
                    commands::invites::show(message_id, format).await?;
                }
                cli::InviteAction::Reply {
                    message_id,
                    action,
                    dry_run,
                    format,
                } => {
                    commands::invites::reply(message_id, action.into(), dry_run, format).await?;
                }
            }
        }
        Some(Command::Invites { action }) => {
            crate::server::ensure_daemon_running().await?;
            match action {
                cli::InvitesAction::List { limit, format } => {
                    commands::invites::list(limit, format).await?;
                }
                cli::InvitesAction::Backfill { format } => {
                    commands::invites::backfill(format).await?;
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
