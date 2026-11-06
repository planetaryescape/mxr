//! User services own one profile and never request root daemon privileges.
use super::{proxy::command, state::Hosting};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub(super) fn name(config: &Hosting) -> String {
    let hash = Sha256::digest(config.config_dir.as_os_str().as_encoded_bytes());
    format!(
        "life.justlivin.mxr.hosting.{}",
        base16ct::lower::encode_string(&hash[..8])
    )
}

pub(super) fn path(config: &Hosting) -> anyhow::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("A user home is required for hosting"))?;
    if cfg!(target_os = "macos") {
        Ok(home
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", name(config))))
    } else if cfg!(target_os = "linux") {
        Ok(home
            .join(".config/systemd/user")
            .join(format!("{}.service", name(config))))
    } else {
        anyhow::bail!("Supervised hosting supports macOS launchd and Linux systemd only")
    }
}
fn text(path: &std::path::Path) -> anyhow::Result<&str> {
    let value = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Service paths must be UTF-8"))?;
    anyhow::ensure!(
        !value.chars().any(char::is_control),
        "Service paths may not contain control characters"
    );
    Ok(value)
}
fn systemd_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    )
}

pub(super) fn render(config: &Hosting, macos: bool) -> anyhow::Result<String> {
    let executable = text(&config.executable)?;
    let mut environments = vec![
        ("MXR_CONFIG_DIR", text(&config.config_dir)?),
        ("MXR_DATA_DIR", text(&config.data_dir)?),
        ("MXR_SOCKET_PATH", text(&config.socket)?),
        ("MXR_INSTANCE", config.instance.as_str()),
    ];
    if let Some(path) = &config.token_dir {
        environments.push(("MXR_TOKEN_DIR", text(path)?));
    }
    if let Some(path) = &config.secrets_file {
        environments.push(("MXR_SECRETS_PATH", text(path)?));
    }
    anyhow::ensure!(
        environments
            .iter()
            .all(|(_, value)| !value.chars().any(char::is_control)),
        "Service environment values may not contain control characters"
    );
    if macos {
        let xml = |v: &str| htmlescape::encode_minimal(v);
        let env = environments
            .iter()
            .map(|(key, value)| format!("<key>{key}</key><string>{}</string>", xml(value)))
            .collect::<String>();
        Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>serve</string><string>supervised</string></array><key>EnvironmentVariables</key><dict>{env}</dict><key>RunAtLoad</key><true/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict><key>ThrottleInterval</key><integer>5</integer></dict></plist>\n",name(config),xml(executable)))
    } else {
        let env = environments
            .iter()
            .map(|(key, value)| {
                format!("Environment={}\n", systemd_quote(&format!("{key}={value}")))
            })
            .collect::<String>();
        Ok(format!("[Unit]\nDescription=mxr private hosted mailbox\nAfter=network-online.target\n[Service]\nType=simple\nExecStart={} serve supervised\n{env}Restart=on-failure\nRestartSec=5\nTimeoutStopSec=20\nKillMode=process\n[Install]\nWantedBy=default.target\n",systemd_quote(executable).replace('$', "$$")))
    }
}

pub(super) async fn prerequisites() -> anyhow::Result<String> {
    if cfg!(target_os = "macos") {
        command("launchctl", &["print".into(), domain().await?]).await?;
        Ok("User login starts hosting; pre-login boot availability is not provided by a LaunchAgent".into())
    } else if cfg!(target_os = "linux") {
        command("systemctl", &["--user".into(), "show-environment".into()]).await?;
        let user = std::env::var("USER")
            .map_err(|_| anyhow::anyhow!("USER is required to inspect login lingering"))?;
        let linger = command(
            "loginctl",
            &[
                "show-user".into(),
                user.clone(),
                "--property=Linger".into(),
                "--value".into(),
            ],
        )
        .await?;
        anyhow::ensure!(linger.trim()=="yes", "Unattended Linux hosting needs user lingering. Run `sudo loginctl enable-linger {user}` and retry; mxr will not change this system prerequisite");
        Ok("User systemd service with lingering starts after boot".into())
    } else {
        anyhow::bail!("Unsupported supervisor")
    }
}
async fn domain() -> anyhow::Result<String> {
    let uid = command("id", &["-u".into()]).await?;
    anyhow::ensure!(
        !uid.trim().is_empty() && uid.trim().bytes().all(|b| b.is_ascii_digit()),
        "Cannot resolve current uid"
    );
    Ok(format!("gui/{}", uid.trim()))
}

