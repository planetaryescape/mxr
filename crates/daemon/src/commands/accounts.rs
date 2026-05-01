use crate::cli::AccountsAction;
use crate::ipc_client::IpcClient;
use crate::state::AppState;
use mxr_core::provider::MailSyncProvider;
use mxr_provider_gmail::auth::{GmailAuth, BUNDLED_CLIENT_ID, BUNDLED_CLIENT_SECRET};
use mxr_search::SearchIndex;
use mxr_store::Store;
use std::time::Duration;

pub async fn run(action: Option<AccountsAction>) -> anyhow::Result<()> {
    match action {
        None => {
            let config = mxr_config::load_config().unwrap_or_default();
            if config.accounts.is_empty() {
                println!("No accounts configured.");
                println!("Run: mxr accounts add gmail|imap|smtp|imap-smtp");
            } else {
                for (key, acct) in &config.accounts {
                    println!(
                        "  {} - {} <{}> [sync: {}, send: {}, {}]",
                        key,
                        acct.name,
                        acct.email,
                        describe_sync(acct.sync.as_ref()),
                        describe_send(acct.send.as_ref()),
                        if acct.enabled { "enabled" } else { "disabled" }
                    );
                }
            }
        }
        Some(AccountsAction::Add { provider }) => match provider.as_str() {
            "gmail" => add_gmail().await?,
            "imap" => add_imap(true).await?,
            "imap-smtp" => add_imap(true).await?,
            "smtp" => add_smtp_only().await?,
            other => anyhow::bail!(
                "Unknown provider '{}'. Supported: gmail, imap, smtp, imap-smtp",
                other
            ),
        },
        Some(AccountsAction::Show { name }) => {
            let config = mxr_config::load_config().unwrap_or_default();
            match config.accounts.get(&name) {
                Some(acct) => {
                    println!("Name:  {}", acct.name);
                    println!("Email: {}", acct.email);
                    println!("Enabled: {}", acct.enabled);
                    println!("Sync:  {}", describe_sync(acct.sync.as_ref()));
                    println!("Send:  {}", describe_send(acct.send.as_ref()));
                }
                None => anyhow::bail!("Account '{}' not found", name),
            }
        }
        Some(AccountsAction::Test { name }) => {
            let config = mxr_config::load_config().unwrap_or_default();
            let acct = config
                .accounts
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", name))?;

            if let Some(sync) = &acct.sync {
                match sync {
                    mxr_config::SyncProviderConfig::Gmail {
                        credential_source: _,
                        client_id,
                        client_secret,
                        token_ref,
                    } => {
                        let secret = client_secret.as_deref().unwrap_or("");
                        let mut auth = GmailAuth::new(
                            client_id.clone(),
                            secret.to_string(),
                            token_ref.clone(),
                        );
                        auth.load_existing().await?;
                        let gmail_client = mxr_provider_gmail::client::GmailClient::new(auth);
                        let count = gmail_client
                            .list_labels()
                            .await?
                            .labels
                            .map(|labels| labels.len())
                            .unwrap_or(0);
                        println!("Gmail sync ok for '{}': {} labels", name, count);
                    }
                    mxr_config::SyncProviderConfig::Imap {
                        host,
                        port,
                        username,
                        password_ref,
                        auth_required,
                        use_tls,
                    } => {
                        let account_id = mxr_core::AccountId::from_provider_id("imap", &acct.email);
                        let provider = mxr_provider_imap::ImapProvider::new(
                            account_id,
                            mxr_provider_imap::config::ImapConfig::new(
                                host.clone(),
                                *port,
                                username.clone(),
                                password_ref.clone(),
                                *auth_required,
                                *use_tls,
                            ),
                        );
                        let folders = provider.sync_labels().await?;
                        println!("IMAP sync ok for '{}': {} folders", name, folders.len());
                    }
                }
            }

            if let Some(send) = &acct.send {
                match send {
                    mxr_config::SendProviderConfig::Gmail => {
                        println!("Gmail send ok for '{}'", name);
                    }
                    mxr_config::SendProviderConfig::Smtp {
                        host,
                        port,
                        username,
                        password_ref,
                        auth_required,
                        use_tls,
                    } => {
                        let provider = mxr_provider_smtp::SmtpSendProvider::new(
                            mxr_provider_smtp::config::SmtpConfig::new(
                                host.clone(),
                                *port,
                                username.clone(),
                                password_ref.clone(),
                                *auth_required,
                                *use_tls,
                            ),
                        );
                        provider.test_connection().await?;
                        println!("SMTP send ok for '{}'", name);
                    }
                }
            }
        }
        Some(AccountsAction::Repair { name }) => {
            repair_account_passwords(&name)?;
        }
        Some(AccountsAction::Disable { name }) => {
            disable_account(&name).await?;
        }
        Some(AccountsAction::Remove {
            name,
            dry_run,
            yes,
            purge_local_data,
        }) => {
            remove_account(&name, dry_run, yes, purge_local_data).await?;
        }
    }
    Ok(())
}

