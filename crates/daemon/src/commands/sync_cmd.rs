use crate::cli::OutputFormat;
use crate::commands::progress::{format_thousands, ProgressPrinter};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_config::{load_config, AccountConfig, MxrConfig, SendProviderConfig, SyncProviderConfig};
use mxr_core::id::AccountId;
use mxr_protocol::{AccountSyncStatus, Request, Response, ResponseData, IPC_PROTOCOL_VERSION};
use std::time::{Duration, Instant};

fn render_sync_status(sync_statuses: &[mxr_protocol::AccountSyncStatus], protocol_version: u32) {
    if sync_statuses.is_empty() {
        if protocol_version < IPC_PROTOCOL_VERSION {
            println!("Sync status unavailable from legacy daemon");
        } else {
            println!("No sync-capable accounts");
        }
        return;
    }

    for sync in sync_statuses {
        println!("Account: {}", sync.account_name);
        println!(
            "  Healthy: {}  In progress: {}  Failures: {}",
            sync.healthy, sync.sync_in_progress, sync.consecutive_failures
        );
        println!(
            "  Last success: {}",
            sync.last_success_at.as_deref().unwrap_or("never")
        );
        println!(
            "  Last attempt: {}",
            sync.last_attempt_at.as_deref().unwrap_or("never")
        );
        println!(
            "  Last error: {}",
            sync.last_error.as_deref().unwrap_or("-")
        );
        println!(
            "  Backoff until: {}",
            sync.backoff_until.as_deref().unwrap_or("-")
        );
        println!(
            "  Cursor: {}",
            sync.current_cursor_summary.as_deref().unwrap_or("-")
        );
        println!("  Last synced count: {}", sync.last_synced_count);
    }
}

fn account_id_from_config(account: &AccountConfig) -> AccountId {
    let provider = match (&account.sync, &account.send) {
        (Some(SyncProviderConfig::Gmail { .. }), _) => "gmail",
        (Some(SyncProviderConfig::Imap { .. }), _) => "imap",
        (Some(SyncProviderConfig::OutlookPersonal { .. }), _) => "outlook",
        (Some(SyncProviderConfig::OutlookWork { .. }), _) => "outlook-work",
        (Some(SyncProviderConfig::Fake), _) => "fake",
        (None, Some(SendProviderConfig::Gmail)) => "gmail",
        (None, Some(SendProviderConfig::Smtp { .. })) => "smtp",
        (None, Some(SendProviderConfig::OutlookPersonal { .. })) => "outlook",
        (None, Some(SendProviderConfig::OutlookWork { .. })) => "outlook-work",
        (None, Some(SendProviderConfig::Fake)) => "fake",
        (None, None) => "account",
    };
    AccountId::from_provider_id(provider, &account.email)
}

fn resolve_account_selection(config: &MxrConfig, selector: &str) -> anyhow::Result<AccountId> {
    if let Some(account) = config.accounts.get(selector) {
        return Ok(account_id_from_config(account));
    }

    let matches = config
        .accounts
        .iter()
        .filter(|(_, account)| account.name == selector || account.email == selector)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [(_, account)] => Ok(account_id_from_config(account)),
        [] => anyhow::bail!("Account '{selector}' not found"),
        _ => anyhow::bail!(
            "Account selector '{selector}' is ambiguous. Use the config key from `mxr accounts`."
        ),
    }
}

fn resolve_account_id(selector: Option<&str>) -> anyhow::Result<Option<AccountId>> {
    let Some(selector) = selector else {
        return Ok(None);
    };
    let config = load_config().unwrap_or_default();
    resolve_account_selection(&config, selector).map(Some)
}

fn build_status_request(account_id: Option<&AccountId>) -> Request {
    match account_id {
        Some(account_id) => Request::GetSyncStatus {
            account_id: account_id.clone(),
        },
        None => Request::GetStatus,
    }
}

fn build_sync_request(account_id: Option<AccountId>) -> Request {
    Request::SyncNow {
        account_id,
        background: true,
    }
}