fn receipt_matches(existing: &str, intended: &str, previous: Option<&Hosting>) -> bool {
    let Some(previous) = previous.filter(|p| p.service_installed) else {
        return false;
    };
    let observed = digest(existing);
    previous.service_digest.as_ref() == Some(&observed)
        || previous.service_previous_digest.as_ref() == Some(&observed)
        || (previous.service_digest.is_some() && existing == intended)
}

pub(super) fn check_install(config: &Hosting, previous: Option<&Hosting>) -> anyhow::Result<()> {
    let path = path(config)?;
    let content = render(config, cfg!(target_os = "macos"))?;
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        anyhow::ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Service file must be a regular owned file"
        );
        let existing = std::fs::read_to_string(&path)?;
        anyhow::ensure!(
            receipt_matches(&existing, &content, previous),
            "Service file changed outside mxr; inspect {} before retrying",
            path.display()
        );
    }
    Ok(())
}
pub(super) fn install(config: &Hosting, previous: Option<&Hosting>) -> anyhow::Result<()> {
    check_install(config, previous)?;
    let path = path(config)?;
    let content = render(config, cfg!(target_os = "macos"))?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Service path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

pub(super) fn digest(value: &str) -> String {
    base16ct::lower::encode_string(&Sha256::digest(value.as_bytes()))
}
fn verify(config: &Hosting) -> anyhow::Result<()> {
    let path = path(config)?;
    anyhow::ensure!(
        !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "Service file must not be a symlink"
    );
    let content = std::fs::read_to_string(path)?;
    anyhow::ensure!(
        config.service_installed
            && (config.service_digest.as_deref() == Some(digest(&content).as_str())
                || config.service_previous_digest.as_deref() == Some(digest(&content).as_str())),
        "Owned service file changed; inspect it before starting or stopping hosting"
    );
    Ok(())
}
pub(super) async fn start(config: &Hosting) -> anyhow::Result<()> {
    verify(config)?;
    if cfg!(target_os = "macos") {
        let domain = domain().await?;
        let target = format!("{domain}/{}", name(config));
        if command("launchctl", &["print".into(), target.clone()])
            .await
            .is_err()
        {
            command(
                "launchctl",
                &[
                    "bootstrap".into(),
                    domain,
                    path(config)?.to_string_lossy().into_owned(),
                ],
            )
            .await?;
        }
        // A cleanly exited job can remain loaded. Without -k this starts a
        // stopped job and leaves an already-running owner intact.
        command("launchctl", &["kickstart".into(), target]).await?;
    } else {
        command("systemctl", &["--user".into(), "daemon-reload".into()]).await?;
        command(
            "systemctl",
            &[
                "--user".into(),
                "enable".into(),
                "--now".into(),
                format!("{}.service", name(config)),
            ],
        )
        .await?;
    }
    Ok(())
}
pub(super) async fn stop(config: &Hosting) -> anyhow::Result<()> {
    verify(config)?;
    if cfg!(target_os = "macos") {
        let target = format!("{}/{}", domain().await?, name(config));
        if command("launchctl", &["print".into(), target.clone()])
            .await
            .is_ok()
        {
            command("launchctl", &["bootout".into(), target]).await?;
        }
    } else {
        command(
            "systemctl",
            &[
                "--user".into(),
                "disable".into(),
                "--now".into(),
                format!("{}.service", name(config)),
            ],
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_service_replacement_accepts_only_recorded_or_intended_contents() {
        let mut previous:Hosting=serde_json::from_value(serde_json::json!({"version":1,"executable":"/bin/mxr","config_dir":"/cfg","data_dir":"/data","socket":"/sock","instance":"f","public_origin":"https://host.ts.net","listen":"127.0.0.1:18741","workspace":{"workspace_id":"w","recovery_generation":"g"},"enabled":true,"daemon_enabled":true,"mapping_installed":false,"service_installed":true,"service_digest":digest("new unit"),"service_previous_digest":digest("old unit")})).unwrap();
        assert!(receipt_matches("old unit", "new unit", Some(&previous)));
        assert!(receipt_matches("new unit", "new unit", Some(&previous)));
        assert!(!receipt_matches(
            "another service",
            "new unit",
            Some(&previous)
        ));
        assert!(!receipt_matches("new unit", "new unit", None));
        previous.service_installed = false;
        assert!(!receipt_matches("old unit", "new unit", Some(&previous)));
    }

    #[test]
    fn systemd_paths_quote_specifiers_and_cannot_add_directives() {
        assert_eq!(systemd_quote("/tmp/a%h\"b\\c"), "\"/tmp/a%%h\\\"b\\\\c\"");
        assert!(text(std::path::Path::new("/tmp/a\nExecStart=/evil")).is_err());
    }
}
