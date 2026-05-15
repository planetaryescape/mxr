use std::path::{Path, PathBuf};

use crate::types::MxrConfig;

pub const PROD_INSTANCE_NAME: &str = "mxr";
pub const DEV_INSTANCE_NAME: &str = "mxr-dev";

/// Errors that can occur during config loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file at {path}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse TOML config at {path}")]
    ParseToml {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to serialize TOML config at {path}")]
    SerializeToml {
        path: PathBuf,
        source: toml::ser::Error,
    },
}

/// Returns the mxr config directory (e.g. `~/.config/mxr` on Linux/macOS).
pub fn config_dir() -> PathBuf {
    if let Some(path) = env_path("MXR_CONFIG_DIR") {
        return path;
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_instance_name())
}

/// Instance name reserved for the isolated demo profile started by `mxr demo`.
/// Exported so clients (TUI, web bridge) can detect demo mode without
/// depending on the daemon crate.
pub const DEMO_INSTANCE_NAME: &str = "mxr-demo";

/// Returns the runtime instance name used for data/socket namespacing.
///
/// Defaults:
/// - release builds: `mxr`
/// - debug builds: `mxr-dev`
/// - override with `MXR_INSTANCE`
pub fn app_instance_name() -> String {
    if let Ok(value) = std::env::var("MXR_INSTANCE") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if cfg!(debug_assertions) {
        DEV_INSTANCE_NAME.to_string()
    } else {
        PROD_INSTANCE_NAME.to_string()
    }
}

/// True when the current process is bound to the demo profile. Cheap to call;
/// reads only an env var.
pub fn is_demo_instance() -> bool {
    app_instance_name() == DEMO_INSTANCE_NAME
}

/// Returns the path to the main config file.
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the mxr data directory (e.g. `~/.local/share/mxr` on Linux).
pub fn data_dir() -> PathBuf {
    if let Some(path) = env_path("MXR_DATA_DIR") {
        return path;
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_instance_name())
}

/// Returns the account/OAuth token directory for the current runtime identity.
pub fn token_dir() -> PathBuf {
    if let Some(path) = env_path("MXR_TOKEN_DIR") {
        return path;
    }
    data_dir().join("tokens")
}

/// Scope an OS-keychain service/ref to the current runtime identity.
///
/// Production keeps legacy service names so installed users retain existing
/// credentials. Non-production instances get an identity prefix, preventing a
/// dev daemon from silently reading/writing prod keychain entries even if the
/// config was copied verbatim.
pub fn scoped_credential_service(service: &str) -> String {
    let trimmed = service.trim();
    if trimmed.is_empty() || app_instance_name() == PROD_INSTANCE_NAME {
        return trimmed.to_string();
    }
    format!(
        "{}/{}",
        app_instance_name(),
        trimmed.trim_start_matches('/')
    )
}

/// Gmail OAuth stores a scoped token cache in the OS keychain.
pub fn gmail_oauth_keychain_service() -> String {
    if app_instance_name() == PROD_INSTANCE_NAME {
        "mxr-gmail-oauth".to_string()
    } else {
        format!("{}-gmail-oauth", app_instance_name())
    }
}

/// Default directory for user-initiated attachment saves. Prefers the
/// platform's XDG `Downloads` (or macOS `~/Downloads`), falling back to
/// `~/Downloads` if `dirs::download_dir` is unset.
pub fn default_download_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    })
}

/// Returns the on-disk path where the local daemon's HTTP bridge bearer
/// token is persisted. Created lazily by `read_or_create_bridge_token`.
pub fn bridge_token_path() -> PathBuf {
    if let Some(path) = env_path("MXR_BRIDGE_TOKEN_PATH") {
        return path;
    }
    config_dir().join("bridge-token")
}

/// Returns the on-disk path where the bridge writes the port it actually
/// bound to. Useful for clients (Vite dev proxy, `mxr web`, scripts) that
/// need to discover the port after the bridge applied EADDRINUSE retries.
///
/// File contents are the bare port number on a single line, no trailing
/// newline required.
pub fn bridge_port_path() -> PathBuf {
    if let Some(path) = env_path("MXR_BRIDGE_PORT_PATH") {
        return path;
    }
    config_dir().join("bridge-port")
}

