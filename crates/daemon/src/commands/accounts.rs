use crate::cli::{AccountsAction, AddressesOp, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::AccountId;
use mxr_core::types::AccountAddress;
use mxr_protocol::{
    AccountConfigData, AccountOperationResult, AccountSendConfigData, AccountSummaryData,
    AccountSyncConfigData, AuthFlowData, AuthSessionData, AuthSessionStateData,
    GmailCredentialSourceData, Request, Response, ResponseData,
};
use std::io::IsTerminal;
use std::time::Duration;

pub async fn run(
    action: Option<AccountsAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match action {
        None => list_accounts(format).await?,
        Some(AccountsAction::Add {
            provider,
            account_name,
            email,
            display_name,
            gmail_bundled,
            gmail_client_id,
            gmail_client_secret,
            imap_host,
            imap_port,
            imap_no_auth,
            imap_username,
            imap_password,
            smtp_host,
            smtp_port,
            smtp_no_auth,
            smtp_username,
            smtp_password,
        }) => {
            let args = AddArgs {
                account_name,
                email,
                display_name,
                gmail_bundled,
                gmail_client_id,
                gmail_client_secret,
                imap_host,
                imap_port,
                imap_no_auth,
                imap_username,
                imap_password,
                smtp_host,
                smtp_port,
                smtp_no_auth,
                smtp_username,
                smtp_password,
            };
            match provider.as_str() {
                "gmail" => add_gmail(&args).await?,
                "imap" | "imap-smtp" => add_imap(true, &args).await?,
                "smtp" => add_smtp_only(&args).await?,
                "outlook" => add_outlook().await?,
                "outlook-work" => add_outlook_work().await?,
                other => anyhow::bail!(
                    "Unknown provider '{}'. Supported: gmail, imap, smtp, imap-smtp, outlook, outlook-work",
                    other
                ),
            }
        }
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
        Some(AccountsAction::Addresses { op }) => addresses_dispatch(op, format).await?,
    }
    Ok(())
}

async fn addresses_dispatch(op: AddressesOp, format: Option<OutputFormat>) -> anyhow::Result<()> {
    match op {
        AddressesOp::List { account } => list_addresses(account.as_deref(), format).await,
        AddressesOp::Add {
            account,
            email,
            primary,
        } => add_address(account.as_deref(), &email, primary).await,
        AddressesOp::Remove { account, email } => remove_address(account.as_deref(), &email).await,
        AddressesOp::SetPrimary { account, email } => {
            set_primary_address(account.as_deref(), &email).await
        }
    }
}

async fn resolve_account_id(name: Option<&str>) -> anyhow::Result<AccountId> {
    let mut client = client().await?;
    let summaries = match client.request(Request::ListAccounts).await? {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => accounts,
        Response::Error { message, .. } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected accounts response: {other:?}"),
    };

    let chosen: Option<AccountSummaryData> = match name {
        Some(needle) => summaries
            .into_iter()
            .find(|s| s.key.as_deref() == Some(needle) || s.name == needle || s.email == needle),
        None => summaries.into_iter().find(|s| s.is_default),
    };

    chosen.map(|s| s.account_id).ok_or_else(|| match name {
        Some(n) => anyhow::anyhow!("Account '{n}' not found"),
        None => anyhow::anyhow!(
            "No default account configured. Pass --account=<name> or set a default."
        ),
    })
}

async fn list_addresses(
    account_name: Option<&str>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let account_id = resolve_account_id(account_name).await?;
    let mut client = client().await?;
    let addresses = match client
        .request(Request::ListAccountAddresses { account_id })
        .await?
    {
        Response::Ok {
            data: ResponseData::AccountAddresses { addresses },
        } => addresses,
        Response::Error { message, .. } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected addresses response: {other:?}"),
    };
    render_addresses(&addresses, format)
}

fn render_addresses(
    addresses: &[AccountAddress],
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(addresses)?),
        OutputFormat::Jsonl => println!("{}", jsonl(addresses)?),
        OutputFormat::Csv => {
            println!("email,is_primary");
            for a in addresses {
                let email = a.email.replace('"', "\"\"");
                println!("\"{email}\",{}", a.is_primary);
            }
        }
        OutputFormat::Ids => {
            for a in addresses {
                println!("{}", a.email);
            }
        }
        OutputFormat::Table => {
            if addresses.is_empty() {
                println!("No addresses configured.");
                return Ok(());
            }
            println!("{:<48} PRIMARY", "EMAIL");
            println!("{}", "-".repeat(58));
            for a in addresses {
                let email: String = a.email.chars().take(48).collect();
                println!("{:<48} {}", email, if a.is_primary { "yes" } else { "no" });
            }
        }
    }
    Ok(())
}