pub async fn run(
    account: Option<String>,
    status: bool,
    wait: bool,
    wait_timeout_secs: u64,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account_id(account.as_deref())?;

    if status {
        let statuses = fetch_sync_statuses(&mut client, account_id.as_ref()).await?;
        render_status(&statuses, format);
        return Ok(());
    }

    // The daemon acks as soon as the sync has started, so the trigger itself is
    // quick. It still goes over `request_with_events` for the events the daemon
    // emits before it answers — the "starting sync" line — and nothing after
    // that: the printer is finished the moment the ack lands, and `--wait`
    // below renders its own status polls.
    let json_mode = matches!(
        resolve_format(format.clone()),
        OutputFormat::Json | OutputFormat::Jsonl
    );
    // Taken before the trigger: with the daemon acking up front, a sync that
    // fails leaves the account idle-with-an-error, which is indistinguishable
    // from an account that was already carrying an old error. The comparison
    // is what makes `--wait` able to exit non-zero on a sync it started.
    //
    // `None` means we never got a reading. An empty baseline would be worse
    // than none: every pre-existing error would then look new, and `mxr sync`
    // would fail over something it did not cause.
    let before = if wait {
        match fetch_sync_statuses(&mut client, account_id.as_ref()).await {
            Ok(statuses) => Some(statuses),
            Err(error) => {
                tracing::debug!(%error, "could not read sync status before triggering");
                None
            }
        }
    } else {
        None
    };
    let progress = ProgressPrinter::new(json_mode);
    let resp = client
        .request_with_events(
            build_sync_request(account_id.clone()),
            progress.event_callback(),
        )
        .await;
    progress.finish();
    match resp? {
        Response::Ok {
            data: ResponseData::Ack,
        } => match (json_mode, wait) {
            // With --wait the final status is printed after quiescence, so
            // don't also emit a "triggered" line ahead of it.
            (true, false) => println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "status": "triggered",
                    "account_id": account_id.as_ref().map(std::string::ToString::to_string),
                }))?
            ),
            (true, true) => {}
            (false, _) => println!("Sync triggered"),
        },
        // The daemon rejects a sync it could not start at all — an unknown
        // account, no provider. Once started, failures show up on the status
        // row instead, which is what `--wait` reports.
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }

    // Starting is not finishing: a first backfill runs page after page, and
    // the account's own sync loop may still have queued work. `--wait` is what
    // turns "a sync started" into "the account is idle".
    if wait {
        let waited = wait_for_sync_quiescence(
            &mut client,
            account_id.as_ref(),
            Duration::from_secs(wait_timeout_secs),
            json_mode,
        )
        .await;
        // The status is what a JSON caller asked for, and it is most worth
        // having when the sync failed — so render it either way, then report.
        if json_mode {
            if let Ok(statuses) = fetch_sync_statuses(&mut client, account_id.as_ref()).await {
                render_status(&statuses, format);
            }
        }
        let after = waited?;
        if let Some(before) = before.as_deref() {
            let failures = sync_failures(before, &after);
            if !failures.is_empty() {
                anyhow::bail!("{}", failures.join("; "));
            }
        }
    }
    Ok(())
}

/// Errors left by a sync that ran while we were waiting.
///
/// An account that is idle is not an account that succeeded: a failed sync
/// clears `sync_in_progress` on its way out, so quiescence alone would have
/// `mxr sync --wait` exit 0 on a sync that failed. An error only counts when
/// the attempt carrying it is newer than the one we saw before triggering —
/// otherwise every wait would fail on an error the account has been carrying
/// since yesterday.
fn sync_failures(before: &[AccountSyncStatus], after: &[AccountSyncStatus]) -> Vec<String> {
    after
        .iter()
        .filter_map(|status| {
            let error = status.last_error.as_deref()?;
            let previous = before
                .iter()
                .find(|previous| previous.account_id == status.account_id);
            let is_new = previous.is_none_or(|previous| {
                previous.last_attempt_at != status.last_attempt_at
                    || previous.last_error.as_deref() != Some(error)
            });
            is_new.then(|| format!("sync failed for {}: {error}", status.account_name))
        })
        .collect()
}

