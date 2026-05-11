//! `mxr setup` — first-run helper. Currently delivers the demo-mode
//! slice: drops a fake-provider account into the user's config so they
//! can try the TUI/CLI without real mail credentials.

use crate::cli::AccountsAction;
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
    println!("Choose the fastest path to a useful inbox:");
    println!("  1. Demo inbox - no real mail, opens immediately");
    println!("  2. Gmail - connect your inbox with OAuth");
    println!("  3. IMAP/SMTP - Fastmail, iCloud, Gmail app password, or generic mail");
    println!();

    match prompt("Start with [1/demo, 2/gmail, 3/imap]: ")?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "1" | "d" | "demo" => {
            crate::commands::demo::prepare_environment(
                mxr_provider_fake::fixtures::DEFAULT_DEMO_MESSAGE_COUNT,
            )?;
            crate::commands::demo::run(
                mxr_provider_fake::fixtures::DEFAULT_DEMO_MESSAGE_COUNT,
                false,
            )
            .await?;
        }
        "2" | "g" | "gmail" => {
            crate::commands::accounts::run(
                Some(AccountsAction::Add {
                    provider: "gmail".to_string(),
                    account_name: None,
                    email: None,
                    display_name: None,
                    gmail_bundled: None,
                    gmail_client_id: None,
                    gmail_client_secret: None,
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
        "3" | "i" | "imap" | "imap-smtp" => {
            crate::commands::accounts::run(
                Some(AccountsAction::Add {
                    provider: "imap-smtp".to_string(),
                    account_name: None,
                    email: None,
                    display_name: None,
                    gmail_bundled: None,
                    gmail_client_id: None,
                    gmail_client_secret: None,
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
        other => anyhow::bail!(
            "Unknown setup choice `{other}`. Run `mxr demo` or `mxr accounts add gmail`."
        ),
    }

    Ok(())
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

fn prompt(msg: &str) -> anyhow::Result<String> {
    use std::io::{self, Write};
    print!("{msg}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
