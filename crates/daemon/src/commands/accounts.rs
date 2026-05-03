use crate::cli::{AccountsAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{
    AccountConfigData, AccountOperationResult, AccountSendConfigData, AccountSyncConfigData,
    GmailCredentialSourceData, Request, Response, ResponseData,
};

pub async fn run(
    action: Option<AccountsAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match action {
        None => list_accounts(format).await?,
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
        Some(AccountsAction::Show { name }) => show_account(&name).await?,
        Some(AccountsAction::Test { name }) => test_account(&name).await?,
        Some(AccountsAction::Repair { name }) => repair_account(&name).await?,
        Some(AccountsAction::Disable { name }) => {
            run_account_operation(disable_account_request(&name)).await?
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

async fn client() -> anyhow::Result<IpcClient> {
    crate::server::ensure_daemon_running().await?;
    IpcClient::connect().await
}

async fn list_accounts(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let accounts = account_configs().await?;
    let summaries: Vec<AccountSummaryRow> =
        accounts.iter().map(AccountSummaryRow::from).collect();

    match resolve_format(format) {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&summaries)?);
        }
        OutputFormat::Jsonl => {
            println!("{}", jsonl(&summaries)?);
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["key", "name", "email", "sync", "send", "enabled"])?;
            for row in &summaries {
                writer.write_record([
                    row.key.as_str(),
                    row.name.as_str(),
                    row.email.as_str(),
                    row.sync,
                    row.send,
                    if row.enabled { "true" } else { "false" },
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for row in &summaries {
                println!("{}", row.key);
            }
        }
        OutputFormat::Table => {
            if summaries.is_empty() {
                println!("No accounts configured.");
                println!("Run: mxr accounts add gmail|imap|smtp|imap-smtp");
                return Ok(());
            }
            for row in &summaries {
                println!(
                    "  {} - {} <{}> [sync: {}, send: {}, {}]",
                    row.key,
                    row.name,
                    row.email,
                    row.sync,
                    row.send,
                    if row.enabled { "enabled" } else { "disabled" }
                );
            }
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct AccountSummaryRow {
    key: String,
    name: String,
    email: String,
    sync: &'static str,
    send: &'static str,
    enabled: bool,
}

impl From<&AccountConfigData> for AccountSummaryRow {
    fn from(account: &AccountConfigData) -> Self {
        Self {
            key: account.key.clone(),
            name: account.name.clone(),
            email: account.email.clone(),
            sync: describe_sync_data(account.sync.as_ref()),
            send: describe_send_data(account.send.as_ref()),
            enabled: account.enabled,
        }
    }
}

async fn show_account(name: &str) -> anyhow::Result<()> {
    let acct = find_account_config(name).await?;
    println!("Name:  {}", acct.name);
    println!("Email: {}", acct.email);
    println!("Enabled: {}", acct.enabled);
    println!("Sync:  {}", describe_sync_data(acct.sync.as_ref()));
    println!("Send:  {}", describe_send_data(acct.send.as_ref()));
    Ok(())
}

async fn test_account(name: &str) -> anyhow::Result<()> {
    let account = find_account_config(name).await?;
    run_account_operation(Request::TestAccountConfig { account }).await
}

async fn account_configs() -> anyhow::Result<Vec<AccountConfigData>> {
    let mut client = client().await?;
    match client.request(Request::ListAccountsConfig).await? {
        Response::Ok {
            data: ResponseData::AccountsConfig { accounts },
        } => Ok(accounts),
        Response::Error { message } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected accounts response: {other:?}"),
    }
}

async fn find_account_config(name: &str) -> anyhow::Result<AccountConfigData> {
    account_configs()
        .await?
        .into_iter()
        .find(|account| account.key == name)
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", name))
}

async fn run_account_operation(request: Request) -> anyhow::Result<()> {
    let mut client = client().await?;
    let result = request_account_operation(&mut client, request).await?;
    render_account_operation(result)
}

async fn request_account_operation(
    client: &mut IpcClient,
    request: Request,
) -> anyhow::Result<AccountOperationResult> {
    match client.request(request).await? {
        Response::Ok {
            data: ResponseData::AccountOperation { result },
        } => Ok(result),
        Response::Error { message } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected account operation response: {other:?}"),
    }
}

fn render_account_operation(result: AccountOperationResult) -> anyhow::Result<()> {
    println!("{}", result.summary);
    for step in [result.save, result.auth, result.sync, result.send]
        .into_iter()
        .flatten()
    {
        println!("  {} {}", if step.ok { "ok" } else { "error" }, step.detail);
    }
    if result.ok {
        Ok(())
    } else {
        anyhow::bail!(result.summary)
    }
}

async fn add_gmail() -> anyhow::Result<()> {
    println!("Adding Gmail account\n");

    let (credential_source, client_id, client_secret) =
        if prompt_bool("Use bundled OAuth credentials", true)? {
            (
                mxr_config::GmailCredentialSource::Bundled,
                String::new(),
                None,
            )
        } else {
            println!("See: https://console.cloud.google.com/apis/library/gmail.googleapis.com\n");
            let id = prompt("Client ID: ")?;
            let secret = prompt("Client Secret: ")?;
            (mxr_config::GmailCredentialSource::Custom, id, Some(secret))
        };

    let account_name = prompt("\nAccount name (e.g. personal, work): ")?;
    let email = prompt("Gmail address: ")?;
    ensure_account_available(&account_name).await?;
    let token_ref = format!("mxr/{account_name}-gmail");

    let account = AccountConfigData {
        key: account_name.clone(),
        name: account_name.clone(),
        email,
        enabled: true,
        sync: Some(AccountSyncConfigData::Gmail {
            credential_source: match credential_source {
                mxr_config::GmailCredentialSource::Bundled => GmailCredentialSourceData::Bundled,
                mxr_config::GmailCredentialSource::Custom => GmailCredentialSourceData::Custom,
            },
            client_id,
            client_secret,
            token_ref,
        }),
        send: Some(AccountSendConfigData::Gmail),
        is_default: false,
    };

    println!("\nOpening browser for Google authorization...");
    let mut client = client().await?;
    render_account_operation(
        request_account_operation(
            &mut client,
            Request::AuthorizeAccountConfig {
                account: account.clone(),
                reauthorize: false,
            },
        )
        .await?,
    )?;
    render_account_operation(
        request_account_operation(&mut client, Request::UpsertAccountConfig { account }).await?,
    )?;
    println!("Account '{}' saved.", account_name);
    Ok(())
}

async fn add_imap(include_smtp: bool) -> anyhow::Result<()> {
    println!("Adding IMAP account\n");
    let account_name = prompt("Account name: ")?;
    ensure_account_available(&account_name).await?;
    let display_name = prompt_default("Display name", &account_name)?;
    let email = prompt("Email address: ")?;

    let imap_host = prompt("IMAP host: ")?;
    let imap_port = prompt_default("IMAP port", "993")?.parse::<u16>()?;
    let imap_auth_required = prompt_bool("IMAP requires authentication", true)?;
    let (imap_username, imap_password_ref, imap_password) = if imap_auth_required {
        let imap_username = prompt_default("IMAP username", &email)?;
        let imap_password = prompt_secret("IMAP password: ")?;
        let imap_password_ref = format!("mxr/{account_name}-imap");
        (imap_username, imap_password_ref, Some(imap_password))
    } else {
        (String::new(), String::new(), None)
    };

    let send = if include_smtp {
        let smtp_host = prompt("SMTP host: ")?;
        let smtp_port = prompt_default("SMTP port", "587")?.parse::<u16>()?;
        let smtp_auth_required = prompt_bool("SMTP requires authentication", true)?;
        let (smtp_username, smtp_password_ref, smtp_password) = if smtp_auth_required {
            let smtp_username = prompt_default("SMTP username", &email)?;
            let smtp_password = prompt_secret("SMTP password: ")?;
            let smtp_password_ref = format!("mxr/{account_name}-smtp");
            (smtp_username, smtp_password_ref, Some(smtp_password))
        } else {
            (String::new(), String::new(), None)
        };
        Some(AccountSendConfigData::Smtp {
            host: smtp_host,
            port: smtp_port,
            username: smtp_username,
            password_ref: smtp_password_ref,
            password: smtp_password,
            auth_required: smtp_auth_required,
            use_tls: true,
        })
    } else {
        None
    };

    let account = AccountConfigData {
        key: account_name.clone(),
        name: display_name,
        email,
        enabled: true,
        sync: Some(AccountSyncConfigData::Imap {
            host: imap_host,
            port: imap_port,
            username: imap_username,
            password_ref: imap_password_ref,
            password: imap_password,
            auth_required: imap_auth_required,
            use_tls: true,
        }),
        send,
        is_default: false,
    };

    run_account_operation(Request::UpsertAccountConfig { account }).await?;
    println!("Account '{}' saved.", account_name);
    Ok(())
}

async fn add_smtp_only() -> anyhow::Result<()> {
    println!("Adding SMTP-only account\n");
    let account_name = prompt("Account name: ")?;
    ensure_account_available(&account_name).await?;
    let display_name = prompt_default("Display name", &account_name)?;
    let email = prompt("Email address: ")?;
    let smtp_host = prompt("SMTP host: ")?;
    let smtp_port = prompt_default("SMTP port", "587")?.parse::<u16>()?;
    let smtp_auth_required = prompt_bool("SMTP requires authentication", true)?;
    let (smtp_username, smtp_password_ref, smtp_password) = if smtp_auth_required {
        let smtp_username = prompt_default("SMTP username", &email)?;
        let smtp_password = prompt_secret("SMTP password: ")?;
        let smtp_password_ref = format!("mxr/{account_name}-smtp");
        (smtp_username, smtp_password_ref, Some(smtp_password))
    } else {
        (String::new(), String::new(), None)
    };

    let account = AccountConfigData {
        key: account_name.clone(),
        name: display_name,
        email,
        enabled: true,
        sync: None,
        send: Some(AccountSendConfigData::Smtp {
            host: smtp_host,
            port: smtp_port,
            username: smtp_username,
            password_ref: smtp_password_ref,
            password: smtp_password,
            auth_required: smtp_auth_required,
            use_tls: true,
        }),
        is_default: false,
    };

    run_account_operation(Request::UpsertAccountConfig { account }).await?;
    println!("Account '{}' saved.", account_name);
    Ok(())
}

async fn remove_account(
    name: &str,
    dry_run: bool,
    yes: bool,
    purge_local_data: bool,
) -> anyhow::Result<()> {
    if dry_run {
        return run_account_operation(remove_account_request(name, purge_local_data, true)).await;
    }

    if purge_local_data && !yes {
        run_account_operation(remove_account_request(name, true, true)).await?;
        if !prompt_bool(
            &format!("Permanently purge cached mail for '{}'", name),
            false,
        )? {
            anyhow::bail!("Aborted");
        }
    }

    run_account_operation(remove_account_request(name, purge_local_data, false)).await
}

async fn repair_account(name: &str) -> anyhow::Result<()> {
    let mut account = find_account_config(name).await?;

    let mut repairable = false;
    if let Some(AccountSyncConfigData::Imap {
        username,
        password,
        auth_required,
        ..
    }) = &mut account.sync
    {
        if *auth_required {
            *password = Some(prompt_secret(&format!("IMAP password for {}: ", username))?);
            repairable = true;
        }
    }

    if let Some(AccountSendConfigData::Smtp {
        username,
        password,
        auth_required,
        ..
    }) = &mut account.send
    {
        if *auth_required {
            *password = Some(prompt_secret(&format!("SMTP password for {}: ", username))?);
            repairable = true;
        }
    }

    if !repairable {
        anyhow::bail!(
            "Account '{}' has no password-backed IMAP/SMTP credentials to repair",
            name
        );
    }

    run_account_operation(Request::RepairAccountConfig { account }).await
}

async fn ensure_account_available(name: &str) -> anyhow::Result<()> {
    if account_configs()
        .await?
        .iter()
        .any(|account| account.key == name)
    {
        anyhow::bail!("Account '{}' already exists", name);
    }
    Ok(())
}

fn describe_sync_data(sync: Option<&AccountSyncConfigData>) -> &'static str {
    match sync {
        Some(AccountSyncConfigData::Gmail { .. }) => "gmail",
        Some(AccountSyncConfigData::Imap { .. }) => "imap",
        Some(AccountSyncConfigData::Fake) => "fake",
        None => "none",
    }
}

fn describe_send_data(send: Option<&AccountSendConfigData>) -> &'static str {
    match send {
        Some(AccountSendConfigData::Gmail) => "gmail",
        Some(AccountSendConfigData::Smtp { .. }) => "smtp",
        Some(AccountSendConfigData::Fake) => "fake",
        None => "none",
    }
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

fn disable_account_request(name: &str) -> mxr_protocol::Request {
    mxr_protocol::Request::DisableAccountConfig {
        key: name.to_string(),
    }
}

fn remove_account_request(
    name: &str,
    purge_local_data: bool,
    dry_run: bool,
) -> mxr_protocol::Request {
    mxr_protocol::Request::RemoveAccountConfig {
        key: name.to_string(),
        purge_local_data,
        dry_run,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::Request;

    #[test]
    fn disable_and_remove_requests_use_daemon_account_contract() {
        assert!(matches!(
            disable_account_request("work"),
            Request::DisableAccountConfig { key } if key == "work"
        ));

        assert!(matches!(
            remove_account_request("work", true, false),
            Request::RemoveAccountConfig {
                key,
                purge_local_data: true,
                dry_run: false,
            } if key == "work"
        ));
    }
}
