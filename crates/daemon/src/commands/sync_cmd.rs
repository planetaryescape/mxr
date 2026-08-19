use crate::cli::OutputFormat;
use crate::commands::progress::ProgressPrinter;
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
    Request::SyncNow { account_id }
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

    // The daemon runs a sync pass before it answers, so this can take as long
    // as the mailbox needs. `request_with_events` waits without a deadline and
    // streams the daemon's progress events; `request`'s 120s cap would kill a
    // large or slow sync mid-flight while the daemon kept working (#179).
    let json_mode = matches!(
        resolve_format(format.clone()),
        OutputFormat::Json | OutputFormat::Jsonl
    );
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
        // An error here does not necessarily mean the sync failed: past its own
        // ceiling the daemon detaches the pass and answers with an error while
        // the work carries on. Ask the account which it was rather than reading
        // the error text — the status row is the authority.
        Response::Error { message, .. } => {
            let statuses = fetch_sync_statuses(&mut client, account_id.as_ref()).await?;
            if !statuses.iter().any(|status| status.sync_in_progress) {
                anyhow::bail!("{message}");
            }
            if !json_mode {
                println!("Sync is still running in the daemon; following its progress.");
                if !wait {
                    println!("Watch it with `mxr sync --status`.");
                }
            }
        }
        _ => anyhow::bail!("Unexpected response"),
    }

    // Answering does not mean the account is idle: the daemon may have
    // detached a long pass, and the account's own sync loop may still be
    // working (further backfill pages, a queued tick). `--wait` is what turns
    // "a pass ran" into "the account is idle".
    if wait {
        wait_for_sync_quiescence(
            &mut client,
            account_id.as_ref(),
            Duration::from_secs(wait_timeout_secs),
        )
        .await?;
        if json_mode {
            let statuses = fetch_sync_statuses(&mut client, account_id.as_ref()).await?;
            render_status(&statuses, format);
        }
    }
    Ok(())
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
) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        // Three outcomes, three responses: the daemon rejecting the query is
        // fatal, a dropped connection earns a reconnect, and a status read that
        // simply stalled while the daemon is busy with post-sync work says
        // nothing about the sync — keep polling until the deadline.
        match poll_sync_statuses(client, account_id).await {
            StatusPoll::Statuses(statuses) => {
                if !statuses.iter().any(|status| status.sync_in_progress) {
                    return Ok(());
                }
            }
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

/// One sync-status poll, keeping "the daemon said no" apart from "the
/// connection went away".
enum StatusPoll {
    Statuses(Vec<AccountSyncStatus>),
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
                    data: ResponseData::Status { sync_statuses, .. },
                },
            ) => StatusPoll::Statuses(sync_statuses),
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
        } = build_sync_request(Some(account_id.clone()))
        {
            Some(account_id)
        } else {
            None
        };

        assert_eq!(requested_account_id, Some(account_id));
    }
}
