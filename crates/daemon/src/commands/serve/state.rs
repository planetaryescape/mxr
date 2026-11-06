//! A host's desired state is independent of any client selection.
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Hosting {
    pub version: u32,
    pub executable: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub socket: PathBuf,
    pub instance: String,
    #[serde(default)]
    pub token_dir: Option<PathBuf>,
    #[serde(default)]
    pub secrets_file: Option<PathBuf>,
    pub public_origin: String,
    pub listen: SocketAddr,
    pub workspace: mxr_protocol::ArchiveWorkspaceData,
    pub enabled: bool,
    pub daemon_enabled: bool,
    pub mapping_installed: bool,
    pub service_installed: bool,
    #[serde(default)]
    pub service_digest: Option<String>,
    #[serde(default)]
    pub service_previous_digest: Option<String>,
}

pub(super) fn path() -> PathBuf {
    mxr_config::config_dir().join("hosting.json")
}

pub(super) fn read() -> anyhow::Result<Option<Hosting>> {
    read_at(&path())
}
fn read_at(path: &Path) -> anyhow::Result<Option<Hosting>> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => {
            anyhow::bail!("Hosting state must be a regular file")
        }
        Err(e) => return Err(e.into()),
        _ => (),
    }
    let config: Hosting = serde_json::from_slice(&std::fs::read(path)?)?;
    anyhow::ensure!(config.version == 1, "Unsupported hosting state version");
    Ok(Some(config))
}

pub(super) fn write(config: &Hosting) -> anyhow::Result<()> {
    let _startup = mxr_config::lock_hosting_daemon_startup()?;
    write_at(&path(), config)
}
fn write_at(path: &Path, config: &Hosting) -> anyhow::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Hosting state has no parent"))?;
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    use std::os::unix::fs::PermissionsExt;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(&serde_json::to_vec_pretty(config)?)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    std::fs::File::open(dir)?.sync_all()?;
    Ok(())
}

/// A single command owns service/proxy reconciliation. The runner only reads state.
pub(super) fn lock() -> anyhow::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = path().with_extension("lock");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)?;
    file.try_lock()
        .map_err(|_| anyhow::anyhow!("Another hosting command is running"))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_state_survives_atomic_replacement_and_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosting.json");
        let mut config = Hosting {
            version: 1,
            executable: "/bin/mxr".into(),
            config_dir: "/cfg".into(),
            data_dir: "/data".into(),
            socket: "/sock".into(),
            instance: "fixture".into(),
            token_dir: None,
            secrets_file: None,
            public_origin: "https://fixture.ts.net".into(),
            listen: "127.0.0.1:18741".parse().unwrap(),
            workspace: mxr_protocol::ArchiveWorkspaceData {
                workspace_id: "w".into(),
                recovery_generation: "g".into(),
            },
            enabled: true,
            daemon_enabled: true,
            mapping_installed: true,
            service_installed: true,
            service_digest: None,
            service_previous_digest: None,
        };
        write_at(&path, &config).unwrap();
        config.enabled = false;
        config.daemon_enabled = false;
        write_at(&path, &config).unwrap();
        assert_eq!(read_at(&path).unwrap(), Some(config));
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