/// Atomically write the bound bridge port to `bridge_port_path()`. Errors
/// from this function are non-fatal — log and continue; clients can still
/// discover the port via `mxr status` or `--print-url`.
pub fn write_bridge_port(port: u16) -> std::io::Result<()> {
    let path = bridge_port_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("port.tmp");
    std::fs::write(&tmp, format!("{port}\n"))?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove the bridge-port file. Called when the bridge fails to start or
/// shuts down, so a subsequent `mxr web` invocation doesn't probe a port
/// that's no longer ours and then time out waiting for a detached child
/// it would have spawned faster if it knew the cached port was stale.
pub fn clear_bridge_port() {
    let _ = std::fs::remove_file(bridge_port_path());
}

/// Read the bridge port the daemon last bound to. Returns `None` if the
/// file is missing or unparseable.
pub fn read_bridge_port() -> Option<u16> {
    std::fs::read_to_string(bridge_port_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Returns the on-disk path for a token that authenticates against a remote
/// daemon at `host`. The web-launcher writes one of these per remote host so
/// `mxr web --remote-host foo.example.com` opens the browser pre-authenticated.
pub fn remote_bridge_token_path(host: &str) -> PathBuf {
    let safe: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    config_dir()
        .join("bridge-tokens")
        .join(format!("{safe}.token"))
}

/// Reads the persisted bridge token from disk, or generates and writes a new
/// UUID v4 token if none exists. The file is created with mode 0600 on Unix.
pub fn read_or_create_bridge_token() -> std::io::Result<String> {
    let path = bridge_token_path();
    if let Ok(contents) = std::fs::read_to_string(&path) {
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let token = uuid::Uuid::now_v7().to_string();
    write_secret(&path, &token)?;
    Ok(token)
}

#[cfg(unix)]
fn write_secret(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    use std::io::Write;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

/// Returns the IPC socket path for the current instance.
pub fn socket_path() -> PathBuf {
    if let Some(path) = env_path("MXR_SOCKET_PATH") {
        return path;
    }
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join(app_instance_name())
            .join("mxr.sock")
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(app_instance_name())
            .join("mxr.sock")
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

/// Load config from the default config file path, falling back to defaults.
pub fn load_config() -> Result<MxrConfig, ConfigError> {
    load_config_from_path(&config_file_path())
}

/// Save config to the default config file path.
pub fn save_config(config: &MxrConfig) -> Result<(), ConfigError> {
    save_config_to_path(config, &config_file_path())
}

/// Load config from a specific file path. Returns defaults if file doesn't exist.
pub fn load_config_from_path(path: &Path) -> Result<MxrConfig, ConfigError> {
    let mut config = match std::fs::read_to_string(path) {
        Ok(contents) => load_config_from_str(&contents).map_err(|e| match e {
            ConfigError::ParseToml { source, .. } => ConfigError::ParseToml {
                path: path.to_path_buf(),
                source,
            },
            other => other,
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(
                "config file not found at {}, using defaults",
                path.display()
            );
            MxrConfig::default()
        }
        Err(e) => {
            return Err(ConfigError::ReadFile {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };

    apply_env_overrides(&mut config);
    Ok(config)
}

/// Save config to a specific path.
pub fn save_config_to_path(config: &MxrConfig, path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConfigError::ReadFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let contents = toml::to_string_pretty(config).map_err(|source| ConfigError::SerializeToml {
        path: path.to_path_buf(),
        source,
    })?;
    std::fs::write(path, contents).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

/// Load config from a TOML string.
pub fn load_config_from_str(toml_str: &str) -> Result<MxrConfig, ConfigError> {
    toml::from_str(toml_str).map_err(|e| ConfigError::ParseToml {
        path: PathBuf::from("<string>"),
        source: e,
    })
}

/// Apply environment variable overrides to the config.
fn apply_env_overrides(config: &mut MxrConfig) {
    if let Ok(val) = std::env::var("MXR_EDITOR") {
        config.general.editor = Some(val);
    }
    if let Ok(val) = std::env::var("MXR_SYNC_INTERVAL") {
        if let Ok(interval) = val.parse::<u64>() {
            config.general.sync_interval = interval;
        }
    }
    if let Ok(val) = std::env::var("MXR_DEFAULT_ACCOUNT") {
        config.general.default_account = Some(val);
    }
    if let Ok(val) = std::env::var("MXR_ATTACHMENT_DIR") {
        config.general.attachment_dir = PathBuf::from(val);
    }
    if let Ok(val) = std::env::var("MXR_DOWNLOAD_DIR") {
        config.general.download_dir = PathBuf::from(val);
    }
    if let Ok(val) = std::env::var("MXR_SAFETY_POLICY") {
        if let Some(policy) = crate::types::SafetyPolicy::parse_env(&val) {
            config.general.safety_policy = policy;
        }
    }
}
