use crate::cli::AccountsAction;
use mxr_core::provider::MailSyncProvider;
use mxr_provider_gmail::auth::{GmailAuth, BUNDLED_CLIENT_ID, BUNDLED_CLIENT_SECRET};

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
                        "  {} - {} <{}> [sync: {}, send: {}]",
                        key,
                        acct.name,
                        acct.email,
                        describe_sync(acct.sync.as_ref()),
                        describe_send(acct.send.as_ref())
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
                        use_tls,
                    } => {
                        let account_id = mxr_core::AccountId::from_provider_id("imap", &acct.email);
                        let provider = mxr_provider_imap::ImapProvider::new(
                            account_id,
                            mxr_provider_imap::config::ImapConfig {
                                host: host.clone(),
                                port: *port,
                                username: username.clone(),
                                password_ref: password_ref.clone(),
                                use_tls: *use_tls,
                            },
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
                        use_tls,
                    } => {
                        let provider = mxr_provider_smtp::SmtpSendProvider::new(
                            mxr_provider_smtp::config::SmtpConfig {
                                host: host.clone(),
                                port: *port,
                                username: username.clone(),
                                password_ref: password_ref.clone(),
                                use_tls: *use_tls,
                            },
                        );
                        provider.test_connection().await?;
                        println!("SMTP send ok for '{}'", name);
                    }
                }
            }
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
    let imap_username = prompt_default("IMAP username", &email)?;
    let imap_password = prompt_secret("IMAP password: ")?;
    let imap_password_ref = format!("mxr/{account_name}-imap");
    store_password(&imap_password_ref, &imap_username, &imap_password)?;

    let send = if include_smtp {
        let smtp_host = prompt("SMTP host: ")?;
        let smtp_port = prompt_default("SMTP port", "587")?.parse::<u16>()?;
        let smtp_username = prompt_default("SMTP username", &email)?;
        let smtp_password = prompt_secret("SMTP password: ")?;
        let smtp_password_ref = format!("mxr/{account_name}-smtp");
        store_password(&smtp_password_ref, &smtp_username, &smtp_password)?;
        Some(mxr_config::SendProviderConfig::Smtp {
            host: smtp_host,
            port: smtp_port,
            username: smtp_username,
            password_ref: smtp_password_ref,
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
            sync: Some(mxr_config::SyncProviderConfig::Imap {
                host: imap_host,
                port: imap_port,
                username: imap_username,
                password_ref: imap_password_ref,
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
    let smtp_username = prompt_default("SMTP username", &email)?;
    let smtp_password = prompt_secret("SMTP password: ")?;
    let smtp_password_ref = format!("mxr/{account_name}-smtp");
    store_password(&smtp_password_ref, &smtp_username, &smtp_password)?;

    upsert_account(
        account_name.clone(),
        mxr_config::AccountConfig {
            name: display_name,
            email,
            sync: None,
            send: Some(mxr_config::SendProviderConfig::Smtp {
                host: smtp_host,
                port: smtp_port,
                username: smtp_username,
                password_ref: smtp_password_ref,
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
    let entry = keyring::Entry::new(service, username)?;
    entry.set_password(password)?;
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

fn prompt_secret(msg: &str) -> anyhow::Result<String> {
    prompt(msg)
}
