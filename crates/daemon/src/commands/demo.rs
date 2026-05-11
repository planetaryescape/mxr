use crate::ipc_client::IpcClient;
use mxr_config::{AccountConfig, SendProviderConfig, SyncProviderConfig};
use mxr_core::id::AccountId;
use mxr_protocol::{AccountSyncStatus, Request, Response, ResponseData};
use std::path::PathBuf;
use std::time::Duration;

const DEMO_ACCOUNT_KEY: &str = "demo";
const DEMO_INSTANCE: &str = "mxr-demo";
const DEMO_COUNT_MARKER: &str = "demo-message-count";

#[derive(Debug, Clone)]
pub struct DemoPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

pub fn prepare_environment(messages: usize) -> anyhow::Result<DemoPaths> {
    let paths = demo_paths();
    std::env::set_var("MXR_INSTANCE", DEMO_INSTANCE);
    std::env::set_var("MXR_CONFIG_DIR", &paths.config_dir);
    std::env::set_var("MXR_DATA_DIR", &paths.data_dir);
    std::env::set_var("MXR_FAKE_DATASET", "demo");
    std::env::set_var("MXR_FAKE_MESSAGE_COUNT", messages.to_string());
    Ok(paths)
}

pub async fn reset_profile() -> anyhow::Result<()> {
    let socket = mxr_config::socket_path();
    let state =
        crate::server::shutdown_daemon_for_maintenance(&socket, Duration::from_secs(5)).await?;
    if matches!(state, crate::server::SocketState::Reachable) {
        anyhow::bail!(
            "Demo daemon is still running at {}. Stop it, then retry `mxr demo --reset`.",
            socket.display()
        );
    }

    let paths = demo_paths();
    remove_dir_if_exists(&paths.config_dir)?;
    remove_dir_if_exists(&paths.data_dir)?;
    Ok(())
}

pub async fn run(messages: usize, no_tui: bool) -> anyhow::Result<()> {
    let prepared_paths = prepare_environment(messages)?;
    if let Some(existing_count) = read_demo_message_count(&prepared_paths)? {
        if existing_count != messages {
            println!(
                "Demo profile has {existing_count} messages; resetting for {messages} messages..."
            );
            reset_profile().await?;
        }
    }

    let paths = ensure_demo_config(messages)?;
    println!("mxr demo profile");
    println!(
        "  config: {}",
        paths.config_dir.join("config.toml").display()
    );
    println!("  data:   {}", paths.data_dir.display());
    println!("  inbox:  {messages} synthetic messages");
    println!();

    crate::server::ensure_daemon_running().await?;
    println!("Seeding demo mailbox...");
    let status = trigger_demo_sync_and_wait(Duration::from_secs(180)).await?;
    if status.last_synced_count > 0 {
        println!("Synced {} demo messages.", status.last_synced_count);
        write_demo_message_count(&paths, messages)?;
    } else {
        println!("Demo mailbox is already up to date.");
    }

    if no_tui {
        println!();
        println!("Demo is ready. Open it with:");
        println!("  mxr demo");
        println!();
        println!("Reset it with:");
        println!("  mxr demo --reset");
        return Ok(());
    }

    println!();
    println!("Opening demo TUI. Nothing here touches your real inbox.");
    crate::server::ensure_daemon_supports_tui().await?;
    mxr_tui::run().await?;
    Ok(())
}

