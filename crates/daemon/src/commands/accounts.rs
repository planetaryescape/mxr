use crate::cli::AccountsAction;
use mxr_provider_gmail::auth::{GmailAuth, BUNDLED_CLIENT_ID, BUNDLED_CLIENT_SECRET};

pub async fn run(action: Option<AccountsAction>) -> anyhow::Result<()> {
    match action {
        None => {
            let config = mxr_config::load_config().unwrap_or_default();
            if config.accounts.is_empty() {
                println!("No accounts configured.");
                println!("Run: mxr accounts add gmail");
            } else {
                for (key, acct) in &config.accounts {
                    println!("  {} - {} <{}>", key, acct.name, acct.email);
                }
            }
        }
        Some(AccountsAction::Add { provider }) => match provider.as_str() {
            "gmail" => add_gmail().await?,
            other => anyhow::bail!("Unknown provider '{}'. Supported: gmail", other),
        },
        Some(AccountsAction::Show { name }) => {
            let config = mxr_config::load_config().unwrap_or_default();
            match config.accounts.get(&name) {
                Some(acct) => {
                    println!("Name:  {}", acct.name);
                    println!("Email: {}", acct.email);
                    println!(
                        "Sync:  {}",
                        if acct.sync.is_some() {
                            "configured"
                        } else {
                            "none"
                        }
                    );
                    println!(
                        "Send:  {}",
                        if acct.send.is_some() {
                            "configured"
                        } else {
                            "none"
                        }
                    );
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
            match &acct.sync {
                Some(mxr_config::SyncProviderConfig::Gmail {
                    client_id,
                    client_secret,
                    token_ref,
                }) => {
                    let secret = client_secret.as_deref().unwrap_or("");
                    let mut auth = GmailAuth::new(
                        client_id.clone(),
                        secret.to_string(),
                        token_ref.clone(),
                    );
                    match auth.load_existing().await {
                        Ok(()) => {
                            let gmail_client =
                                mxr_provider_gmail::client::GmailClient::new(auth);
                            match gmail_client.list_labels().await {
                                Ok(resp) => {
                                    let count =
                                        resp.labels.map(|l| l.len()).unwrap_or(0);
                                    println!(
                                        "Connected to '{}': {} labels found",
                                        name, count
                                    );
                                }
                                Err(e) => {
                                    println!("Connection failed for '{}': {}", name, e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Auth failed for '{}': {}", name, e);
                        }
                    }
                }
                None => {
                    println!("Account '{}' has no sync configuration", name);
                }
            }
        }
    }
    Ok(())
}

async fn add_gmail() -> anyhow::Result<()> {
    println!("Adding Gmail account\n");

    // Determine credentials: bundled or user-provided
    let (client_id, client_secret) = match (BUNDLED_CLIENT_ID, BUNDLED_CLIENT_SECRET) {
        (Some(id), Some(secret)) => {
            println!("Using bundled OAuth credentials.");
            (id.to_string(), secret.to_string())
        }
        _ => {
            println!("No bundled OAuth credentials. You'll need your own Google Cloud project.");
            println!("See: https://console.cloud.google.com/apis/library/gmail.googleapis.com\n");
            let id = prompt("Client ID: ")?;
            let secret = prompt("Client Secret: ")?;
            (id, secret)
        }
    };

    let account_name = prompt("\nAccount name (e.g. personal, work): ")?;
    let email = prompt("Gmail address: ")?;
    let token_ref = format!("{}-gmail", account_name);

    // Run OAuth flow
    println!("\nOpening browser for Google authorization...");
    let mut auth = GmailAuth::new(client_id.clone(), client_secret.clone(), token_ref.clone());
    auth.interactive_auth().await?;
    println!("Authorization successful!\n");

    // Write to config.toml
    let config_path = mxr_config::config_file_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let entry = format!(
        r#"
[accounts.{account_name}]
name = "{account_name}"
email = "{email}"

[accounts.{account_name}.sync]
type = "gmail"
client_id = "{client_id}"
client_secret = "{client_secret}"
token_ref = "{token_ref}"

[accounts.{account_name}.send]
type = "gmail"
"#
    );

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config_path)?;
    use std::io::Write;
    writeln!(file, "{entry}")?;

    println!("Config written to {}", config_path.display());
    println!("Run `mxr sync` to start syncing.");
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
