use crate::cli::OutputFormat;
use crate::commands::degraded_status_reading;
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
    all: bool,
    dry_run: bool,
    status: bool,
    wait: bool,
    wait_timeout_secs: u64,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account_id(account.as_deref())?;

    if all {
        return run_all(
            &mut client,
            dry_run,
            status,
            wait,
            wait_timeout_secs,
            format,
        )
        .await;
    }

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
            let failures = sync_failures_since(before, &after);
            if !failures.is_empty() {
                anyhow::bail!("{}", failures.join("; "));
            }
        }
    }
    Ok(())
}

async fn run_all(
    client: &mut IpcClient,
    dry_run: bool,
    status: bool,
    wait: bool,
    wait_timeout_secs: u64,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    if status {
        render_status(&fetch_sync_statuses(client, None).await?, format);
        return Ok(());
    }

    let protocol_version = match client.request(Request::GetStatus).await? {
        Response::Ok {
            data: ResponseData::Status {
                protocol_version, ..
            },
        } => protocol_version,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from daemon while checking global sync support"),
    };
    ensure_sync_all_protocol(protocol_version)?;

    let json_mode = matches!(
        resolve_format(format.clone()),
        OutputFormat::Json | OutputFormat::Jsonl
    );
    if dry_run {
        let targets = match client.request(Request::PreviewSyncAll).await? {
            Response::Ok {
                data: ResponseData::SyncPlan { targets },
            } => targets,
            Response::Error { message, .. } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response from daemon while selecting sync targets"),
        };
        match resolve_format(format) {
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "dry_run": true,
                    "targets": targets,
                }))?
            ),
            OutputFormat::Jsonl => {
                for target in targets {
                    println!(
                        "{}",
                        serde_json::to_string(&serde_json::json!({
                            "type": "sync_target",
                            "target": target,
                        }))?
                    );
                }
            }
            _ if targets.is_empty() => println!("No enabled sync-capable accounts"),
            _ => {
                for target in targets {
                    let availability = if target.provider_available {
                        "provider ready"
                    } else {
                        "provider unavailable"
                    };
                    println!(
                        "{} ({}) — {availability}",
                        target.account_name, target.account_id
                    );
                }
            }
        }
        return Ok(());
    }

    // The dispatch result is authoritative: preview and execution can be
    // separated by a configuration change.
    let progress = ProgressPrinter::new(json_mode);
    let response = client
        .request_with_events(Request::SyncAll, progress.event_callback())
        .await;
    progress.finish();
    let results = match response? {
        Response::Ok {
            data: ResponseData::SyncBatch { results },
        } => results,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from daemon while starting global sync"),
    };

    let target_ids = sync_wait_target_ids(&results);
    let statuses = if wait {
        wait_for_sync_targets(
            client,
            &target_ids,
            Duration::from_secs(wait_timeout_secs),
            json_mode,
        )
        .await?
    } else {
        Vec::new()
    };
    let mut failures = results
        .iter()
        .filter(|result| result.result == mxr_protocol::SyncAccountResultKind::Failed)
        .map(|result| {
            format!(
                "sync failed for {}: {}",
                result.account_name,
                result.error.as_deref().unwrap_or("couldn't start")
            )
        })
        .collect::<Vec<_>>();
    if wait {
        failures.extend(sync_failures(&results, &statuses));
    }
    match resolve_format(format) {
        OutputFormat::Json if wait => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "results": results,
                "statuses": statuses,
                "failures": failures,
            }))?
        ),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "results": results,
            }))?
        ),
        OutputFormat::Jsonl => {
            for result in &results {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "sync_result",
                        "result": result,
                    }))?
                );
                if result.result == mxr_protocol::SyncAccountResultKind::Failed {
                    println!(
                        "{}",
                        serde_json::to_string(&serde_json::json!({
                            "type": "sync_failure",
                            "account_id": result.account_id,
                            "account_name": result.account_name,
                            "error": result.error,
                        }))?
                    );
                }
            }
            for sync in &statuses {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "sync_status",
                        "status": sync,
                    }))?
                );
                let (kind, error) = match sync.last_error.as_deref() {
                    Some(error) => ("sync_failed", Some(error)),
                    None => ("sync_completed", None),
                };
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": kind,
                        "account_id": sync.account_id,
                        "account_name": sync.account_name,
                        "error": error,
                    }))?
                );
            }
        }
        _ => {
            for result in &results {
                match result.result {
                    mxr_protocol::SyncAccountResultKind::Started => {
                        println!("{}: sync started", result.account_name);
                    }
                    mxr_protocol::SyncAccountResultKind::Joined => {
                        println!("{}: joined the running sync", result.account_name);
                    }
                    mxr_protocol::SyncAccountResultKind::Failed => println!(
                        "{}: sync failed to start: {}",
                        result.account_name,
                        result.error.as_deref().unwrap_or("unknown error")
                    ),
                }
            }
            if wait {
                render_sync_status(&statuses, IPC_PROTOCOL_VERSION);
            }
        }
    }
    if !failures.is_empty() {
        anyhow::bail!("{}", failures.join("; "));
    }
    Ok(())
}