async fn add_gmail() -> anyhow::Result<()> {
    println!("Adding Gmail account\n");

    let (credential_source, client_id, client_secret) =
        match (BUNDLED_CLIENT_ID, BUNDLED_CLIENT_SECRET) {
            (Some(id), Some(secret)) => {
                println!("Using bundled OAuth credentials.");
                (
                    mxr_config::GmailCredentialSource::Bundled,
                    id.to_string(),
                    secret.to_string(),
                )
            }
            _ => {
                println!(
                    "No bundled OAuth credentials. You'll need your own Google Cloud project."
                );
                println!(
                    "See: https://console.cloud.google.com/apis/library/gmail.googleapis.com\n"
                );
                let id = prompt("Client ID: ")?;
                let secret = prompt("Client Secret: ")?;
                (mxr_config::GmailCredentialSource::Custom, id, secret)
            }
        };

    let account_name = prompt("\nAccount name (e.g. personal, work): ")?;
    let email = prompt("Gmail address: ")?;
    ensure_account_available(&account_name)?;
    let token_ref = format!("mxr/{account_name}-gmail");

    println!("\nOpening browser for Google authorization...");
    let mut auth = GmailAuth::new(client_id.clone(), client_secret.clone(), token_ref.clone());
    auth.interactive_auth().await?;
    println!("Authorization successful!\n");

    upsert_account(
        account_name.clone(),
        mxr_config::AccountConfig {
            name: account_name.clone(),
            email,
            enabled: true,
            sync: Some(mxr_config::SyncProviderConfig::Gmail {
                credential_source,
                client_id,
                client_secret: Some(client_secret),
                token_ref,
            }),
            send: Some(mxr_config::SendProviderConfig::Gmail),
        },
    )?;

    println!(
        "Account '{}' saved. Restart daemon to load it.",
        account_name
    );
    Ok(())
}

