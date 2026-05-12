//! `mxr setup` — first-run helper for demo, real mail accounts, and
//! optional LLM configuration.

use crate::cli::AccountsAction;
use inquire::{Confirm, CustomType, Password, PasswordDisplayMode, Select, Text};
use mxr_config::{AccountConfig, SyncProviderConfig};
use std::io::IsTerminal;

pub async fn run(demo: bool, key: String, force: bool) -> anyhow::Result<()> {
    if !demo {
        if std::io::stdin().is_terminal() {
            run_interactive_setup().await?;
            return Ok(());
        }
        print_quick_start();
        return Ok(());
    }

    let config_path = mxr_config::config_file_path();
    let mut config = mxr_config::load_config_from_path(&config_path).unwrap_or_default();

    if config.accounts.contains_key(&key) && !force {
        anyhow::bail!(
            "An account with key `{key}` already exists in {}. Pass --force to overwrite.",
            config_path.display()
        );
    }

    let account = AccountConfig {
        name: "Demo".into(),
        email: "demo@example.com".into(),
        enabled: true,
        sync: Some(SyncProviderConfig::Fake),
        send: None,
    };
    config.accounts.insert(key.clone(), account);

    // Make sure the demo account is the default if no default is set —
    // saves the user a step.
    if config
        .general
        .default_account
        .as_deref()
        .map(|d| d.is_empty())
        .unwrap_or(true)
    {
        config.general.default_account = Some(key.clone());
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    mxr_config::save_config_to_path(&config, &config_path)?;

    println!("✓ Wrote demo account `{key}` to {}", config_path.display());
    println!();
    println!("Next steps:");
    println!("  1. mxr daemon --foreground   # start the daemon (fake provider seeds mail)");
    println!("  2. mxr                       # open the TUI");
    println!("  3. mxr search 'is:unread'    # try a CLI search");
    println!();
    println!("To remove the demo later:");
    println!("  mxr accounts remove {key}");

    Ok(())
}

async fn run_interactive_setup() -> anyhow::Result<()> {
    println!("Welcome to mxr.");

    let choice = Select::new(
        "What do you want to set up?",
        vec![
            "Demo inbox",
            "Gmail account",
            "IMAP/SMTP account",
            "LLM features only",
        ],
    )
    .prompt()?;

    match choice {
        "Demo inbox" => {
            crate::commands::demo::prepare_environment(
                mxr_provider_fake::fixtures::CURATED_DEMO_MESSAGE_COUNT,
            )?;
            crate::commands::demo::run(
                mxr_provider_fake::fixtures::CURATED_DEMO_MESSAGE_COUNT,
                false,
            )
            .await?;
        }
        "Gmail account" => {
            let account_name = text_required("Account key", Some("personal"))?;
            let email = text_required("Gmail address", None)?;
            let (gmail_bundled, gmail_client_id, gmail_client_secret) =
                gmail_credentials_from_wizard()?;
            crate::commands::accounts::run(
                Some(AccountsAction::Add {
                    provider: "gmail".to_string(),
                    account_name: Some(account_name),
                    email: Some(email),
                    display_name: None,
                    gmail_bundled: Some(gmail_bundled),
                    gmail_client_id,
                    gmail_client_secret,
                    imap_host: None,
                    imap_port: 993,
                    imap_no_auth: false,
                    imap_username: None,
                    imap_password: None,
                    smtp_host: None,
                    smtp_port: 465,
                    smtp_no_auth: false,
                    smtp_username: None,
                    smtp_password: None,
                }),
                None,
            )
            .await?;
            println!();
            println!("Next: `mxr sync --wait`, then `mxr`.");
        }
        "IMAP/SMTP account" => {
            let imap = imap_smtp_from_wizard()?;
            crate::commands::accounts::run(
                Some(AccountsAction::Add {
                    provider: "imap-smtp".to_string(),
                    account_name: Some(imap.account_name),
                    email: Some(imap.email),
                    display_name: Some(imap.display_name),
                    gmail_bundled: None,
                    gmail_client_id: None,
                    gmail_client_secret: None,
                    imap_host: Some(imap.imap_host),
                    imap_port: imap.imap_port,
                    imap_no_auth: !imap.imap_auth_required,
                    imap_username: imap.imap_username,
                    imap_password: imap.imap_password,
                    smtp_host: Some(imap.smtp_host),
                    smtp_port: imap.smtp_port,
                    smtp_no_auth: !imap.smtp_auth_required,
                    smtp_username: imap.smtp_username,
                    smtp_password: imap.smtp_password,
                }),
                None,
            )
            .await?;
            println!();
            println!("Next: `mxr sync --wait`, then `mxr`.");
        }
        "LLM features only" => run_llm_wizard()?,
        _ => unreachable!("inquire Select returned an unknown setup option"),
    }

    if choice != "LLM features only"
        && Confirm::new("Configure LLM features now?")
            .with_default(false)
            .prompt()?
    {
        run_llm_wizard()?;
    }

    Ok(())
}

fn text_required(message: &str, default: Option<&str>) -> anyhow::Result<String> {
    let mut prompt = Text::new(message);
    if let Some(default) = default {
        prompt = prompt.with_default(default);
    }
    let value = prompt.prompt()?;
    if value.trim().is_empty() {
        anyhow::bail!("{message} is required");
    }
    Ok(value)
}

fn secret_required(message: &str) -> anyhow::Result<String> {
    let value = Password::new(message)
        .without_confirmation()
        .with_display_mode(PasswordDisplayMode::Hidden)
        .prompt()?;
    if value.trim().is_empty() {
        anyhow::bail!("{message} is required");
    }
    Ok(value)
}

fn gmail_credentials_from_wizard() -> anyhow::Result<(bool, Option<String>, Option<String>)> {
    if gmail_bundled_credentials_available()
        && Confirm::new("Use bundled mxr Gmail OAuth credentials?")
            .with_default(true)
            .prompt()?
    {
        return Ok((true, None, None));
    }

    if !gmail_bundled_credentials_available() {
        println!("This build does not include bundled Gmail OAuth credentials.");
        println!("Use installed-app credentials from Google Cloud.");
    }
    let client_id = text_required("Google OAuth client ID", None)?;
    let client_secret = secret_required("Google OAuth client secret")?;
    Ok((false, Some(client_id), Some(client_secret)))
}

fn gmail_bundled_credentials_available() -> bool {
    mxr_provider_gmail::auth::BUNDLED_CLIENT_ID.is_some()
        && mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET.is_some()
}

struct ImapSmtpWizard {
    account_name: String,
    display_name: String,
    email: String,
    imap_host: String,
    imap_port: u16,
    imap_auth_required: bool,
    imap_username: Option<String>,
    imap_password: Option<String>,
    smtp_host: String,
    smtp_port: u16,
    smtp_auth_required: bool,
    smtp_username: Option<String>,
    smtp_password: Option<String>,
}

fn imap_smtp_from_wizard() -> anyhow::Result<ImapSmtpWizard> {
    let account_name = text_required("Account key", Some("work"))?;
    let display_name = Text::new("Display name")
        .with_default(&account_name)
        .prompt()?;
    let email = text_required("Email address", None)?;
    let imap_host = text_required("IMAP host", None)?;
    let imap_port = CustomType::<u16>::new("IMAP port")
        .with_default(993)
        .with_error_message("Enter a valid port")
        .prompt()?;
    let imap_auth_required = Confirm::new("IMAP requires authentication?")
        .with_default(true)
        .prompt()?;
    let (imap_username, imap_password) =
        credentials_if_needed(imap_auth_required, "IMAP username", "IMAP password", &email)?;

    let smtp_host = text_required("SMTP host", None)?;
    let smtp_port = CustomType::<u16>::new("SMTP port")
        .with_default(465)
        .with_error_message("Enter a valid port")
        .prompt()?;
    let smtp_auth_required = Confirm::new("SMTP requires authentication?")
        .with_default(true)
        .prompt()?;
    let (smtp_username, smtp_password) =
        credentials_if_needed(smtp_auth_required, "SMTP username", "SMTP password", &email)?;

    Ok(ImapSmtpWizard {
        account_name,
        display_name,
        email,
        imap_host,
        imap_port,
        imap_auth_required,
        imap_username,
        imap_password,
        smtp_host,
        smtp_port,
        smtp_auth_required,
        smtp_username,
        smtp_password,
    })
}

fn credentials_if_needed(
    required: bool,
    username_prompt: &str,
    password_prompt: &str,
    default_username: &str,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    if !required {
        return Ok((None, None));
    }
    let username = Text::new(username_prompt)
        .with_default(default_username)
        .prompt()?;
    let password = secret_required(password_prompt)?;
    Ok((Some(username), Some(password)))
}

fn run_llm_wizard() -> anyhow::Result<()> {
    let preset = Select::new(
        "LLM provider",
        vec!["Local Ollama", "OpenAI-compatible cloud", "Custom"],
    )
    .prompt()?;

    let config_path = mxr_config::config_file_path();
    let mut config = mxr_config::load_config_from_path(&config_path).unwrap_or_default();
    match preset {
        "Local Ollama" => {
            let model_default = config.llm.model.clone();
            let model = Text::new("Model")
                .with_default(model_default.as_str())
                .prompt()?;
            apply_llm_setup_config(
                &mut config,
                true,
                "http://localhost:11434/v1".into(),
                model,
                String::new(),
                false,
            );
        }
        "OpenAI-compatible cloud" => {
            apply_llm_setup_config(
                &mut config,
                true,
                Text::new("Base URL")
                    .with_default("https://api.openai.com/v1")
                    .prompt()?,
                Text::new("Model").with_default("gpt-4o-mini").prompt()?,
                Text::new("API key environment variable")
                    .with_default("OPENAI_API_KEY")
                    .prompt()?,
                Confirm::new("Allow relationship data to leave this machine?")
                    .with_default(false)
                    .prompt()?,
            );
        }
        "Custom" => {
            let base_url_default = config.llm.base_url.clone();
            let model_default = config.llm.model.clone();
            let api_key_env_default = config.llm.api_key_env.clone();
            let allow_cloud_default = config.llm.allow_cloud_relationship_data;
            let base_url = Text::new("Base URL")
                .with_default(base_url_default.as_str())
                .prompt()?;
            let model = Text::new("Model")
                .with_default(model_default.as_str())
                .prompt()?;
            let api_key_env = Text::new("API key environment variable (blank for none)")
                .with_default(api_key_env_default.as_str())
                .prompt()?;
            let allow_cloud_relationship_data =
                Confirm::new("Allow relationship data to leave this machine?")
                    .with_default(allow_cloud_default)
                    .prompt()?;
            apply_llm_setup_config(
                &mut config,
                true,
                base_url,
                model,
                api_key_env,
                allow_cloud_relationship_data,
            );
        }
        _ => unreachable!("inquire Select returned an unknown LLM option"),
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    mxr_config::save_config_to_path(&config, &config_path)?;
    println!("Saved LLM config to {}", config_path.display());
    Ok(())
}

fn apply_llm_setup_config(
    config: &mut mxr_config::MxrConfig,
    enabled: bool,
    base_url: String,
    model: String,
    api_key_env: String,
    allow_cloud_relationship_data: bool,
) {
    config.llm.enabled = enabled;
    config.llm.base_url = base_url;
    config.llm.model = model;
    config.llm.api_key_env = api_key_env;
    config.llm.allow_cloud_relationship_data = allow_cloud_relationship_data;
}

fn print_quick_start() {
    println!("mxr setup — first-run guidance");
    println!();
    println!("Quickest path: try a full demo inbox with no real mail access.");
    println!("  mxr demo");
    println!();
    println!("Real-account paths (the daemon will guide you through OAuth):");
    println!("  Gmail:");
    println!("    mxr accounts add gmail --account-name personal --email me@gmail.com");
    println!("  IMAP/SMTP (Fastmail, generic provider):");
    println!("    mxr accounts add imap --account-name work --email me@work.example.com \\");
    println!("                         --imap-host imap.work.example.com --imap-port 993 \\");
    println!("                         --smtp-host smtp.work.example.com --smtp-port 465");
    println!();
    println!("Already have accounts configured? Try:");
    println!("  mxr status         # daemon health");
    println!("  mxr doctor         # diagnostics + remediation");
}

#[cfg(test)]
mod tests {
    use super::apply_llm_setup_config;

    #[test]
    fn apply_llm_setup_config_updates_only_base_llm_fields() {
        let mut config = mxr_config::MxrConfig::default();

        apply_llm_setup_config(
            &mut config,
            true,
            "https://api.example.com/v1".into(),
            "model-x".into(),
            "MXR_LLM_KEY".into(),
            true,
        );

        assert!(config.llm.enabled);
        assert_eq!(config.llm.base_url, "https://api.example.com/v1");
        assert_eq!(config.llm.model, "model-x");
        assert_eq!(config.llm.api_key_env, "MXR_LLM_KEY");
        assert!(config.llm.allow_cloud_relationship_data);
    }
}