fn ensure_sync_all_protocol(protocol_version: u32) -> anyhow::Result<()> {
    if protocol_version < IPC_PROTOCOL_VERSION {
        anyhow::bail!(
            "Global sync requires daemon IPC protocol {IPC_PROTOCOL_VERSION}; running daemon uses {protocol_version}. Update or restart the daemon before retrying."
        );
    }
    Ok(())
}

fn sync_wait_target_ids(
    results: &[mxr_protocol::SyncAccountResult],
) -> std::collections::HashSet<AccountId> {
    results
        .iter()
        .filter(|result| result.result != mxr_protocol::SyncAccountResultKind::Failed)
        .map(|result| result.account_id.clone())
        .collect()
}

fn sync_targets_quiescent(
    statuses: &[AccountSyncStatus],
    target_ids: &std::collections::HashSet<AccountId>,
) -> bool {
    statuses.len() == target_ids.len()
        && statuses
            .iter()
            .map(|status| &status.account_id)
            .collect::<std::collections::HashSet<_>>()
            == target_ids.iter().collect()
        && !statuses.iter().any(|status| status.sync_in_progress)
}

async fn wait_for_sync_targets(
    client: &mut IpcClient,
    target_ids: &std::collections::HashSet<AccountId>,
    timeout: Duration,
    json_mode: bool,
) -> anyhow::Result<Vec<AccountSyncStatus>> {
    if target_ids.is_empty() {
        return Ok(Vec::new());
    }
    let progress = ProgressPrinter::new(json_mode);
    let deadline = Instant::now() + timeout;
    let mut last_line: Option<String> = None;
    loop {
        match poll_sync_statuses(client, None).await {
            StatusPoll::Statuses(mut statuses) => {
                statuses.retain(|status| target_ids.contains(&status.account_id));
                if sync_targets_quiescent(&statuses, target_ids) {
                    progress.finish();
                    return Ok(statuses);
                }
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
            StatusPoll::Disconnected(error) => *client = reconnect_client(&error).await?,
        }
        if Instant::now() >= deadline {
            progress.finish();
            anyhow::bail!(
                "timed out after {}s waiting for sync to quiesce",
                timeout.as_secs()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Errors left by a sync that this invocation started or joined.
///
/// Success clears `last_error`; a failed retry can retain the exact same error
/// and attempt timestamp as the status observed before joining. Therefore an
/// idle selected account with an error is a failed wait, while errors for
/// accounts outside the dispatched target set are irrelevant.
fn sync_failures(
    results: &[mxr_protocol::SyncAccountResult],
    statuses: &[AccountSyncStatus],
) -> Vec<String> {
    let selected = results
        .iter()
        .filter(|result| result.result != mxr_protocol::SyncAccountResultKind::Failed)
        .map(|result| &result.account_id)
        .collect::<std::collections::HashSet<_>>();
    statuses
        .iter()
        .filter(|status| selected.contains(&status.account_id))
        .filter_map(|status| {
            status
                .last_error
                .as_deref()
                .map(|error| format!("sync failed for {}: {error}", status.account_name))
        })
        .collect()
}

/// Legacy account-scoped sync has no dispatch result set, so it compares its
/// selected status with the snapshot taken before its one-account request.
fn sync_failures_since(before: &[AccountSyncStatus], after: &[AccountSyncStatus]) -> Vec<String> {
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
    /// saturated — which is exactly what a large sync does to it — and says so
    /// with `degraded`, reporting zero accounts and no statuses. Reading that
    /// as "no account is syncing" would end the wait on the daemon's busiest
    /// moment, so treat it as no reading at all.
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
                            degraded,
                            ..
                        },
                },
            ) => {
                if degraded_status_reading(degraded, &accounts) {
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

    #[test]
    fn global_sync_requires_the_current_protocol() {
        for version in 4..IPC_PROTOCOL_VERSION {
            let error =
                ensure_sync_all_protocol(version).expect_err("an older protocol cannot sync all");
            assert!(error.to_string().contains(&format!(
                "requires daemon IPC protocol {IPC_PROTOCOL_VERSION}"
            )));
            assert!(error
                .to_string()
                .contains(&format!("running daemon uses {version}")));
        }
        assert!(ensure_sync_all_protocol(IPC_PROTOCOL_VERSION).is_ok());
    }

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
                    max_connections: 4,
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
                    max_connections: 4,
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

    #[test]
    fn legacy_unscoped_sync_request_still_targets_default_account_only() {
        assert!(matches!(
            build_sync_request(None),
            Request::SyncNow {
                account_id: None,
                background: true,
            }
        ));
        assert!(!matches!(build_sync_request(None), Request::SyncAll));
    }

    #[test]
    fn sync_wait_only_tracks_accounts_that_started_or_joined() {
        let started = AccountId::from_provider_id("fake", "started@example.com");
        let joined = AccountId::from_provider_id("fake", "joined@example.com");
        let failed = AccountId::from_provider_id("fake", "failed@example.com");
        let results = [
            mxr_protocol::SyncAccountResult {
                account_id: started.clone(),
                account_name: "Started".into(),
                result: mxr_protocol::SyncAccountResultKind::Started,
                error: None,
            },
            mxr_protocol::SyncAccountResult {
                account_id: joined.clone(),
                account_name: "Joined".into(),
                result: mxr_protocol::SyncAccountResultKind::Joined,
                error: None,
            },
            mxr_protocol::SyncAccountResult {
                account_id: failed.clone(),
                account_name: "Failed".into(),
                result: mxr_protocol::SyncAccountResultKind::Failed,
                error: Some("provider unavailable".into()),
            },
        ];

        assert_eq!(
            sync_wait_target_ids(&results),
            std::collections::HashSet::from([started, joined])
        );
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

    /// An idle pass has no denominator, so the line must not offer one. The
    /// daemon withholds the total for exactly this reason — rendering it would
    /// read "0/0", which looks like a stalled sync rather than an idle one.
    #[test]
    fn sync_progress_line_never_renders_a_zero_denominator() {
        let idle = with_progress(status("personal", "t0", None), 0, None);
        let line = sync_progress_line(&[idle]).expect("a reporting account produces a line");
        assert_eq!(line, "personal: 0 — Stored 3000 messages");
        assert!(!line.contains('/'), "no denominator when none is known");
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

    #[test]
    fn sync_failures_reports_an_error_when_the_initial_status_read_was_unavailable() {
        let result = sync_result("personal", mxr_protocol::SyncAccountResultKind::Started);
        let after = vec![status("personal", "t1", Some("imap: connection refused"))];
        assert_eq!(
            sync_failures(&[result], &after),
            vec!["sync failed for personal: imap: connection refused".to_string()]
        );
    }

    #[test]
    fn sync_failures_reports_the_same_error_after_a_joined_retry() {
        let result = sync_result("personal", mxr_protocol::SyncAccountResultKind::Joined);
        let after = vec![status("personal", "t0", Some("same provider error"))];
        assert_eq!(
            sync_failures(&[result], &after),
            vec!["sync failed for personal: same provider error".to_string()]
        );
    }

    #[test]
    fn sync_failures_is_empty_when_the_sync_succeeded() {
        let result = sync_result("personal", mxr_protocol::SyncAccountResultKind::Started);
        let after = vec![status("personal", "t1", None)];
        assert!(sync_failures(&[result], &after).is_empty());
    }

    #[test]
    fn sync_failures_ignores_errors_for_unselected_accounts() {
        let selected = sync_result("selected", mxr_protocol::SyncAccountResultKind::Started);
        let statuses = vec![
            status("selected", "t1", None),
            status("unselected", "t0", Some("old failure")),
        ];
        assert!(sync_failures(&[selected], &statuses).is_empty());
    }

    #[test]
    fn legacy_sync_failures_reports_a_new_account_error() {
        let before = vec![status("personal", "t0", None)];
        let after = vec![status("personal", "t1", Some("connection refused"))];
        assert_eq!(
            sync_failures_since(&before, &after),
            vec!["sync failed for personal: connection refused".to_string()]
        );
    }

    #[test]
    fn legacy_sync_failures_ignores_an_error_that_predates_the_request() {
        let before = vec![status("personal", "t0", Some("stale auth failure"))];
        let after = vec![status("personal", "t0", Some("stale auth failure"))];
        assert!(sync_failures_since(&before, &after).is_empty());
    }

    #[test]
    fn wait_does_not_treat_a_missing_selected_account_as_quiescent() {
        let selected = AccountId::from_provider_id("fake", "selected");
        let target_ids = std::collections::HashSet::from([selected]);
        assert!(!sync_targets_quiescent(&[], &target_ids));
        assert!(sync_targets_quiescent(
            &[status("selected", "t1", None)],
            &target_ids
        ));
    }

    #[test]
    fn wait_rejects_partial_status_sets_even_when_returned_accounts_are_idle() {
        let target_ids = std::collections::HashSet::from([
            AccountId::from_provider_id("fake", "selected"),
            AccountId::from_provider_id("fake", "missing"),
        ]);
        assert!(!sync_targets_quiescent(
            &[status("selected", "t1", None)],
            &target_ids
        ));
    }

    fn sync_result(
        account_name: &str,
        result: mxr_protocol::SyncAccountResultKind,
    ) -> mxr_protocol::SyncAccountResult {
        mxr_protocol::SyncAccountResult {
            account_id: AccountId::from_provider_id("fake", account_name),
            account_name: account_name.into(),
            result,
            error: None,
        }
    }
}