async fn add_imap(include_smtp: bool) -> anyhow::Result<()> {
    println!("Adding IMAP account\n");
    let account_name = prompt("Account name: ")?;
    ensure_account_available(&account_name)?;
    let display_name = prompt_default("Display name", &account_name)?;
    let email = prompt("Email address: ")?;

    let imap_host = prompt("IMAP host: ")?;
    let imap_port = prompt_default("IMAP port", "993")?.parse::<u16>()?;
    let imap_auth_required = prompt_bool("IMAP requires authentication", true)?;
    let (imap_username, imap_password_ref) = if imap_auth_required {
        let imap_username = prompt_default("IMAP username", &email)?;
        let imap_password = prompt_secret("IMAP password: ")?;
        let imap_password_ref = format!("mxr/{account_name}-imap");
        store_password(&imap_password_ref, &imap_username, &imap_password)?;
        (imap_username, imap_password_ref)
    } else {
        (String::new(), String::new())
    };

    let send = if include_smtp {
        let smtp_host = prompt("SMTP host: ")?;
        let smtp_port = prompt_default("SMTP port", "587")?.parse::<u16>()?;
        let smtp_auth_required = prompt_bool("SMTP requires authentication", true)?;
        let (smtp_username, smtp_password_ref) = if smtp_auth_required {
            let smtp_username = prompt_default("SMTP username", &email)?;
            let smtp_password = prompt_secret("SMTP password: ")?;
            let smtp_password_ref = format!("mxr/{account_name}-smtp");
            store_password(&smtp_password_ref, &smtp_username, &smtp_password)?;
            (smtp_username, smtp_password_ref)
        } else {
            (String::new(), String::new())
        };
        Some(mxr_config::SendProviderConfig::Smtp {
            host: smtp_host,
            port: smtp_port,
            username: smtp_username,
            password_ref: smtp_password_ref,
            auth_required: smtp_auth_required,
            use_tls: true,
        })
    } else {
        None
    };

    upsert_account(
        account_name.clone(),
        mxr_config::AccountConfig {
            name: display_name,
            email,
            enabled: true,
            sync: Some(mxr_config::SyncProviderConfig::Imap {
                host: imap_host,
                port: imap_port,
                username: imap_username,
                password_ref: imap_password_ref,
                auth_required: imap_auth_required,
                use_tls: true,
            }),
            send,
        },
    )?;

    println!(
        "Account '{}' saved. Restart daemon to load it.",
        account_name
    );
    Ok(())
}

async fn add_smtp_only() -> anyhow::Result<()> {
    println!("Adding SMTP-only account\n");
    let account_name = prompt("Account name: ")?;
    ensure_account_available(&account_name)?;
    let display_name = prompt_default("Display name", &account_name)?;
    let email = prompt("Email address: ")?;
    let smtp_host = prompt("SMTP host: ")?;
    let smtp_port = prompt_default("SMTP port", "587")?.parse::<u16>()?;
    let smtp_auth_required = prompt_bool("SMTP requires authentication", true)?;
    let (smtp_username, smtp_password_ref) = if smtp_auth_required {
        let smtp_username = prompt_default("SMTP username", &email)?;
        let smtp_password = prompt_secret("SMTP password: ")?;
        let smtp_password_ref = format!("mxr/{account_name}-smtp");
        store_password(&smtp_password_ref, &smtp_username, &smtp_password)?;
        (smtp_username, smtp_password_ref)
    } else {
        (String::new(), String::new())
    };

    upsert_account(
        account_name.clone(),
        mxr_config::AccountConfig {
            name: display_name,
            email,
            enabled: true,
            sync: None,
            send: Some(mxr_config::SendProviderConfig::Smtp {
                host: smtp_host,
                port: smtp_port,
                username: smtp_username,
                password_ref: smtp_password_ref,
                auth_required: smtp_auth_required,
                use_tls: true,
            }),
        },
    )?;

    println!(
        "Account '{}' saved. Restart daemon to load it.",
        account_name
    );
    Ok(())
}

fn upsert_account(name: String, account: mxr_config::AccountConfig) -> anyhow::Result<()> {
    let mut config = mxr_config::load_config().unwrap_or_default();
    config.accounts.insert(name.clone(), account);
    if config.general.default_account.is_none() {
        config.general.default_account = Some(name);
    }
    mxr_config::save_config(&config)?;
    Ok(())
}

async fn disable_account(name: &str) -> anyhow::Result<()> {
    let mut config = mxr_config::load_config().unwrap_or_default();
    let account = config
        .accounts
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", name))?;
    account.enabled = false;
    let account_id = account_id_for_config(account);
    refresh_default_account(&mut config);
    mxr_config::save_config(&config)?;
    set_db_account_enabled(&account_id, false).await?;
    restart_daemon_if_running().await?;
    println!("Disabled account '{}'.", name);
    Ok(())
}