async fn add_address(account_name: Option<&str>, email: &str, primary: bool) -> anyhow::Result<()> {
    let account_id = resolve_account_id(account_name).await?;
    let mut client = client().await?;
    match client
        .request(Request::AddAccountAddress {
            account_id,
            email: email.to_string(),
            primary,
        })
        .await?
    {
        Response::Ok { .. } => {
            println!("Added {email}{}.", if primary { " (primary)" } else { "" });
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn remove_address(account_name: Option<&str>, email: &str) -> anyhow::Result<()> {
    let account_id = resolve_account_id(account_name).await?;
    let mut client = client().await?;
    match client
        .request(Request::RemoveAccountAddress {
            account_id,
            email: email.to_string(),
        })
        .await?
    {
        Response::Ok { .. } => {
            println!("Removed {email}.");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn set_primary_address(account_name: Option<&str>, email: &str) -> anyhow::Result<()> {
    let account_id = resolve_account_id(account_name).await?;
    let mut client = client().await?;
    match client
        .request(Request::SetPrimaryAccountAddress {
            account_id,
            email: email.to_string(),
        })
        .await?
    {
        Response::Ok { .. } => {
            println!("Promoted {email} to primary.");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn client() -> anyhow::Result<IpcClient> {
    crate::server::ensure_daemon_running().await?;
    IpcClient::connect().await
}

async fn list_accounts(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let accounts = account_configs().await?;
    let summaries: Vec<AccountSummaryRow> = accounts.iter().map(AccountSummaryRow::from).collect();

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
        Response::Error { message, .. } => anyhow::bail!(message),
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
        Response::Error { message, .. } => anyhow::bail!(message),
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

async fn authorize_and_save_account(account: AccountConfigData) -> anyhow::Result<()> {
    let mut client = client().await?;
    let session = request_auth_session(
        &mut client,
        Request::StartAuthSession {
            account,
            reauthorize: false,
            flow: AuthFlowData::Auto,
        },
    )
    .await?;
    let authorized = wait_for_auth_session(&mut client, session).await?;
    let completed = request_auth_session(
        &mut client,
        Request::CompleteAuthSession {
            session_id: authorized.session_id,
            save_account: true,
        },
    )
    .await?;

    match completed.state {
        AuthSessionStateData::Authorized => {
            if let Some(message) = completed.message {
                println!("{message}");
            }
            println!("Account '{}' saved.", completed.account_key);
            Ok(())
        }
        AuthSessionStateData::Failed => anyhow::bail!(
            "authorization failed: {}",
            completed
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        ),
        AuthSessionStateData::Cancelled => anyhow::bail!("authorization cancelled"),
        AuthSessionStateData::Starting | AuthSessionStateData::WaitingForUser => {
            anyhow::bail!("authorization did not complete")
        }
    }
}

async fn wait_for_auth_session(
    client: &mut IpcClient,
    mut session: AuthSessionData,
) -> anyhow::Result<AuthSessionData> {
    let mut printed_prompt: Option<String> = None;

    loop {
        print_auth_session_prompt(&session, &mut printed_prompt);
        match session.state {
            AuthSessionStateData::Authorized => return Ok(session),
            AuthSessionStateData::Failed => anyhow::bail!(
                "authorization failed: {}",
                session.error.unwrap_or_else(|| "unknown error".to_string())
            ),
            AuthSessionStateData::Cancelled => anyhow::bail!("authorization cancelled"),
            AuthSessionStateData::Starting | AuthSessionStateData::WaitingForUser => {}
        }

        let delay = session.poll_interval_secs.unwrap_or(2).clamp(1, 10);
        tokio::time::sleep(Duration::from_secs(delay)).await;
        session = request_auth_session(
            client,
            Request::GetAuthSession {
                session_id: session.session_id.clone(),
            },
        )
        .await?;
    }
}

fn print_auth_session_prompt(session: &AuthSessionData, printed_prompt: &mut Option<String>) {
    let prompt_key = format!(
        "{}|{}|{}",
        session.auth_url.as_deref().unwrap_or_default(),
        session.verification_uri.as_deref().unwrap_or_default(),
        session.user_code.as_deref().unwrap_or_default()
    );
    if prompt_key.trim_matches('|').is_empty() || printed_prompt.as_deref() == Some(&prompt_key) {
        return;
    }
    *printed_prompt = Some(prompt_key);

    if let Some(url) = session.auth_url.as_deref() {
        println!("Open this authorization URL: {url}");
        if open::that(url).is_ok() {
            println!("(Browser opened automatically)");
        }
    }
    if let (Some(uri), Some(code)) = (
        session.verification_uri.as_deref(),
        session.user_code.as_deref(),
    ) {
        println!("Go to {uri} and enter: {code}");
    }
    if let Some(message) = session.message.as_deref() {
        println!("{message}");
    }
}

async fn request_auth_session(
    client: &mut IpcClient,
    request: Request,
) -> anyhow::Result<AuthSessionData> {
    match client.request(request).await? {
        Response::Ok {
            data: ResponseData::AuthSession { session },
        } => Ok(session),
        Response::Error { message, .. } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected auth session response: {other:?}"),
    }
}

/// Optional inputs for `mxr accounts add`. When `Some`, skip the corresponding
/// interactive prompt; when `None`, fall back to the wizard. Hardened so
/// scripts can drive the flow without a TTY.
pub(super) struct AddArgs {
    pub account_name: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub gmail_bundled: Option<bool>,
    pub gmail_client_id: Option<String>,
    pub gmail_client_secret: Option<String>,
    pub imap_host: Option<String>,
    pub imap_port: u16,
    pub imap_no_auth: bool,
    pub imap_username: Option<String>,
    pub imap_password: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_no_auth: bool,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
}

fn or_prompt(value: Option<String>, msg: &str) -> anyhow::Result<String> {
    if let Some(v) = value {
        return Ok(v);
    }
    prompt(msg)
}

fn or_prompt_default(value: Option<String>, msg: &str, default: &str) -> anyhow::Result<String> {
    if let Some(v) = value {
        return Ok(v);
    }
    prompt_default(msg, default)
}

fn or_prompt_secret(value: Option<String>, env_var: &str, msg: &str) -> anyhow::Result<String> {
    if let Some(v) = value {
        return Ok(v);
    }
    if let Ok(v) = std::env::var(env_var) {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    prompt_secret(msg)
}

async fn add_gmail(args: &AddArgs) -> anyhow::Result<()> {
    println!("Adding Gmail account\n");

    let custom_requested = args.gmail_bundled == Some(false)
        || args.gmail_client_id.is_some()
        || args.gmail_client_secret.is_some();
    let bundled = if custom_requested {
        false
    } else if args.gmail_bundled == Some(true) {
        if !gmail_bundled_credentials_available() {
            anyhow::bail!(missing_gmail_bundled_credentials_message());
        }
        true
    } else if gmail_bundled_credentials_available() {
        true
    } else if std::io::stdin().is_terminal() {
        print_missing_gmail_bundled_credentials_help();
        if !prompt_bool("Use your own Google OAuth credentials now", false)? {
            anyhow::bail!(
                "Gmail setup cancelled. Try `mxr demo` first, install the official release build, or rerun with `--gmail-bundled=false`."
            );
        }
        false
    } else {
        anyhow::bail!(missing_gmail_bundled_credentials_message());
    };

    let (credential_source, client_id, client_secret) = if bundled {
        (
            mxr_config::GmailCredentialSource::Bundled,
            String::new(),
            None,
        )
    } else {
        if args.gmail_client_id.is_none() {
            println!("See: https://console.cloud.google.com/apis/library/gmail.googleapis.com\n");
        }
        let id = or_prompt(args.gmail_client_id.clone(), "Client ID: ")?;
        let secret = or_prompt_secret(
            args.gmail_client_secret.clone(),
            "MXR_GMAIL_CLIENT_SECRET",
            "Client Secret: ",
        )?;
        (mxr_config::GmailCredentialSource::Custom, id, Some(secret))
    };

    let account_name = or_prompt(
        args.account_name.clone(),
        "\nAccount name (e.g. personal, work): ",
    )?;
    let email = or_prompt(args.email.clone(), "Gmail address: ")?;
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

    authorize_and_save_account(account).await?;
    Ok(())
}

fn gmail_bundled_credentials_available() -> bool {
    mxr_provider_gmail::auth::BUNDLED_CLIENT_ID.is_some()
        && mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET.is_some()
}

fn missing_gmail_bundled_credentials_message() -> &'static str {
    "This mxr build does not include one-click Gmail OAuth credentials. Install an official release build, run `mxr demo` to try mxr without your inbox, or rerun Gmail setup with `--gmail-bundled=false --gmail-client-id ...` and `MXR_GMAIL_CLIENT_SECRET`."
}

fn print_missing_gmail_bundled_credentials_help() {
    println!("This build cannot start one-click Gmail OAuth because it was built without the official mxr Gmail client.");
    println!("Fastest safe path: run `mxr demo` to try a full synthetic inbox first.");
    println!(
        "To connect Gmail from this build, use your own Google OAuth desktop-app credentials."
    );
    println!();
}

async fn add_imap(include_smtp: bool, args: &AddArgs) -> anyhow::Result<()> {
    println!("Adding IMAP account\n");
    let account_name = or_prompt(args.account_name.clone(), "Account name: ")?;
    ensure_account_available(&account_name).await?;
    let display_name = or_prompt_default(args.display_name.clone(), "Display name", &account_name)?;
    let email = or_prompt(args.email.clone(), "Email address: ")?;

    let imap_host = or_prompt(args.imap_host.clone(), "IMAP host: ")?;
    let imap_port_default = args.imap_port.to_string();
    let imap_port = or_prompt_default(None, "IMAP port", &imap_port_default)?.parse::<u16>()?;
    let imap_auth_required = if args.imap_no_auth {
        false
    } else if args.imap_username.is_some() || args.imap_password.is_some() {
        true
    } else {
        prompt_bool("IMAP requires authentication", true)?
    };
    let (imap_username, imap_password_ref, imap_password) = if imap_auth_required {
        let imap_username = or_prompt_default(args.imap_username.clone(), "IMAP username", &email)?;
        let imap_password = or_prompt_secret(
            args.imap_password.clone(),
            "MXR_IMAP_PASSWORD",
            "IMAP password: ",
        )?;
        let imap_password_ref = format!("mxr/{account_name}-imap");
        (imap_username, imap_password_ref, Some(imap_password))
    } else {
        (String::new(), String::new(), None)
    };

    let send = if include_smtp {
        let smtp_host = or_prompt(args.smtp_host.clone(), "SMTP host: ")?;
        let smtp_port_default = args.smtp_port.to_string();
        let smtp_port = or_prompt_default(None, "SMTP port", &smtp_port_default)?.parse::<u16>()?;
        let smtp_auth_required = if args.smtp_no_auth {
            false
        } else if args.smtp_username.is_some() || args.smtp_password.is_some() {
            true
        } else {
            prompt_bool("SMTP requires authentication", true)?
        };
        let (smtp_username, smtp_password_ref, smtp_password) = if smtp_auth_required {
            let smtp_username =
                or_prompt_default(args.smtp_username.clone(), "SMTP username", &email)?;
            let smtp_password = or_prompt_secret(
                args.smtp_password.clone(),
                "MXR_SMTP_PASSWORD",
                "SMTP password: ",
            )?;
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

async fn add_smtp_only(args: &AddArgs) -> anyhow::Result<()> {
    println!("Adding SMTP-only account\n");
    let account_name = or_prompt(args.account_name.clone(), "Account name: ")?;
    ensure_account_available(&account_name).await?;
    let display_name = or_prompt_default(args.display_name.clone(), "Display name", &account_name)?;
    let email = or_prompt(args.email.clone(), "Email address: ")?;
    let smtp_host = or_prompt(args.smtp_host.clone(), "SMTP host: ")?;
    let smtp_port_default = args.smtp_port.to_string();
    let smtp_port = or_prompt_default(None, "SMTP port", &smtp_port_default)?.parse::<u16>()?;
    let smtp_auth_required = if args.smtp_no_auth {
        false
    } else if args.smtp_username.is_some() || args.smtp_password.is_some() {
        true
    } else {
        prompt_bool("SMTP requires authentication", true)?
    };
    let (smtp_username, smtp_password_ref, smtp_password) = if smtp_auth_required {
        let smtp_username = or_prompt_default(args.smtp_username.clone(), "SMTP username", &email)?;
        let smtp_password = or_prompt_secret(
            args.smtp_password.clone(),
            "MXR_SMTP_PASSWORD",
            "SMTP password: ",
        )?;
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

async fn add_outlook() -> anyhow::Result<()> {
    add_outlook_inner(OutlookAccountKind::Personal).await
}

async fn add_outlook_work() -> anyhow::Result<()> {
    add_outlook_inner(OutlookAccountKind::Work).await
}

#[derive(Clone, Copy)]
enum OutlookAccountKind {
    Personal,
    Work,
}

async fn add_outlook_inner(kind: OutlookAccountKind) -> anyhow::Result<()> {
    let label = match kind {
        OutlookAccountKind::Personal => "Outlook (Personal)",
        OutlookAccountKind::Work => "Outlook (Work)",
    };
    println!("Adding {label} account (OAuth2 + IMAP + SMTP)\n");

    println!("Press Enter to use bundled Azure app credentials when available.");
    let client_id = prompt("Azure app client ID: ")?;
    let client_id = if client_id.trim().is_empty() {
        None
    } else {
        Some(client_id)
    };

    let account_name = prompt("\nAccount name (e.g. personal, work): ")?;
    ensure_account_available(&account_name).await?;
    let display_name = prompt_default("Display name", &account_name)?;
    let email = prompt("Microsoft email address: ")?;

    let token_ref = format!("mxr/{account_name}-outlook");
    let (sync, send) = match kind {
        OutlookAccountKind::Work => (
            Some(AccountSyncConfigData::OutlookWork {
                client_id: client_id.clone(),
                token_ref: token_ref.clone(),
            }),
            Some(AccountSendConfigData::OutlookWork {
                client_id: client_id.clone(),
                token_ref: token_ref.clone(),
            }),
        ),
        OutlookAccountKind::Personal => (
            Some(AccountSyncConfigData::OutlookPersonal {
                client_id: client_id.clone(),
                token_ref: token_ref.clone(),
            }),
            Some(AccountSendConfigData::OutlookPersonal {
                client_id,
                token_ref: token_ref.clone(),
            }),
        ),
    };

    let account = AccountConfigData {
        key: account_name.clone(),
        name: display_name,
        email,
        enabled: true,
        sync,
        send,
        is_default: false,
    };

    authorize_and_save_account(account).await?;
    Ok(())
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
        Some(AccountSyncConfigData::OutlookPersonal { .. }) => "outlook",
        Some(AccountSyncConfigData::OutlookWork { .. }) => "outlook-work",
        Some(AccountSyncConfigData::Fake) => "fake",
        None => "none",
    }
}

fn describe_send_data(send: Option<&AccountSendConfigData>) -> &'static str {
    match send {
        Some(AccountSendConfigData::Gmail) => "gmail",
        Some(AccountSendConfigData::Smtp { .. }) => "smtp",
        Some(AccountSendConfigData::OutlookPersonal { .. }) => "outlook",
        Some(AccountSendConfigData::OutlookWork { .. }) => "outlook-work",
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

    #[test]
    fn missing_gmail_bundled_credentials_points_to_safe_paths() {
        let message = missing_gmail_bundled_credentials_message();
        assert!(message.contains("mxr demo"));
        assert!(message.contains("official release build"));
        assert!(message.contains("--gmail-bundled=false"));
        assert!(message.contains("MXR_GMAIL_CLIENT_SECRET"));
    }
}
