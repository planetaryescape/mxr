//! Offline operator maintenance deliberately bypasses daemon autostart and IPC.
use anyhow::{bail, Context};
use mxr_store::backup::{self, BackupManifest};
use std::path::{Path, PathBuf};

fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = path
        .parent()
        .context("Destination needs a parent")?
        .canonicalize()?;
    Ok(parent.join(
        path.file_name()
            .context("Destination needs a directory name")?,
    ))
}
fn empty(path: &Path) -> anyhow::Result<()> {
    if let Ok(meta) = path.symlink_metadata() {
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || std::fs::read_dir(path)?.next().is_some()
        {
            bail!("Destination must be absent or an empty real directory");
        }
    }
    Ok(())
}
fn publish(stage: &Path, destination: &Path) -> anyhow::Result<()> {
    empty(destination)?;
    sync_tree(stage)?;
    if destination.exists() {
        std::fs::remove_dir(destination)?;
    }
    std::fs::rename(stage, destination)?;
    std::fs::File::open(destination.parent().context("Destination has no parent")?)?.sync_all()?;
    Ok(())
}
fn sync_tree(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if path.is_dir() { 0o700 } else { 0o600 }),
        )?;
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    }
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn settings(source_config: Option<&Path>) -> anyhow::Result<mxr_config::MxrConfig> {
    let mut safe = mxr_config::MxrConfig::default();
    if let Some(path) = source_config {
        let original: mxr_config::MxrConfig = toml::from_str(&std::fs::read_to_string(path)?)?;
        safe.general.locale = original.general.locale;
        safe.general.default_account = original.general.default_account;
        for (key, account) in original.accounts {
            let original = serde_json::to_value(account)?;
            let mut account = serde_json::json!({"name":original["name"],"email":original["email"],"enabled":false});
            // The colour and tag overrides are the user's own choices, not secrets.
            for field in ["color", "tag"] {
                if let Some(value) = original.get(field).filter(|value| !value.is_null()) {
                    account[field] = value.clone();
                }
            }
            for provider in ["sync", "send"] {
                if let Some(value) = original.get(provider).filter(|v| !v.is_null()) {
                    let mut clean = serde_json::Map::new();
                    for field in [
                        "type",
                        "provider",
                        "host",
                        "port",
                        "username",
                        "tls",
                        "use_tls",
                        "auth_required",
                        "max_connections",
                        "client_id",
                    ] {
                        if let Some(value) = value.get(field) {
                            clean.insert(field.into(), value.clone());
                        }
                    }
                    if value.get("token_ref").is_some() {
                        clean.insert(
                            "token_ref".into(),
                            serde_json::json!(format!("reprovision-{key}")),
                        );
                    }
                    if value.get("password_ref").is_some() {
                        clean.insert(
                            "password_ref".into(),
                            serde_json::json!(format!("reprovision-{key}")),
                        );
                    }
                    account[provider] = serde_json::Value::Object(clean);
                }
            }
            safe.accounts.insert(key, serde_json::from_value(account)?);
        }
    }
    safe.llm.enabled = false;
    safe.search.semantic.enabled = false;
    safe.search.semantic.auto_download_models = false;
    Ok(safe)
}

pub async fn create(
    source_data: &Path,
    source_config: Option<&Path>,
    output: &Path,
    dry_run: bool,
) -> anyhow::Result<BackupManifest> {
    let source = source_data.canonicalize()?;
    let output = absolute(output)?;
    if source == output || output.starts_with(&source) {
        bail!("Backup destination must be outside the source workspace");
    }
    let _source_lease = mxr_store::workspace_lease::exclusive(&source)?;
    let _output_lease = mxr_store::workspace_lease::exclusive(&output)?;
    empty(&output)?;
    let stage = tempfile::Builder::new()
        .prefix(".mxr-backup-")
        .tempdir_in(output.parent().context("Output has no parent")?)?;
    let mut manifest = backup::populate(&source, stage.path())
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::write(
        stage.path().join("settings.json"),
        serde_json::to_vec_pretty(&settings(source_config)?)?,
    )?;
    manifest
        .files
        .push(backup::file_metadata(stage.path(), "settings.json").map_err(anyhow::Error::msg)?);
    manifest.exclusions.push(if source_config.is_some(){"Only explicit source-config account connection metadata and locale/default account are included; all accounts disabled and credentials require reprovisioning"}else{"No source config selected: DB account metadata only; connection settings require reprovisioning"}.into());
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    backup::inspect(stage.path())
        .await
        .map_err(anyhow::Error::msg)?;
    if !dry_run {
        publish(stage.path(), &output)?;
    }
    Ok(manifest)
}
pub async fn inspect(bundle: &Path) -> anyhow::Result<BackupManifest> {
    backup::inspect(bundle).await.map_err(anyhow::Error::msg)
}

pub async fn restore(
    bundle: &Path,
    destination: &Path,
    dry_run: bool,
) -> anyhow::Result<BackupManifest> {
    let bundle = bundle.canonicalize()?;
    let destination = absolute(destination)?;
    if bundle == destination || destination.starts_with(&bundle) {
        bail!("Restore destination must be outside the bundle");
    }
    let _destination_lease = mxr_store::workspace_lease::exclusive(&destination)?;
    empty(&destination)?;
    let manifest = backup::inspect(&bundle).await.map_err(anyhow::Error::msg)?;
    let stage = tempfile::Builder::new()
        .prefix(".mxr-restore-")
        .tempdir_in(destination.parent().context("Destination has no parent")?)?;
    for entry in &manifest.files {
        let relative = backup::checked_relative(&entry.path).map_err(anyhow::Error::msg)?;
        let target = stage.path().join(&relative);
        std::fs::create_dir_all(target.parent().context("Bundle file has no parent")?)?;
        std::fs::copy(bundle.join(relative), target)?;
    }
    // Verify the copied artifact too: a concurrently changed bundle never publishes.
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    backup::inspect(stage.path())
        .await
        .map_err(anyhow::Error::msg)?;
    backup::remap_restored(&stage.path().join("mxr.db"), &destination)
        .await
        .map_err(anyhow::Error::msg)?;
    backup::retire_restored(&stage.path().join("mxr.db"))
        .await
        .map_err(anyhow::Error::msg)?;
    let mut safe: mxr_config::MxrConfig =
        serde_json::from_slice(&std::fs::read(stage.path().join("settings.json"))?)?;
    for account in safe.accounts.values_mut() {
        account.enabled = false;
    }
    safe.llm.enabled = false;
    safe.search.semantic.enabled = false;
    safe.search.semantic.auto_download_models = false;
    std::fs::write(
        stage.path().join("config.toml"),
        toml::to_string_pretty(&safe)?,
    )?;
    std::fs::remove_file(stage.path().join("manifest.json"))?;
    if !dry_run {
        publish(stage.path(), &destination)?;
    }
    Ok(manifest)
}

#[cfg(test)]
mod settings_tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::settings;

    #[test]
    fn portable_settings_keep_colour_and_tag_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[accounts.work]
name = "Work"
email = "bk@work.example"
color = "amber"
tag = "W"
"#,
        )
        .unwrap();
        let sanitized = settings(Some(&path)).unwrap();
        let written = toml::to_string(&sanitized).unwrap();
        let reread: mxr_config::MxrConfig = toml::from_str(&written).unwrap();
        let work = &reread.accounts["work"];
        assert_eq!(work.color.as_deref(), Some("amber"));
        assert_eq!(work.tag.as_deref(), Some("W"));
        assert!(!work.enabled);
    }
}