async fn remove_account(
    name: &str,
    dry_run: bool,
    yes: bool,
    purge_local_data: bool,
) -> anyhow::Result<()> {
    let mut config = mxr_config::load_config().unwrap_or_default();
    let account = config
        .accounts
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", name))?
        .clone();
    let account_id = account_id_for_config(&account);
    let cached_message_count = cached_message_count(&account_id).await.unwrap_or(0);

    if dry_run {
        println!(
            "Would remove account '{}' from config and {} {} cached message(s).",
            name,
            if purge_local_data { "purge" } else { "detach" },
            cached_message_count
        );
        return Ok(());
    }

    if purge_local_data
        && !yes
        && !prompt_bool(
            &format!(
                "Permanently purge {} cached message(s) for '{}'",
                cached_message_count, name
            ),
            false,
        )?
    {
        anyhow::bail!("Aborted");
    }

    let daemon_was_running = if purge_local_data {
        shutdown_daemon_if_running().await?
    } else {
        false
    };

    config.accounts.remove(name);
    refresh_default_account(&mut config);
    mxr_config::save_config(&config)?;
    if purge_local_data {
        purge_db_account(&account_id).await?;
        restart_daemon_after_change(daemon_was_running).await?;
    } else {
        set_db_account_enabled(&account_id, false).await?;
        restart_daemon_if_running().await?;
    }
    if purge_local_data {
        println!(
            "Removed account '{}' and purged {} cached message(s).",
            name, cached_message_count
        );
    } else {
        println!(
            "Removed account '{}' from config; cached mail detached.",
            name
        );
    }
    Ok(())
}

fn account_id_for_config(account: &mxr_config::AccountConfig) -> mxr_core::AccountId {
    let provider = match account.sync.as_ref() {
        Some(mxr_config::SyncProviderConfig::Gmail { .. }) => "gmail",
        Some(mxr_config::SyncProviderConfig::Imap { .. }) => "imap",
        None => match account.send.as_ref() {
            Some(mxr_config::SendProviderConfig::Gmail) => "gmail",
            Some(mxr_config::SendProviderConfig::Smtp { .. }) => "smtp",
            None => "account",
        },
    };
    mxr_core::AccountId::from_provider_id(provider, &account.email)
}

fn refresh_default_account(config: &mut mxr_config::MxrConfig) {
    let current_default_is_enabled = config
        .general
        .default_account
        .as_ref()
        .and_then(|key| config.accounts.get(key))
        .map(|account| account.enabled)
        .unwrap_or(false);
    if current_default_is_enabled {
        return;
    }

    config.general.default_account = config
        .accounts
        .iter()
        .filter(|(_, account)| account.enabled)
        .map(|(key, _)| key.clone())
        .min();
}

async fn set_db_account_enabled(
    account_id: &mxr_core::AccountId,
    enabled: bool,
) -> anyhow::Result<()> {
    let store = open_store().await?;
    store.set_account_enabled(account_id, enabled).await?;
    Ok(())
}

async fn cached_message_count(account_id: &mxr_core::AccountId) -> anyhow::Result<usize> {
    let store = open_store().await?;
    Ok(store.list_message_ids_by_account(account_id).await?.len())
}

async fn purge_db_account(account_id: &mxr_core::AccountId) -> anyhow::Result<()> {
    let store = open_store().await?;
    let message_ids = store.list_message_ids_by_account(account_id).await?;
    remove_search_docs(&message_ids)?;
    store.delete_account(account_id).await?;
    remove_search_docs(&message_ids)?;
    Ok(())
}

async fn open_store() -> anyhow::Result<Store> {
    let data_dir = mxr_config::data_dir();
    std::fs::create_dir_all(&data_dir)?;
    Store::new(&data_dir.join("mxr.db"))
        .await
        .map_err(Into::into)
}

fn remove_search_docs(message_ids: &[mxr_core::MessageId]) -> anyhow::Result<()> {
    if message_ids.is_empty() {
        return Ok(());
    }
    let index_path = mxr_config::data_dir().join("search_index");
    std::fs::create_dir_all(&index_path)?;
    let mut index = SearchIndex::open(&index_path)?;
    for message_id in message_ids {
        index.remove_document(message_id);
    }
    index.commit()?;
    Ok(())
}