async fn trigger_demo_sync_and_wait(timeout: Duration) -> anyhow::Result<AccountSyncStatus> {
    let account_id = demo_account_id();
    let mut client = IpcClient::connect().await?;
    let before = fetch_demo_sync_status(&mut client, &account_id)
        .await
        .ok()
        .and_then(|status| status.last_success_at);

    match client
        .request(Request::SyncNow {
            account_id: Some(account_id.clone()),
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::Ack,
        } => {}
        Response::Error { message, .. } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected sync response: {other:?}"),
    }

    let deadline = std::time::Instant::now() + timeout;
    loop {
        let status = fetch_demo_sync_status(&mut client, &account_id).await?;
        let completed_new_sync = status.last_success_at != before || status.last_synced_count > 0;
        if completed_new_sync && !status.sync_in_progress {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {}s waiting for demo sync to finish",
                timeout.as_secs()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn fetch_demo_sync_status(
    client: &mut IpcClient,
    account_id: &AccountId,
) -> anyhow::Result<AccountSyncStatus> {
    match client
        .request(Request::GetSyncStatus {
            account_id: account_id.clone(),
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::SyncStatus { sync },
        } => Ok(sync),
        Response::Error { message, .. } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected sync status response: {other:?}"),
    }
}

fn demo_account_id() -> AccountId {
    AccountId::from_provider_id("fake", "alex@demo.mxr.local")
}

fn ensure_demo_config(messages: usize) -> anyhow::Result<DemoPaths> {
    let paths = prepare_environment(messages)?;
    let config_path = paths.config_dir.join("config.toml");
    let mut config = mxr_config::load_config_from_path(&config_path).unwrap_or_default();

    config.general.default_account = Some(DEMO_ACCOUNT_KEY.to_string());
    config.accounts.insert(
        DEMO_ACCOUNT_KEY.to_string(),
        AccountConfig {
            name: "Demo".to_string(),
            email: "alex@demo.mxr.local".to_string(),
            enabled: true,
            sync: Some(SyncProviderConfig::Fake),
            send: Some(SendProviderConfig::Fake),
        },
    );

    std::fs::create_dir_all(&paths.config_dir)?;
    std::fs::create_dir_all(&paths.data_dir)?;
    mxr_config::save_config_to_path(&config, &config_path)?;
    Ok(paths)
}

fn demo_paths() -> DemoPaths {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEMO_INSTANCE);
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEMO_INSTANCE);
    DemoPaths {
        config_dir,
        data_dir,
    }
}

fn remove_dir_if_exists(path: &std::path::Path) -> anyhow::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn read_demo_message_count(paths: &DemoPaths) -> anyhow::Result<Option<usize>> {
    let path = paths.data_dir.join(DEMO_COUNT_MARKER);
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents.trim().parse::<usize>().ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_demo_message_count(paths: &DemoPaths, messages: usize) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;
    std::fs::write(paths.data_dir.join(DEMO_COUNT_MARKER), messages.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_paths_are_namespaced() {
        let paths = demo_paths();
        assert!(paths.config_dir.ends_with(DEMO_INSTANCE));
        assert!(paths.data_dir.ends_with(DEMO_INSTANCE));
    }

    #[test]
    fn prepare_environment_selects_isolated_demo_profile() {
        temp_env::with_vars(
            [
                ("MXR_INSTANCE", None::<&str>),
                ("MXR_CONFIG_DIR", None::<&str>),
                ("MXR_DATA_DIR", None::<&str>),
                ("MXR_FAKE_DATASET", None::<&str>),
                ("MXR_FAKE_MESSAGE_COUNT", None::<&str>),
            ],
            || {
                let paths = prepare_environment(123).expect("prepare demo env");
                assert_eq!(std::env::var("MXR_INSTANCE").as_deref(), Ok(DEMO_INSTANCE));
                assert_eq!(
                    std::env::var("MXR_CONFIG_DIR").expect("MXR_CONFIG_DIR set"),
                    paths.config_dir.display().to_string()
                );
                assert_eq!(
                    std::env::var("MXR_DATA_DIR").expect("MXR_DATA_DIR set"),
                    paths.data_dir.display().to_string()
                );
                assert_eq!(std::env::var("MXR_FAKE_DATASET").as_deref(), Ok("demo"));
                assert_eq!(
                    std::env::var("MXR_FAKE_MESSAGE_COUNT").as_deref(),
                    Ok("123")
                );
            },
        );
    }

    #[test]
    fn demo_message_count_marker_round_trips() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let paths = DemoPaths {
            config_dir: temp_dir.path().join("config"),
            data_dir: temp_dir.path().join("data"),
        };

        assert_eq!(
            read_demo_message_count(&paths).expect("read missing marker"),
            None
        );
        write_demo_message_count(&paths, 50_000).expect("write marker");
        assert_eq!(
            read_demo_message_count(&paths).expect("read marker"),
            Some(50_000)
        );
    }
}
