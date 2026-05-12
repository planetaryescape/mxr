use crate::ipc_client::IpcClient;
use chrono::{Datelike, TimeZone};
use mxr_config::{AccountConfig, SendProviderConfig, SyncProviderConfig};
use mxr_core::id::AccountId;
use mxr_core::types::SemanticProfileStatus;
use mxr_protocol::{AccountSyncStatus, Request, Response, ResponseData};
use mxr_rules::{Conditions, FieldCondition, Rule, RuleAction, RuleId, StringMatch};
use std::path::PathBuf;
use std::time::Duration;

const DEMO_ACCOUNT_KEY: &str = "personal";
const DEMO_WORK_ACCOUNT_KEY: &str = "work";
const DEMO_PERSONAL_EMAIL: &str = "alex@demo.mxr.local";
const DEMO_WORK_EMAIL: &str = "alex@work.demo.mxr.local";
const DEMO_INSTANCE: &str = "mxr-demo";
const DEMO_COUNT_MARKER: &str = "demo-message-count";
const DEMO_SEED_VERSION: u32 = 3;

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
    if let Some((existing_count, seed_version)) = read_demo_message_count(&prepared_paths)? {
        if existing_count != messages || seed_version != DEMO_SEED_VERSION {
            println!(
                "Demo profile has seed v{seed_version} with {existing_count} messages; resetting for seed v{DEMO_SEED_VERSION} with {messages} messages..."
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
    println!("  inbox:  {messages} synthetic messages across 2 accounts");
    println!();

    crate::server::ensure_daemon_running().await?;
    seed_demo_rules().await?;
    println!("Seeding demo mailbox...");
    let statuses = trigger_demo_sync_and_wait(Duration::from_secs(180)).await?;
    let synced_count = statuses
        .iter()
        .map(|status| status.last_synced_count as usize)
        .sum::<usize>();
    if synced_count > 0 {
        println!("Synced {synced_count} demo messages.");
        write_demo_message_count(&paths, messages)?;
    } else {
        println!("Demo mailbox is already up to date.");
    }
    prewarm_demo_runtime(synced_count > 0).await?;

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

async fn prewarm_demo_runtime(force_semantic: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    println!("Precomputing demo analytics...");
    match client.request(Request::RebuildAnalytics).await? {
        Response::Ok {
            data: ResponseData::AnalyticsRebuildSummary { .. },
        } => {}
        Response::Error { message, .. } => {
            println!("  analytics prewarm skipped: {message}");
        }
        other => anyhow::bail!("Unexpected analytics prewarm response: {other:?}"),
    }

    prewarm_wrapped(&mut client, None).await?;
    for account_id in demo_account_ids() {
        prewarm_wrapped(&mut client, Some(account_id)).await?;
    }

    let semantic_snapshot = match client.request(Request::GetSemanticStatus).await? {
        Response::Ok {
            data: ResponseData::SemanticStatus { snapshot },
        } => snapshot,
        Response::Error { message, .. } => {
            println!("  semantic prewarm skipped: {message}");
            return Ok(());
        }
        other => anyhow::bail!("Unexpected semantic status response: {other:?}"),
    };

    if !semantic_snapshot.enabled {
        println!("  semantic prewarm skipped: semantic search is disabled");
        return Ok(());
    }

    let active_profile = semantic_snapshot.active_profile;
    let active_record = semantic_snapshot
        .profiles
        .iter()
        .find(|profile| profile.profile == active_profile);
    let semantic_ready = active_record.is_some_and(|profile| {
        profile.status == SemanticProfileStatus::Ready && profile.last_indexed_at.is_some()
    });
    if !force_semantic && semantic_ready {
        println!("  semantic vectors already warm");
        return Ok(());
    }

    println!(
        "Precomputing semantic vectors for {}...",
        active_profile.as_str()
    );
    match client.request(Request::ReindexSemantic).await? {
        Response::Ok {
            data: ResponseData::SemanticStatus { .. },
        } => {}
        Response::Error { message, .. } => {
            println!("  semantic prewarm skipped: {message}");
        }
        other => anyhow::bail!("Unexpected semantic prewarm response: {other:?}"),
    }

    Ok(())
}

async fn prewarm_wrapped(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let Some(start) = chrono::Utc
        .with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0)
        .single()
    else {
        return Ok(());
    };
    let label = format!("{} year-to-date", now.year());
    match client
        .request(Request::Wrapped {
            account_id,
            since_unix: start.timestamp(),
            until_unix: now.timestamp(),
            label,
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::Wrapped { .. },
        } => {}
        Response::Error { message, .. } => {
            println!("  wrapped prewarm skipped: {message}");
        }
        other => anyhow::bail!("Unexpected wrapped prewarm response: {other:?}"),
    }
    Ok(())
}

async fn trigger_demo_sync_and_wait(timeout: Duration) -> anyhow::Result<Vec<AccountSyncStatus>> {
    let mut statuses = Vec::new();
    for account_id in demo_account_ids() {
        statuses.push(trigger_demo_account_sync_and_wait(account_id, timeout).await?);
    }
    Ok(statuses)
}

async fn trigger_demo_account_sync_and_wait(
    account_id: AccountId,
    timeout: Duration,
) -> anyhow::Result<AccountSyncStatus> {
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

async fn seed_demo_rules() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    for rule in demo_rules() {
        let value = serde_json::to_value(rule)?;
        match client.request(Request::UpsertRule { rule: value }).await? {
            Response::Ok {
                data: ResponseData::RuleData { .. },
            } => {}
            Response::Error { message, .. } => anyhow::bail!("failed to seed demo rule: {message}"),
            other => anyhow::bail!("Unexpected rule seed response: {other:?}"),
        }
    }
    Ok(())
}

fn demo_rules() -> Vec<Rule> {
    let now = chrono::Utc::now();
    vec![
        Rule {
            id: RuleId("demo-newsletters".to_string()),
            name: "Demo: newsletters are marked read".to_string(),
            enabled: true,
            priority: 10,
            conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
            actions: vec![
                RuleAction::AddLabel {
                    label: "newsletters".to_string(),
                },
                RuleAction::MarkRead,
            ],
            created_at: now,
            updated_at: now,
        },
        Rule {
            id: RuleId("demo-build-failures".to_string()),
            name: "Demo: star build failures".to_string(),
            enabled: true,
            priority: 20,
            conditions: Conditions::Field(FieldCondition::Subject {
                pattern: StringMatch::Contains("Build failed".to_string()),
            }),
            actions: vec![
                RuleAction::AddLabel {
                    label: "alerts".to_string(),
                },
                RuleAction::Star,
            ],
            created_at: now,
            updated_at: now,
        },
        Rule {
            id: RuleId("demo-receipts".to_string()),
            name: "Demo: receipts leave the inbox after read".to_string(),
            enabled: true,
            priority: 30,
            conditions: Conditions::And {
                conditions: vec![
                    Conditions::Field(FieldCondition::HasLabel {
                        label: "receipts".to_string(),
                    }),
                    Conditions::Not {
                        condition: Box::new(Conditions::Field(FieldCondition::IsUnread)),
                    },
                ],
            },
            actions: vec![RuleAction::Archive],
            created_at: now,
            updated_at: now,
        },
        Rule {
            id: RuleId("demo-promotions".to_string()),
            name: "Demo: promotions are marked read".to_string(),
            enabled: true,
            priority: 40,
            conditions: Conditions::Field(FieldCondition::From {
                pattern: StringMatch::Contains("@promo.demo.mxr.local".to_string()),
            }),
            actions: vec![
                RuleAction::AddLabel {
                    label: "promotions".to_string(),
                },
                RuleAction::MarkRead,
            ],
            created_at: now,
            updated_at: now,
        },
        Rule {
            id: RuleId("demo-potential-spam".to_string()),
            name: "Demo: suspicious inbox mail gets flagged".to_string(),
            enabled: true,
            priority: 50,
            conditions: Conditions::Or {
                conditions: vec![
                    Conditions::Field(FieldCondition::Subject {
                        pattern: StringMatch::Contains("action required".to_string()),
                    }),
                    Conditions::Field(FieldCondition::BodyContains {
                        pattern: StringMatch::Contains("urgent password reset".to_string()),
                    }),
                ],
            },
            actions: vec![
                RuleAction::AddLabel {
                    label: "potential_spam".to_string(),
                },
                RuleAction::Star,
            ],
            created_at: now,
            updated_at: now,
        },
    ]
}

fn demo_account_ids() -> [AccountId; 2] {
    [
        AccountId::from_provider_id("fake", DEMO_PERSONAL_EMAIL),
        AccountId::from_provider_id("fake", DEMO_WORK_EMAIL),
    ]
}

fn ensure_demo_config(messages: usize) -> anyhow::Result<DemoPaths> {
    let paths = prepare_environment(messages)?;
    let config_path = paths.config_dir.join("config.toml");
    let mut config = mxr_config::load_config_from_path(&config_path).unwrap_or_default();

    config.general.default_account = Some(DEMO_ACCOUNT_KEY.to_string());
    config.accounts.insert(
        DEMO_ACCOUNT_KEY.to_string(),
        AccountConfig {
            name: "Demo Personal".to_string(),
            email: DEMO_PERSONAL_EMAIL.to_string(),
            enabled: true,
            sync: Some(SyncProviderConfig::Fake),
            send: Some(SendProviderConfig::Fake),
        },
    );
    config.accounts.insert(
        DEMO_WORK_ACCOUNT_KEY.to_string(),
        AccountConfig {
            name: "Demo Work".to_string(),
            email: DEMO_WORK_EMAIL.to_string(),
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

fn read_demo_message_count(paths: &DemoPaths) -> anyhow::Result<Option<(usize, u32)>> {
    let path = paths.data_dir.join(DEMO_COUNT_MARKER);
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim();
            let (version, count) = if let Some((version, count)) = trimmed.split_once(':') {
                (
                    version.parse::<u32>().unwrap_or(0),
                    count.parse::<usize>().unwrap_or(0),
                )
            } else {
                (0, trimmed.parse::<usize>().unwrap_or(0))
            };
            Ok((count > 0).then_some((count, version)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_demo_message_count(paths: &DemoPaths, messages: usize) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;
    std::fs::write(
        paths.data_dir.join(DEMO_COUNT_MARKER),
        format!("{DEMO_SEED_VERSION}:{messages}"),
    )?;
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
    fn demo_account_ids_cover_personal_and_work() {
        let ids = demo_account_ids();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(
            ids[0],
            AccountId::from_provider_id("fake", DEMO_PERSONAL_EMAIL)
        );
        assert_eq!(ids[1], AccountId::from_provider_id("fake", DEMO_WORK_EMAIL));
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
            Some((50_000, DEMO_SEED_VERSION))
        );
    }

    #[test]
    fn legacy_demo_message_count_marker_forces_seed_version_reset() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let paths = DemoPaths {
            config_dir: temp_dir.path().join("config"),
            data_dir: temp_dir.path().join("data"),
        };
        std::fs::create_dir_all(&paths.data_dir).expect("create data dir");
        std::fs::write(paths.data_dir.join(DEMO_COUNT_MARKER), "50000")
            .expect("write legacy marker");

        assert_eq!(
            read_demo_message_count(&paths).expect("read marker"),
            Some((50_000, 0))
        );
    }
}