async fn restart_daemon_if_running() -> anyhow::Result<()> {
    restart_daemon_after_change(IpcClient::connect().await.is_ok()).await
}

async fn restart_daemon_after_change(was_running: bool) -> anyhow::Result<()> {
    if was_running {
        crate::server::restart_daemon().await?;
    }
    Ok(())
}

async fn shutdown_daemon_if_running() -> anyhow::Result<bool> {
    if IpcClient::connect().await.is_err() {
        return Ok(false);
    }

    let socket_path = AppState::socket_path();
    let state =
        crate::server::shutdown_daemon_for_maintenance(&socket_path, Duration::from_secs(3))
            .await?;
    if matches!(state, crate::server::SocketState::Reachable) {
        anyhow::bail!("Running daemon did not stop cleanly; account removal aborted");
    }
    Ok(true)
}

fn ensure_account_available(name: &str) -> anyhow::Result<()> {
    let config = mxr_config::load_config().unwrap_or_default();
    if config.accounts.contains_key(name) {
        anyhow::bail!("Account '{}' already exists", name);
    }
    Ok(())
}

fn describe_sync(sync: Option<&mxr_config::SyncProviderConfig>) -> &'static str {
    match sync {
        Some(mxr_config::SyncProviderConfig::Gmail { .. }) => "gmail",
        Some(mxr_config::SyncProviderConfig::Imap { .. }) => "imap",
        None => "none",
    }
}

fn describe_send(send: Option<&mxr_config::SendProviderConfig>) -> &'static str {
    match send {
        Some(mxr_config::SendProviderConfig::Gmail) => "gmail",
        Some(mxr_config::SendProviderConfig::Smtp { .. }) => "smtp",
        None => "none",
    }
}

fn store_password(service: &str, username: &str, password: &str) -> anyhow::Result<()> {
    mxr_keychain::set_password(service, username, password)?;
    Ok(())
}

fn repair_account_passwords(name: &str) -> anyhow::Result<()> {
    let config = mxr_config::load_config().unwrap_or_default();
    let account = config
        .accounts
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", name))?;

    let mut repaired_any = false;

    if let Some(mxr_config::SyncProviderConfig::Imap {
        username,
        password_ref,
        auth_required,
        ..
    }) = &account.sync
    {
        if *auth_required {
            let password = prompt_secret(&format!("IMAP password for {}: ", username))?;
            store_password(password_ref, username, &password)?;
            repaired_any = true;
        }
    }

    if let Some(mxr_config::SendProviderConfig::Smtp {
        username,
        password_ref,
        auth_required,
        ..
    }) = &account.send
    {
        if *auth_required {
            let password = prompt_secret(&format!("SMTP password for {}: ", username))?;
            store_password(password_ref, username, &password)?;
            repaired_any = true;
        }
    }

    if !repaired_any {
        anyhow::bail!(
            "Account '{}' has no password-backed IMAP/SMTP credentials to repair",
            name
        );
    }

    println!("Repaired keychain credentials for '{}'.", name);
    Ok(())
}

fn prompt(msg: &str) -> anyhow::Result<String> {
    use std::io::{self, Write};
    print!("{msg}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_default(msg: &str, default: &str) -> anyhow::Result<String> {
    let value = prompt(&format!("{msg} [{default}]: "))?;
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value)
    }
}

fn prompt_bool(msg: &str, default: bool) -> anyhow::Result<bool> {
    let default_label = if default { "Y/n" } else { "y/N" };
    let value = prompt(&format!("{msg}? [{default_label}]: "))?;
    if value.is_empty() {
        return Ok(default);
    }
    match value.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => anyhow::bail!("Please answer yes or no."),
    }
}

fn prompt_secret(msg: &str) -> anyhow::Result<String> {
    prompt(msg)
}
