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
const DEMO_INSTANCE: &str = mxr_config::DEMO_INSTANCE_NAME;
const DEMO_COUNT_MARKER: &str = "demo-message-count";
const DEMO_SEED_VERSION: u32 = 4;
const DEMO_DEFAULT_MESSAGES: usize = 50_000;
const DEMO_ACTIVE_MARKER: &str = "demo-active";

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

/// Path to the "demo is active" marker file. Independent of MXR_INSTANCE so
/// it stays visible across CLI invocations regardless of which profile the
/// daemon is currently bound to.
pub fn active_marker_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mxr")
        .join(DEMO_ACTIVE_MARKER)
}

pub fn is_active() -> bool {
    active_marker_path().exists()
}

fn read_active_marker() -> Option<usize> {
    let contents = std::fs::read_to_string(active_marker_path()).ok()?;
    contents.trim().parse::<usize>().ok()
}

fn write_active_marker(messages: usize) -> anyhow::Result<()> {
    let path = active_marker_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{messages}\n"))?;
    Ok(())
}

fn remove_active_marker() -> anyhow::Result<()> {
    match std::fs::remove_file(active_marker_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Apply demo environment for the currently-active demo profile. Used by
/// every non-Demo CLI command when the active marker is present so that
/// `mxr search`, `mxr cat`, etc. transparently operate on demo data.
pub fn apply_active_environment() -> anyhow::Result<Option<DemoPaths>> {
    if !is_active() {
        return Ok(None);
    }
    let messages = read_active_marker().unwrap_or(DEMO_DEFAULT_MESSAGES);
    Ok(Some(prepare_environment(messages)?))
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
    remove_active_marker()?;
    Ok(())
}

/// `mxr demo stop` — exit sticky demo mode. Shuts down the demo daemon and
/// removes the active marker. Real-account profile is restored on the next
/// CLI invocation because demo env vars only live in this process.
pub async fn stop() -> anyhow::Result<()> {
    if !is_active() {
        println!("Demo mode is not active.");
        return Ok(());
    }

    // We must apply demo env before reading socket_path so we target the
    // demo daemon, not the user's real daemon.
    prepare_environment(read_active_marker().unwrap_or(DEMO_DEFAULT_MESSAGES))?;

    let socket = mxr_config::socket_path();
    println!("Stopping demo daemon at {}...", socket.display());
    let state =
        crate::server::shutdown_daemon_for_maintenance(&socket, Duration::from_secs(10)).await?;
    if matches!(state, crate::server::SocketState::Reachable) {
        anyhow::bail!(
            "Demo daemon at {} did not shut down within the timeout. Try again.",
            socket.display()
        );
    }

    remove_active_marker()?;
    println!("Demo mode stopped. Real profile restored.");
    Ok(())
}

/// `mxr demo status` — print whether sticky demo is active and where its
/// profile lives.
pub fn status() -> anyhow::Result<()> {
    if !is_active() {
        println!("Demo mode: inactive");
        return Ok(());
    }
    let paths = demo_paths();
    let messages = read_active_marker().unwrap_or(DEMO_DEFAULT_MESSAGES);
    println!("Demo mode: active");
    println!("  marker:   {}", active_marker_path().display());
    println!("  config:   {}", paths.config_dir.display());
    println!("  data:     {}", paths.data_dir.display());
    println!("  messages: {messages} (seed v{DEMO_SEED_VERSION})");
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
    println!("Seeding demo surfaces (snippets, signatures, screener, drafts, ...)");
    seed_demo_surfaces().await?;
    prewarm_demo_runtime(synced_count > 0).await?;

    // Sticky demo: every subsequent `mxr <cmd>` invocation should hit demo
    // data until the user runs `mxr demo stop`. Marker is written outside
    // the demo data dir so it survives independent of MXR_INSTANCE.
    write_active_marker(messages)?;
    println!();
    println!("Demo mode is now active. All `mxr` commands target the demo profile.");
    println!("Run `mxr demo stop` to exit demo mode.");

    if no_tui {
        println!();
        println!("Demo is ready. Open it with:");
        println!("  mxr demo");
        println!();
        println!("Reset it with:");
        println!("  mxr demo reset");
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
            return Ok(());
        }
        other => anyhow::bail!("Unexpected semantic prewarm response: {other:?}"),
    }

    // ReindexSemantic returns once the request is acknowledged; the worker
    // keeps producing embeddings in the background while the profile sits
    // in `Indexing`. Without this wait, `mxr search` after demo start would
    // show "Indexing..." until the background work finished. Poll until the
    // active profile transitions to `Ready`.
    wait_for_semantic_ready(&mut client, active_profile, Duration::from_secs(600)).await?;
    println!("  semantic vectors ready");

    // Fill LLM-backed caches (voice, decisions) using the canned demo provider
    // so first-click on those surfaces shows pre-built content. Soft-fails
    // per account.
    println!("Prewarming LLM-backed surfaces (voice, decisions)...");
    if let Err(error) = prewarm_llm_caches(&mut client).await {
        println!("  llm prewarm skipped: {error}");
    }
    Ok(())
}

async fn wait_for_semantic_ready(
    client: &mut IpcClient,
    active_profile: mxr_core::types::SemanticProfile,
    timeout: Duration,
) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let snapshot = match client.request(Request::GetSemanticStatus).await? {
            Response::Ok {
                data: ResponseData::SemanticStatus { snapshot },
            } => snapshot,
            Response::Error { message, .. } => {
                anyhow::bail!("semantic status query failed while waiting: {message}");
            }
            other => anyhow::bail!("Unexpected semantic status response: {other:?}"),
        };
        let active = snapshot
            .profiles
            .iter()
            .find(|profile| profile.profile == active_profile);
        match active.map(|profile| profile.status) {
            Some(SemanticProfileStatus::Ready) => return Ok(()),
            Some(SemanticProfileStatus::Error) => {
                let message = active
                    .and_then(|profile| profile.last_error.clone())
                    .unwrap_or_else(|| "unknown error".to_string());
                anyhow::bail!("semantic prewarm failed: {message}");
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {}s waiting for semantic profile {} to reach Ready",
                timeout.as_secs(),
                active_profile.as_str()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
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

/// Populate every "empty queue" surface the demo profile would otherwise show
/// as blank: snippets, signatures, custom labels, saved searches, drafts,
/// screener decisions, snoozed mail, and reply-later flags. All upserts are
/// idempotent — re-running `mxr demo` against an existing profile leaves the
/// number of seeded artifacts unchanged.
///
/// Soft-fails on per-feature errors (logs and continues) because a partial
/// seed is better than an aborted `mxr demo` start.
async fn seed_demo_surfaces() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    if let Err(error) = seed_demo_snippets(&mut client).await {
        println!("  seed snippets skipped: {error}");
    }
    if let Err(error) = seed_demo_signatures(&mut client).await {
        println!("  seed signatures skipped: {error}");
    }
    if let Err(error) = seed_demo_labels(&mut client).await {
        println!("  seed labels skipped: {error}");
    }
    if let Err(error) = seed_demo_saved_searches(&mut client).await {
        println!("  seed saved searches skipped: {error}");
    }
    if let Err(error) = seed_demo_screener(&mut client).await {
        println!("  seed screener skipped: {error}");
    }
    if let Err(error) = seed_demo_message_state(&mut client).await {
        println!("  seed snooze/reply-later skipped: {error}");
    }
    if let Err(error) = seed_demo_drafts(&mut client).await {
        println!("  seed drafts skipped: {error}");
    }
    Ok(())
}

async fn seed_demo_snippets(client: &mut IpcClient) -> anyhow::Result<()> {
    let snippets: &[(&str, &str)] = &[
        (
            "thanks",
            "Thanks for the heads-up — really appreciate the context.",
        ),
        (
            "intro",
            "Quick intro: {{name}} on my team has been working on {{topic}}. I'll let them take it from here.",
        ),
        (
            "decline-politely",
            "Thanks for thinking of me. I can't take this on right now, but happy to revisit next quarter if it's still open.",
        ),
        (
            "oof",
            "I'm out of office through {{date}} with limited access to email. For anything urgent, please reach {{backup}}.",
        ),
        (
            "nudge",
            "Bumping this — wanted to make sure it didn't get lost in the shuffle. Any thoughts when you get a moment?",
        ),
    ];
    for (name, body) in snippets {
        let vars: Vec<String> = body
            .match_indices("{{")
            .filter_map(|(start, _)| {
                let after = &body[start + 2..];
                after.find("}}").map(|end| after[..end].trim().to_string())
            })
            .collect();
        match client
            .request(Request::SetSnippet {
                name: (*name).to_string(),
                body: (*body).to_string(),
                vars,
            })
            .await?
        {
            Response::Ok { .. } => {}
            Response::Error { message, .. } => {
                anyhow::bail!("failed to seed snippet `{name}`: {message}");
            }
        }
    }
    Ok(())
}

async fn seed_demo_signatures(client: &mut IpcClient) -> anyhow::Result<()> {
    let signatures: &[(&str, &str)] = &[
        ("formal", "Best regards,\nAlex Demo\nDemo Co. — Engineering"),
        ("casual", "—\nAlex"),
    ];
    for (name, body) in signatures {
        match client
            .request(Request::SetSignature {
                name: (*name).to_string(),
                body: (*body).to_string(),
            })
            .await?
        {
            Response::Ok { .. } => {}
            Response::Error { message, .. } => {
                anyhow::bail!("failed to seed signature `{name}`: {message}");
            }
        }
    }
    Ok(())
}

async fn seed_demo_labels(client: &mut IpcClient) -> anyhow::Result<()> {
    let labels: &[(&str, Option<&str>)] = &[
        ("family", Some("#ff8a5b")),
        ("priority-q4", Some("#ffd166")),
        ("read-after-lunch", Some("#06d6a0")),
    ];
    for (name, color) in labels {
        match client
            .request(Request::CreateLabel {
                name: (*name).to_string(),
                color: color.map(std::string::ToString::to_string),
                account_id: None,
            })
            .await?
        {
            Response::Ok { .. } => {}
            // Soft-tolerate "already exists" since CreateLabel is not always
            // idempotent at the provider layer (the fake provider should be,
            // but real providers reject duplicates).
            Response::Error { message, .. } if message.to_ascii_lowercase().contains("exist") => {}
            Response::Error { message, .. } => {
                anyhow::bail!("failed to seed label `{name}`: {message}");
            }
        }
    }
    Ok(())
}

async fn seed_demo_saved_searches(client: &mut IpcClient) -> anyhow::Result<()> {
    let entries: &[(&str, &str)] = &[
        ("Unread today", "is:unread date:today"),
        ("From boss", "from:alice@work.com"),
        ("Owed replies", "is:owed"),
        ("Build alerts", "subject:\"build failed\""),
    ];
    for (name, query) in entries {
        match client
            .request(Request::CreateSavedSearch {
                name: (*name).to_string(),
                query: (*query).to_string(),
                search_mode: mxr_core::types::SearchMode::Lexical,
            })
            .await?
        {
            Response::Ok { .. } => {}
            // CreateSavedSearch is keyed by name; a duplicate is expected on
            // re-run and is not an error worth surfacing.
            Response::Error { message, .. }
                if message.to_ascii_lowercase().contains("exist")
                    || message.to_ascii_lowercase().contains("unique") => {}
            Response::Error { message, .. } => {
                anyhow::bail!("failed to seed saved search `{name}`: {message}");
            }
        }
    }
    Ok(())
}

async fn seed_demo_screener(client: &mut IpcClient) -> anyhow::Result<()> {
    // Mix of dispositions so the triage screen shows variety. Senders are
    // pulled from the fake provider's generator pool — see
    // `crates/provider-fake/` for the source list.
    let personal = AccountId::from_provider_id("fake", DEMO_PERSONAL_EMAIL);
    let entries: &[(
        &AccountId,
        &str,
        mxr_protocol::ScreenerDispositionData,
        Option<&str>,
    )] = &[
        (
            &personal,
            "alice@work.com",
            mxr_protocol::ScreenerDispositionData::Allow,
            None,
        ),
        (
            &personal,
            "bob@work.com",
            mxr_protocol::ScreenerDispositionData::PaperTrail,
            Some("paper-trail"),
        ),
        (
            &personal,
            "carol@work.com",
            mxr_protocol::ScreenerDispositionData::PaperTrail,
            Some("paper-trail"),
        ),
        (
            &personal,
            "diana@work.com",
            mxr_protocol::ScreenerDispositionData::Feed,
            Some("feed"),
        ),
        (
            &personal,
            "eve@zhang.com",
            mxr_protocol::ScreenerDispositionData::Deny,
            None,
        ),
    ];
    for (account_id, sender_email, disposition, route_label) in entries {
        match client
            .request(Request::SetScreenerDecision {
                account_id: (*account_id).clone(),
                sender_email: (*sender_email).to_string(),
                disposition: *disposition,
                route_label: route_label.map(std::string::ToString::to_string),
            })
            .await?
        {
            Response::Ok { .. } => {}
            Response::Error { message, .. } => {
                anyhow::bail!("failed to seed screener decision `{sender_email}`: {message}");
            }
        }
    }
    Ok(())
}

async fn seed_demo_message_state(client: &mut IpcClient) -> anyhow::Result<()> {
    // Pull the latest 20 envelopes from the personal demo account so we have
    // stable IDs to snooze / reply-later flag.
    let personal = AccountId::from_provider_id("fake", DEMO_PERSONAL_EMAIL);
    let envelopes = match client
        .request(Request::ListEnvelopes {
            label_id: None,
            account_id: Some(personal),
            limit: 20,
            offset: 0,
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        } => envelopes,
        Response::Error { message, .. } => {
            anyhow::bail!("failed to list demo envelopes: {message}");
        }
        other => anyhow::bail!("Unexpected envelope list response: {other:?}"),
    };

    if envelopes.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now();
    // Snooze the first 5 (mixed wake times) so the snoozed queue is non-empty.
    let snooze_waketimes = [
        chrono::Duration::days(1),
        chrono::Duration::days(2),
        chrono::Duration::days(3),
        chrono::Duration::weeks(1),
        chrono::Duration::weeks(2),
    ];
    for (envelope, offset) in envelopes.iter().zip(snooze_waketimes.iter()).take(5) {
        let wake_at = now + *offset;
        if let Response::Error { message, .. } = client
            .request(Request::Snooze {
                message_id: envelope.id.clone(),
                wake_at,
            })
            .await?
        {
            // Already-snoozed is OK; keep going.
            if !message.to_ascii_lowercase().contains("snooz") {
                println!("    snooze skipped for {}: {message}", envelope.id);
            }
        }
    }

    // Flag a different 5 for reply-later.
    for envelope in envelopes.iter().skip(5).take(5) {
        if let Response::Error { message, .. } = client
            .request(Request::SetReplyLater {
                message_id: envelope.id.clone(),
                flag: true,
            })
            .await?
        {
            println!("    reply-later skipped for {}: {message}", envelope.id);
        }
    }

    Ok(())
}

/// Trigger one-shot rebuilds for LLM-backed caches so summarize / decisions /
/// commitments / relationship-profile / voice surfaces aren't empty on first
/// click. The DemoLlmProvider answers all the underlying LLM calls, so this
/// runs entirely offline.
///
/// Called from `prewarm_demo_runtime` after the analytics + semantic prewarm
/// so it benefits from the synced + indexed mailbox.
async fn prewarm_llm_caches(client: &mut IpcClient) -> anyhow::Result<()> {
    for account_id in demo_account_ids() {
        if let Err(error) = prewarm_user_voice(client, &account_id).await {
            println!("  voice rebuild skipped for {account_id}: {error}");
        }
        if let Err(error) = prewarm_decision_log(client, &account_id).await {
            println!("  decision-log rebuild skipped for {account_id}: {error}");
        }
    }
    Ok(())
}

async fn prewarm_user_voice(client: &mut IpcClient, account_id: &AccountId) -> anyhow::Result<()> {
    match client
        .request(Request::RebuildUserVoice {
            account_id: account_id.clone(),
        })
        .await?
    {
        Response::Ok { .. } => Ok(()),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn prewarm_decision_log(
    client: &mut IpcClient,
    account_id: &AccountId,
) -> anyhow::Result<()> {
    match client
        .request(Request::RebuildDecisionLog {
            account_id: account_id.clone(),
            since_days: 365,
        })
        .await?
    {
        Response::Ok { .. } => Ok(()),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn seed_demo_drafts(client: &mut IpcClient) -> anyhow::Result<()> {
    use mxr_core::id::DraftId;
    use mxr_core::types::{Address, Draft, DraftIntent};

    let personal = AccountId::from_provider_id("fake", DEMO_PERSONAL_EMAIL);
    let now = chrono::Utc::now();
    let drafts: Vec<Draft> = vec![
        Draft {
            id: DraftId::from_provider_id("demo", "draft-q4-roadmap"),
            account_id: personal.clone(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: Some("Alice".to_string()),
                email: "alice@work.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Q4 roadmap — first pass".to_string(),
            body_markdown: "Hey Alice,\n\nQuick draft of the Q4 roadmap before the planning meeting. The big rocks:\n\n1. Ship v0.6 by end of October.\n2. Migration window in mid-November.\n3. Search-perf cleanup before the holiday freeze.\n\nGrab me before the standup if you want to push anything around.\n\n— Alex".to_string(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: now - chrono::Duration::hours(3),
            updated_at: now - chrono::Duration::hours(3),
        },
        Draft {
            id: DraftId::from_provider_id("demo", "draft-perf-followup"),
            account_id: personal,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: Some("Diana".to_string()),
                email: "diana@work.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "perf regression — follow-up".to_string(),
            body_markdown: "Diana,\n\nWanted to circle back on the perf regression you flagged. Did you get a chance to repro on the latest cut? If it's still there I'll carve out time on Thursday to dig in with you.\n\n— Alex".to_string(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: now - chrono::Duration::hours(1),
            updated_at: now - chrono::Duration::minutes(20),
        },
    ];

    for draft in drafts {
        if let Response::Error { message, .. } =
            client.request(Request::SaveDraft { draft }).await?
        {
            // SaveDraft is idempotent on `draft.id`; a "duplicate" warning is
            // not a hard failure.
            if !message.to_ascii_lowercase().contains("exist") {
                println!("    draft save skipped: {message}");
            }
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

    // Flip LLM on in the demo config so handlers like summarize / briefing /
    // draft-assist don't short-circuit before the runtime can swap in
    // `DemoLlmProvider`. The provider itself ignores config.llm.base_url and
    // api_key_env when running in demo mode (`build_llm_provider` shortcuts
    // on `is_demo_instance`), so no real network calls are ever made.
    config.llm.enabled = true;
    config.llm.model = "mxr-demo-canned".to_string();

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
    use super::{
        demo_account_ids, demo_paths, is_active, prepare_environment, read_active_marker,
        read_demo_message_count, remove_active_marker, write_active_marker,
        write_demo_message_count, DemoPaths, DEMO_COUNT_MARKER, DEMO_INSTANCE, DEMO_PERSONAL_EMAIL,
        DEMO_SEED_VERSION, DEMO_WORK_EMAIL,
    };
    use mxr_core::id::AccountId;

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

    /// The sticky-demo marker round-trip: writing then reading via the file
    /// system yields the same message count, and the marker lives outside
    /// the demo data dir so it survives independent of MXR_INSTANCE.
    #[test]
    fn active_marker_round_trips() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        // Redirect config_dir via XDG/HOME so active_marker_path() lands in
        // our temp dir instead of the user's real config.
        temp_env::with_vars(
            [
                ("XDG_CONFIG_HOME", Some(temp_dir.path().as_os_str())),
                ("HOME", Some(temp_dir.path().as_os_str())),
            ],
            || {
                let _ = remove_active_marker();
                assert!(!is_active());
                write_active_marker(12_345).expect("write marker");
                assert!(is_active());
                assert_eq!(read_active_marker(), Some(12_345));
                remove_active_marker().expect("remove marker");
                assert!(!is_active());
            },
        );
    }
}