async fn fetch_sync_statuses(
    client: &mut IpcClient,
    account_id: Option<&AccountId>,
) -> anyhow::Result<Vec<AccountSyncStatus>> {
    let resp = client.request(build_status_request(account_id)).await?;
    match (account_id, resp) {
        (
            Some(_),
            Response::Ok {
                data: ResponseData::SyncStatus { sync },
            },
        ) => Ok(vec![sync]),
        (
            None,
            Response::Ok {
                data: ResponseData::Status { sync_statuses, .. },
            },
        ) => Ok(sync_statuses),
        (_, Response::Error { message, .. }) => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

fn render_status(statuses: &[AccountSyncStatus], format: Option<OutputFormat>) {
    match resolve_format(format) {
        OutputFormat::Json => {
            let _ = serde_json::to_string_pretty(&statuses).map(|out| println!("{out}"));
        }
        OutputFormat::Jsonl => {
            let _ = jsonl(statuses).map(|out| println!("{out}"));
        }
        _ => render_sync_status(statuses, IPC_PROTOCOL_VERSION),
    }
}

async fn wait_for_sync_quiescence(
    client: &mut IpcClient,
    account_id: Option<&AccountId>,
    timeout: Duration,
    json_mode: bool,
) -> anyhow::Result<Vec<AccountSyncStatus>> {
    let progress = ProgressPrinter::new(json_mode);
    let deadline = Instant::now() + timeout;
    let mut last_line: Option<String> = None;
    loop {
        // Four outcomes, four responses: the daemon rejecting the query is
        // fatal, a dropped connection earns a reconnect, a degraded snapshot
        // is no reading at all, and a status read that simply stalled while
        // the daemon is busy with post-sync work says nothing about the sync —
        // keep polling until the deadline.
        match poll_sync_statuses(client, account_id).await {
            StatusPoll::Statuses(statuses) => {
                if !statuses.iter().any(|status| status.sync_in_progress) {
                    return Ok(statuses);
                }
                // Only when the line actually changes: the poll runs ten times
                // a second and piped output gets one line per call, so an
                // unthrottled note buries a five-minute backfill in thousands
                // of identical lines.
                let line = sync_progress_line(&statuses);
                if line.is_some() && line != last_line {
                    if let Some(line) = line.as_deref() {
                        progress.note(line);
                    }
                    last_line = line;
                }
            }
            StatusPoll::Degraded => {}
            StatusPoll::Rejected(message) => anyhow::bail!("{message}"),
            StatusPoll::Disconnected(error) => {
                *client = reconnect_client(&error).await?;
            }
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {}s waiting for sync to quiesce",
                timeout.as_secs()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Render the live progress of whichever accounts are mid-sync, so `--wait`
/// shows movement instead of a bare spinner.
fn sync_progress_line(statuses: &[AccountSyncStatus]) -> Option<String> {
    let parts = statuses
        .iter()
        .filter_map(|status| {
            let progress = status.progress.as_ref()?;
            let total = progress
                .total
                .map_or_else(String::new, |total| format!("/{}", format_thousands(total)));
            Some(format!(
                "{}: {}{total} — {}",
                status.account_name,
                format_thousands(progress.current),
                progress.message
            ))
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// One sync-status poll, keeping "the daemon said no" apart from "the
/// connection went away" and from "the daemon answered, but with nothing".
enum StatusPoll {
    Statuses(Vec<AccountSyncStatus>),
    /// `GetStatus` fast-fails its DB-backed snapshot when the reader pool is
    /// saturated — which is exactly what a large sync does to it — and reports
    /// zero accounts and no statuses rather than flagging itself on the wire.
    /// Reading that as "no account is syncing" would end the wait on the
    /// daemon's busiest moment, so treat it as no reading at all.
    Degraded,
    Rejected(String),
    Disconnected(anyhow::Error),
}

async fn poll_sync_statuses(client: &mut IpcClient, account_id: Option<&AccountId>) -> StatusPoll {
    match client.request(build_status_request(account_id)).await {
        Ok(resp) => match (account_id, resp) {
            (
                Some(_),
                Response::Ok {
                    data: ResponseData::SyncStatus { sync },
                },
            ) => StatusPoll::Statuses(vec![sync]),
            (
                None,
                Response::Ok {
                    data:
                        ResponseData::Status {
                            sync_statuses,
                            accounts,
                            ..
                        },
                },
            ) => {
                // A daemon with no accounts configured still names one
                // ("unknown"), so an empty account list can only mean the
                // snapshot degraded.
                if accounts.is_empty() {
                    StatusPoll::Degraded
                } else {
                    StatusPoll::Statuses(sync_statuses)
                }
            }
            (_, Response::Error { message, .. }) => StatusPoll::Rejected(message),
            _ => StatusPoll::Rejected("Unexpected response from daemon".to_string()),
        },
        Err(error) => StatusPoll::Disconnected(error),
    }
}

/// Reconnect after a dropped connection, giving up rather than hammering a
/// daemon that is gone for good.
async fn reconnect_client(cause: &anyhow::Error) -> anyhow::Result<IpcClient> {
    let mut last_error = None;
    for _ in 0..5 {
        match IpcClient::connect().await {
            Ok(client) => return Ok(client),
            Err(error) => last_error = Some(error),
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    anyhow::bail!(
        "Lost the daemon connection while waiting for sync: {cause}. Reconnecting also failed: {}",
        last_error.map_or_else(|| "unknown error".to_string(), |error| error.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> MxrConfig {
        let mut config = MxrConfig::default();
        config.accounts.insert(
            "personal".into(),
            AccountConfig {
                name: "Personal".into(),
                email: "me@example.com".into(),
                enabled: true,
                sync: Some(SyncProviderConfig::Gmail {
                    credential_source: mxr_config::GmailCredentialSource::Bundled,
                    client_id: "cid".into(),
                    client_secret: None,
                    token_ref: "secret://gmail".into(),
                }),
                send: Some(SendProviderConfig::Gmail),
            },
        );
        config.accounts.insert(
            "work".into(),
            AccountConfig {
                name: "Work".into(),
                email: "me@work.example".into(),
                enabled: true,
                sync: Some(SyncProviderConfig::Imap {
                    host: "imap.example.com".into(),
                    port: 993,
                    username: "me".into(),
                    password_ref: "secret://imap".into(),
                    auth_required: true,
                    use_tls: true,
                }),
                send: Some(SendProviderConfig::Smtp {
                    host: "smtp.example.com".into(),
                    port: 465,
                    username: "me".into(),
                    password_ref: "secret://smtp".into(),
                    auth_required: true,
                    use_tls: true,
                }),
            },
        );
        config
    }

    fn ambiguous_config() -> MxrConfig {
        let mut config = sample_config();
        config.accounts.insert(
            "work-2".into(),
            AccountConfig {
                name: "Work".into(),
                email: "other@work.example".into(),
                enabled: true,
                sync: Some(SyncProviderConfig::Imap {
                    host: "imap.other.example.com".into(),
                    port: 993,
                    username: "other".into(),
                    password_ref: "secret://imap-2".into(),
                    auth_required: true,
                    use_tls: true,
                }),
                send: None,
            },
        );
        config
    }

    #[test]
    fn resolve_account_selection_accepts_config_key_name_and_email() {
        let config = sample_config();

        let by_key =
            resolve_account_selection(&config, "personal").expect("config key should resolve");
        let by_name =
            resolve_account_selection(&config, "Work").expect("display name should resolve");
        let by_email = resolve_account_selection(&config, "me@example.com")
            .expect("email address should resolve");

        assert_eq!(
            by_key,
            AccountId::from_provider_id("gmail", "me@example.com")
        );
        assert_eq!(
            by_name,
            AccountId::from_provider_id("imap", "me@work.example")
        );
        assert_eq!(by_email, by_key);
    }

    #[test]
    fn resolve_account_selection_rejects_ambiguous_display_names() {
        let error = resolve_account_selection(&ambiguous_config(), "Work")
            .err()
            .map(|error| error.to_string());

        assert!(matches!(error.as_deref(), Some(text) if text.contains("ambiguous")));
    }

    #[test]
    fn resolve_account_selection_rejects_unknown_accounts() {
        let error = resolve_account_selection(&sample_config(), "missing")
            .err()
            .map(|error| error.to_string());

        assert!(matches!(error.as_deref(), Some(text) if text.contains("not found")));
    }

    #[test]
    fn build_status_request_targets_selected_account() {
        let account_id = AccountId::from_provider_id("imap", "me@work.example");

        let requested_account_id = if let Request::GetSyncStatus { account_id } =
            build_status_request(Some(&account_id))
        {
            Some(account_id)
        } else {
            None
        };

        assert_eq!(requested_account_id, Some(account_id));
    }

    #[test]
    fn build_sync_request_preserves_selected_account() {
        let account_id = AccountId::from_provider_id("gmail", "me@example.com");

        let requested_account_id = if let Request::SyncNow {
            account_id: Some(account_id),
            ..
        } = build_sync_request(Some(account_id.clone()))
        {
            Some(account_id)
        } else {
            None
        };

        assert_eq!(requested_account_id, Some(account_id));
    }

    fn status(name: &str, attempt: &str, error: Option<&str>) -> AccountSyncStatus {
        AccountSyncStatus {
            account_id: AccountId::from_provider_id("fake", name),
            account_name: name.into(),
            last_attempt_at: Some(attempt.into()),
            last_success_at: None,
            last_error: error.map(str::to_string),
            failure_class: None,
            consecutive_failures: 0,
            backoff_until: None,
            sync_in_progress: false,
            current_cursor_summary: None,
            last_synced_count: 0,
            healthy: error.is_none(),
            progress: None,
        }
    }

    fn with_progress(
        mut status: AccountSyncStatus,
        current: u32,
        total: Option<u32>,
    ) -> AccountSyncStatus {
        status.progress = Some(mxr_protocol::SyncProgressData {
            current,
            total,
            message: "Stored 3000 messages".to_string(),
        });
        status
    }

    /// The denominator is only there when the provider could count what is
    /// left, so the line has to read sensibly either way.
    #[test]
    fn sync_progress_line_shows_a_denominator_only_when_one_is_known() {
        let known = with_progress(status("personal", "t0", None), 3_000, Some(50_000));
        assert_eq!(
            sync_progress_line(&[known]).as_deref(),
            Some("personal: 3,000/50,000 — Stored 3000 messages")
        );

        let unknown = with_progress(status("personal", "t0", None), 3_000, None);
        assert_eq!(
            sync_progress_line(&[unknown]).as_deref(),
            Some("personal: 3,000 — Stored 3000 messages")
        );
    }

    /// Accounts that are not reporting progress contribute nothing, and a
    /// silent set of accounts produces no line at all.
    #[test]
    fn sync_progress_line_skips_accounts_without_progress() {
        assert_eq!(sync_progress_line(&[status("personal", "t0", None)]), None);

        let statuses = vec![
            status("personal", "t0", None),
            with_progress(status("work", "t0", None), 12, None),
        ];
        assert_eq!(
            sync_progress_line(&statuses).as_deref(),
            Some("work: 12 — Stored 3000 messages")
        );
    }

    /// A failed sync goes idle on its way out, so quiescence alone would have
    /// `--wait` report success on a sync that failed.
    #[test]
    fn sync_failures_reports_an_error_from_an_attempt_we_waited_on() {
        let before = vec![status("personal", "t0", None)];
        let after = vec![status("personal", "t1", Some("imap: connection refused"))];
        assert_eq!(
            sync_failures(&before, &after),
            vec!["sync failed for personal: imap: connection refused".to_string()]
        );
    }

    /// An account that was already carrying an error before we triggered, and
    /// has not been attempted since, is not this command's failure to report.
    #[test]
    fn sync_failures_ignores_an_error_that_predates_the_trigger() {
        let before = vec![status("personal", "t0", Some("stale auth failure"))];
        let after = vec![status("personal", "t0", Some("stale auth failure"))];
        assert!(sync_failures(&before, &after).is_empty());
    }

    #[test]
    fn sync_failures_is_empty_when_the_sync_succeeded() {
        let before = vec![status("personal", "t0", None)];
        let after = vec![status("personal", "t1", None)];
        assert!(sync_failures(&before, &after).is_empty());
    }
}
