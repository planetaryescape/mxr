use crate::cli::AccountsAction;
use crate::mxr_core::provider::MailSyncProvider;
use crate::mxr_provider_gmail::auth::{GmailAuth, BUNDLED_CLIENT_ID, BUNDLED_CLIENT_SECRET};
use crate::mxr_provider_outlook::auth::BUNDLED_CLIENT_ID as OUTLOOK_BUNDLED_CLIENT_ID;

pub async fn run(action: Option<AccountsAction>) -> anyhow::Result<()> {
    match action {
        None => {
            let config = crate::mxr_config::load_config().unwrap_or_default();
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
            "outlook" => add_outlook().await?,
            "outlook-work" => add_outlook_work().await?,
            other => anyhow::bail!(
                "Unknown provider '{}'. Supported: gmail, imap, smtp, imap-smtp, outlook, outlook-work",
                other
            ),
        },
        Some(AccountsAction::Show { name }) => {
            let config = crate::mxr_config::load_config().unwrap_or_default();
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
            let config = crate::mxr_config::load_config().unwrap_or_default();
            let acct = config
                .accounts
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", name))?;

            if let Some(sync) = &acct.sync {
                match sync {
                    crate::mxr_config::SyncProviderConfig::Gmail {
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
                        let gmail_client =
                            crate::mxr_provider_gmail::client::GmailClient::new(auth);
                        let count = gmail_client
                            .list_labels()
                            .await?
                            .labels
                            .map(|labels| labels.len())
                            .unwrap_or(0);
                        println!("Gmail sync ok for '{}': {} labels", name, count);
                    }
                    crate::mxr_config::SyncProviderConfig::Imap {
                        host,
                        port,
                        username,
                        password_ref,
                        auth_required,
                        use_tls,
                    } => {
                        let account_id =
                            crate::mxr_core::AccountId::from_provider_id("imap", &acct.email);
                        let provider = crate::mxr_provider_imap::ImapProvider::new(
                            account_id,
                            crate::mxr_provider_imap::config::ImapConfig {
                                host: host.clone(),
                                port: *port,
                                username: username.clone(),
                                password_ref: password_ref.clone(),
                                auth_required: *auth_required,
                                use_tls: *use_tls,
                            },
                        );
                        let folders = provider.sync_labels().await?;
                        println!("IMAP sync ok for '{}': {} folders", name, folders.len());
                    }
                    crate::mxr_config::SyncProviderConfig::OutlookPersonal {
                        client_id,
                        token_ref,
                    }
                    | crate::mxr_config::SyncProviderConfig::OutlookWork {
                        client_id,
                        token_ref,
                    } => {
                        let tenant = match &acct.sync {
                            Some(crate::mxr_config::SyncProviderConfig::OutlookWork { .. }) => {
                                crate::mxr_provider_outlook::OutlookTenant::Work
                            }
                            _ => crate::mxr_provider_outlook::OutlookTenant::Personal,
                        };
                        let cid = client_id
                            .clone()
                            .or_else(|| OUTLOOK_BUNDLED_CLIENT_ID.map(String::from))
                            .ok_or_else(|| {
                                anyhow::anyhow!("no client_id and no bundled OUTLOOK_CLIENT_ID")
                            })?;
                        let auth = std::sync::Arc::new(
                            crate::mxr_provider_outlook::OutlookAuth::new(
                                cid,
                                token_ref.clone(),
                                tenant,
                            ),
                        );
                        let email = acct.email.clone();
                        let token_fn: std::sync::Arc<
                            dyn Fn() -> futures::future::BoxFuture<
                                'static,
                                anyhow::Result<String>,
                            > + Send
                                + Sync,
                        > = std::sync::Arc::new(move || {
                            let auth = auth.clone();
                            Box::pin(async move {
                                auth.get_valid_access_token()
                                    .await
                                    .map_err(|e| anyhow::anyhow!(e))
                            })
                        });
                        let account_id = crate::mxr_core::AccountId::from_provider_id(
                            "outlook",
                            &email,
                        );
                        let factory =
                            crate::mxr_provider_imap::XOAuth2ImapSessionFactory::new(
                                "outlook.office365.com".to_string(),
                                993,
                                email.clone(),
                                token_fn,
                            );
                        let provider =
                            crate::mxr_provider_imap::ImapProvider::with_session_factory(
                                account_id,
                                crate::mxr_provider_imap::config::ImapConfig {
                                    host: "outlook.office365.com".to_string(),
                                    port: 993,
                                    username: email,
                                    password_ref: String::new(),
                                    auth_required: true,
                                    use_tls: true,
                                },
                                Box::new(factory),
                            );
                        let folders = provider.sync_labels().await?;
                        println!(
                            "Outlook IMAP ok for '{}': {} folders",
                            name,
                            folders.len()
                        );
                    }
                }
            }

            if let Some(send) = &acct.send {
                match send {
                    crate::mxr_config::SendProviderConfig::OutlookPersonal { token_ref }
                    | crate::mxr_config::SendProviderConfig::OutlookWork { token_ref } => {
                        let tenant = match &acct.send {
                            Some(crate::mxr_config::SendProviderConfig::OutlookWork { .. }) => {
                                crate::mxr_provider_outlook::OutlookTenant::Work
                            }
                            _ => crate::mxr_provider_outlook::OutlookTenant::Personal,
                        };
                        let cid = match &acct.sync {
                            Some(
                                crate::mxr_config::SyncProviderConfig::OutlookPersonal {
                                    client_id: Some(id),
                                    ..
                                }
                                | crate::mxr_config::SyncProviderConfig::OutlookWork {
                                    client_id: Some(id),
                                    ..
                                },
                            ) => id.clone(),
                            _ => OUTLOOK_BUNDLED_CLIENT_ID
                                .map(String::from)
                                .ok_or_else(|| {
                                    anyhow::anyhow!("no client_id and no bundled OUTLOOK_CLIENT_ID")
                                })?,
                        };
                        let auth = std::sync::Arc::new(
                            crate::mxr_provider_outlook::OutlookAuth::new(
                                cid,
                                token_ref.clone(),
                                tenant,
                            ),
                        );
                        let email = acct.email.clone();
                        let token_fn: std::sync::Arc<
                            dyn Fn() -> futures::future::BoxFuture<
                                'static,
                                anyhow::Result<String>,
                            > + Send
                                + Sync,
                        > = std::sync::Arc::new(move || {
                            let auth = auth.clone();
                            Box::pin(async move {
                                auth.get_valid_access_token()
                                    .await
                                    .map_err(|e| anyhow::anyhow!(e))
                            })
                        });
                        let provider =
                            crate::mxr_provider_outlook::OutlookSmtpSendProvider::new(
                                "smtp.office365.com".to_string(),
                                587,
                                email,
                                token_fn,
                            );
                        provider
                            .test_connection()
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                        println!("Outlook SMTP ok for '{}'", name);
                    }
                    crate::mxr_config::SendProviderConfig::Gmail => {
                        println!("Gmail send ok for '{}'", name);
                    }
                    crate::mxr_config::SendProviderConfig::Smtp {
                        host,
                        port,
                        username,
                        password_ref,
                        auth_required,
                        use_tls,
                    } => {
                        let provider = crate::mxr_provider_smtp::SmtpSendProvider::new(
                            crate::mxr_provider_smtp::config::SmtpConfig {
                                host: host.clone(),
                                port: *port,
                                username: username.clone(),
                                password_ref: password_ref.clone(),
                                auth_required: *auth_required,
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
                    crate::mxr_config::GmailCredentialSource::Bundled,
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
                (crate::mxr_config::GmailCredentialSource::Custom, id, secret)
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
        crate::mxr_config::AccountConfig {
            name: account_name.clone(),
            email,
            sync: Some(crate::mxr_config::SyncProviderConfig::Gmail {
                credential_source,
                client_id,
                client_secret: Some(client_secret),
                token_ref,
            }),
            send: Some(crate::mxr_config::SendProviderConfig::Gmail),
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
        Some(crate::mxr_config::SendProviderConfig::Smtp {
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
        crate::mxr_config::AccountConfig {
            name: display_name,
            email,
            sync: Some(crate::mxr_config::SyncProviderConfig::Imap {
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
        crate::mxr_config::AccountConfig {
            name: display_name,
            email,
            sync: None,
            send: Some(crate::mxr_config::SendProviderConfig::Smtp {
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

fn upsert_account(name: String, account: crate::mxr_config::AccountConfig) -> anyhow::Result<()> {
    let mut config = crate::mxr_config::load_config().unwrap_or_default();
    config.accounts.insert(name.clone(), account);
    if config.general.default_account.is_none() {
        config.general.default_account = Some(name);
    }
    crate::mxr_config::save_config(&config)?;
    Ok(())
}

fn ensure_account_available(name: &str) -> anyhow::Result<()> {
    let config = crate::mxr_config::load_config().unwrap_or_default();
    if config.accounts.contains_key(name) {
        anyhow::bail!("Account '{}' already exists", name);
    }
    Ok(())
}

fn describe_sync(sync: Option<&crate::mxr_config::SyncProviderConfig>) -> &'static str {
    match sync {
        Some(crate::mxr_config::SyncProviderConfig::Gmail { .. }) => "gmail",
        Some(crate::mxr_config::SyncProviderConfig::Imap { .. }) => "imap",
        Some(crate::mxr_config::SyncProviderConfig::OutlookPersonal { .. }) => "outlook",
        Some(crate::mxr_config::SyncProviderConfig::OutlookWork { .. }) => "outlook-work",
        None => "none",
    }
}

fn describe_send(send: Option<&crate::mxr_config::SendProviderConfig>) -> &'static str {
    match send {
        Some(crate::mxr_config::SendProviderConfig::Gmail) => "gmail",
        Some(crate::mxr_config::SendProviderConfig::Smtp { .. }) => "smtp",
        Some(crate::mxr_config::SendProviderConfig::OutlookPersonal { .. }) => "outlook",
        Some(crate::mxr_config::SendProviderConfig::OutlookWork { .. }) => "outlook-work",
        None => "none",
    }
}

async fn add_outlook() -> anyhow::Result<()> {
    add_outlook_inner(crate::mxr_provider_outlook::OutlookTenant::Personal).await
}

async fn add_outlook_work() -> anyhow::Result<()> {
    add_outlook_inner(crate::mxr_provider_outlook::OutlookTenant::Work).await
}

async fn add_outlook_inner(
    tenant: crate::mxr_provider_outlook::OutlookTenant,
) -> anyhow::Result<()> {
    let label = match tenant {
        crate::mxr_provider_outlook::OutlookTenant::Personal => "Outlook (Personal)",
        crate::mxr_provider_outlook::OutlookTenant::Work => "Outlook (Work)",
    };
    println!("Adding {label} account (OAuth2 + IMAP + SMTP)\n");

    let client_id = match OUTLOOK_BUNDLED_CLIENT_ID {
        Some(id) => {
            println!("Using bundled Azure app credentials.");
            id.to_string()
        }
        None => {
            println!("No bundled Azure app client ID found.");
            println!(
                "Register a multi-tenant public client app at https://portal.azure.com and enter your client ID below."
            );
            prompt("Azure app client ID: ")?
        }
    };

    let account_name = prompt("\nAccount name (e.g. personal, work): ")?;
    ensure_account_available(&account_name)?;
    let display_name = prompt_default("Display name", &account_name)?;
    let email = prompt("Microsoft email address: ")?;

    let token_ref = format!("mxr/{account_name}-outlook");
    let auth = crate::mxr_provider_outlook::OutlookAuth::new(
        client_id.clone(),
        token_ref.clone(),
        tenant,
    );

    println!("\nStarting Microsoft device code authorization...");
    let device_resp = auth.start_device_flow().await?;

    println!(
        "\nGo to {} and enter: {}",
        device_resp.verification_uri, device_resp.user_code
    );
    let open_url = device_resp
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device_resp.verification_uri);
    if open::that(open_url).is_ok() {
        println!("(Browser opened automatically)");
    } else {
        println!("(Could not open browser — copy the URL above)");
    }
    println!(
        "Waiting for authorization (expires in {} seconds)...\n",
        device_resp.expires_in
    );

    let tokens = auth
        .poll_for_token(&device_resp.device_code, device_resp.interval)
        .await?;
    auth.save_tokens(&tokens)?;
    println!("Authorization successful!\n");

    let client_id_stored = if OUTLOOK_BUNDLED_CLIENT_ID.is_some() {
        None
    } else {
        Some(client_id)
    };
    let (sync_config, send_config) = match auth.tenant_kind() {
        crate::mxr_provider_outlook::OutlookTenant::Work => (
            crate::mxr_config::SyncProviderConfig::OutlookWork {
                client_id: client_id_stored,
                token_ref: token_ref.clone(),
            },
            crate::mxr_config::SendProviderConfig::OutlookWork {
                token_ref: token_ref.clone(),
            },
        ),
        crate::mxr_provider_outlook::OutlookTenant::Personal => (
            crate::mxr_config::SyncProviderConfig::OutlookPersonal {
                client_id: client_id_stored,
                token_ref: token_ref.clone(),
            },
            crate::mxr_config::SendProviderConfig::OutlookPersonal {
                token_ref: token_ref.clone(),
            },
        ),
    };

    upsert_account(
        account_name.clone(),
        crate::mxr_config::AccountConfig {
            name: display_name,
            email,
            sync: Some(sync_config),
            send: Some(send_config),
        },
    )?;

    println!(
        "Account '{}' saved. Restart daemon to load it.",
        account_name
    );
    Ok(())
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
