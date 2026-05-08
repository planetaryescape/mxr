//! `mxr setup` — first-run helper. Currently delivers the demo-mode
//! slice: drops a fake-provider account into the user's config so they
//! can try the TUI/CLI without real mail credentials.

use mxr_config::{AccountConfig, MxrConfig, SyncProviderConfig};

pub async fn run(demo: bool, key: String, force: bool) -> anyhow::Result<()> {
    if !demo {
        print_quick_start();
        return Ok(());
    }

    let config_path = mxr_config::config_file_path();
    let mut config = match mxr_config::load_config_from_path(&config_path) {
        Ok(c) => c,
        Err(_) => {
            // No config yet (or unreadable) — start fresh. The save
            // path will create parent directories as needed.
            MxrConfig::default()
        }
    };

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

fn print_quick_start() {
    println!("mxr setup — first-run guidance");
    println!();
    println!("Quickest path: try the demo account (in-memory, no real mail).");
    println!("  mxr setup --demo");
    println!();
    println!("Real-account paths (the daemon will guide you through OAuth):");
    println!("  Gmail:");
    println!("    mxr accounts add gmail --key personal --email me@gmail.com");
    println!("  IMAP/SMTP (Fastmail, generic provider):");
    println!("    mxr accounts add imap --key work --email me@work.example.com \\");
    println!("                         --imap-host imap.work.example.com:993 \\");
    println!("                         --smtp-host smtp.work.example.com:465");
    println!();
    println!("Already have accounts configured? Try:");
    println!("  mxr status         # daemon health");
    println!("  mxr doctor         # diagnostics + remediation");
}
