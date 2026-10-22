use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::types::{AccountConfig, MxrConfig};

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
    #[error("account '{key}' has an invalid identity: {message}")]
    AccountIdentity { key: String, message: String },
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

/// Returns the path to the disk-first credential store (`secrets.toml`).
///
/// This is the authoritative location for password-backed IMAP/SMTP secrets.
/// See [`crate::SecretStore`] for the storage format and the plaintext-at-0600
/// tradeoff. Override with `MXR_SECRETS_PATH` (used by tests for isolation).
pub fn secrets_file_path() -> PathBuf {
    if let Some(path) = env_path("MXR_SECRETS_PATH") {
        return path;
    }
    config_dir().join("secrets.toml")
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

/// Environment variable that supplies the daemon IPC bearer token directly,
/// overriding the on-disk token file (phase 5, transport adapters).
pub const DAEMON_TOKEN_ENV: &str = "MXR_DAEMON_TOKEN";

/// The on-disk path for the daemon IPC token — the secret the TCP-loopback
/// transport authenticates with.
///
/// **This is a DIFFERENT secret from the HTTP bridge token** ([`bridge_token_path`]).
/// The bridge deliberately hands its token to any loopback caller via the
/// `GET /api/v1/auth/local-token` endpoint (to bootstrap the web SPA without a
/// paste), so reusing it for raw-IPC auth would let any local process fetch it
/// over HTTP and then reach the daemon over TCP. The IPC token is never exposed
/// by any HTTP endpoint. Override with `MXR_DAEMON_TOKEN_PATH`.
pub fn daemon_token_path() -> PathBuf {
    if let Some(path) = env_path("MXR_DAEMON_TOKEN_PATH") {
        return path;
    }
    config_dir().join("daemon-token")
}

/// Resolve the daemon IPC bearer token (the TCP transport's secret).
///
/// Precedence: `MXR_DAEMON_TOKEN` (env, when set and non-empty) **>** the token
/// file at [`daemon_token_path`] (mode 0600, never HTTP-exposed). When `create`
/// is true and no file exists, a fresh 0600 token is minted atomically (the
/// daemon does this at startup so the listener has a token to check); when
/// `create` is false the file is only read (a client dialing `tcp://` with no
/// token available returns `None` and simply fails the server's auth gate).
///
/// Reading an existing file re-asserts mode 0600 (tightening a too-open file),
/// and creation uses an exclusive `O_EXCL` open so a concurrent daemon start
/// cannot race two different tokens onto disk.
pub fn resolve_daemon_token(create: bool) -> std::io::Result<Option<String>> {
    if let Ok(value) = std::env::var(DAEMON_TOKEN_ENV) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    let path = daemon_token_path();
    if create {
        read_or_create_secret_0600(&path).map(Some)
    } else {
        read_secret_enforcing_0600(&path)
    }
}

/// Read an existing 0600 secret file, tightening its permissions to 0600 if a
/// wider mode is found. `Ok(None)` for a missing or empty file.
fn read_secret_enforcing_0600(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let token = contents.trim();
            if token.is_empty() {
                return Ok(None);
            }
            enforce_mode_0600(path)?;
            Ok(Some(token.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Read the secret, or mint one atomically. Uses an exclusive create so two
/// daemons racing to start can never write two different tokens: the loser
/// observes `AlreadyExists` and re-reads the winner's file.
fn read_or_create_secret_0600(path: &Path) -> std::io::Result<String> {
    if let Some(token) = read_secret_enforcing_0600(path)? {
        return Ok(token);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let token = uuid::Uuid::now_v7().to_string();
    match create_new_secret_0600(path, &token) {
        Ok(()) => Ok(token),
        // Raced by another process between the read and the exclusive create:
        // use whatever it wrote rather than clobbering it.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_secret_enforcing_0600(path)?.ok_or_else(|| {
                std::io::Error::other("daemon token file appeared then vanished during creation")
            })
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn create_new_secret_0600(path: &Path, token: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true) // O_EXCL: fail if it already exists
        .mode(0o600)
        .open(path)?;
    file.write_all(token.as_bytes())?;
    file.write_all(b"\n")
}

#[cfg(not(unix))]
fn create_new_secret_0600(path: &Path, token: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(token.as_bytes())?;
    file.write_all(b"\n")
}

#[cfg(unix)]
fn enforce_mode_0600(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path)?;
    if metadata.permissions().mode() & 0o777 == 0o600 {
        return Ok(());
    }
    let mut perms = metadata.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn enforce_mode_0600(_path: &Path) -> std::io::Result<()> {
    Ok(())
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

/// An exclusive advisory lock on the config file, held until it is dropped.
/// It lives on a sibling `config.toml.lock`, not the config file itself, so
/// saving the config never replaces the file being locked.
#[derive(Debug)]
pub struct ConfigFileLock {
    _file: std::fs::File,
}

fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

/// Wait for the exclusive lock on the config file at `path`. Everything that
/// reads the config, changes it and writes it back holds this across the
/// three, in this process or another (`mxr config set` runs in its own), so
/// one writer's stale copy never replaces another's change.
pub fn lock_config_file_at(path: &Path) -> Result<ConfigFileLock, ConfigError> {
    let lock = lock_path(path);
    let io = |source| ConfigError::ReadFile {
        path: lock.clone(),
        source,
    };
    if let Some(parent) = lock.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .map_err(io)?;
    file.lock().map_err(io)?;
    Ok(ConfigFileLock { _file: file })
}

/// [`lock_config_file_at`] for the default config file.
pub fn lock_config_file() -> Result<ConfigFileLock, ConfigError> {
    lock_config_file_at(&config_file_path())
}

/// Load the config at `path`, change it and save it, holding the config file
/// lock throughout. Nothing is saved when `mutate` fails.
pub fn update_config_at_path<T, E: From<ConfigError>>(
    path: &Path,
    mutate: impl FnOnce(&mut MxrConfig) -> Result<T, E>,
) -> Result<T, E> {
    let _lock = lock_config_file_at(path)?;
    let mut config = load_config_from_path(path)?;
    let out = mutate(&mut config)?;
    save_config_to_path(&config, path)?;
    Ok(out)
}

/// [`update_config_at_path`] for the default config file.
pub fn update_config<T, E: From<ConfigError>>(
    mutate: impl FnOnce(&mut MxrConfig) -> Result<T, E>,
) -> Result<T, E> {
    update_config_at_path(&config_file_path(), mutate)
}

/// Settings that were removed because nothing read them. Serde ignores unknown
/// fields, so an old `config.toml` that still sets them keeps loading; the
/// loader names them once instead of letting them look like working settings.
pub const RETIRED_CONFIG_KEYS: &[&str] = &[
    "appearance.sidebar",
    "appearance.date_format",
    "appearance.date_format_full",
    "appearance.subject_max_width",
    "humanizer.apply_to_summaries",
];

/// The retired keys that appear in a parsed config document, in
/// [`RETIRED_CONFIG_KEYS`] order.
pub fn retired_config_keys_in(table: &toml::Table) -> Vec<&'static str> {
    RETIRED_CONFIG_KEYS
        .iter()
        .copied()
        .filter(|key| {
            let Some((section, field)) = key.split_once('.') else {
                return false;
            };
            table
                .get(section)
                .and_then(toml::Value::as_table)
                .is_some_and(|table| table.contains_key(field))
        })
        .collect()
}

/// Warns once per process. Logs key names only, never values.
fn warn_retired_config_keys_once(contents: &str) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    // The daemon loads config to pick its log level before the subscriber
    // exists; warning then would drop the line and use up the once.
    if !tracing::dispatcher::has_been_set() {
        return;
    }
    let Ok(table) = contents.parse::<toml::Table>() else {
        return;
    };
    let keys = retired_config_keys_in(&table);
    if keys.is_empty() {
        return;
    }
    WARNED.call_once(|| {
        tracing::warn!("config keys ignored: no longer used: {}", keys.join(", "));
    });
}

/// Load config from a specific file path. Returns defaults if file doesn't exist.
pub fn load_config_from_path(path: &Path) -> Result<MxrConfig, ConfigError> {
    let mut config = match std::fs::read_to_string(path) {
        Ok(contents) => {
            warn_retired_config_keys_once(&contents);
            load_config_from_str(&contents).map_err(|e| match e {
                ConfigError::ParseToml { source, .. } => ConfigError::ParseToml {
                    path: path.to_path_buf(),
                    source,
                },
                other => other,
            })?
        }
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
    let mut config: MxrConfig = toml::from_str(toml_str).map_err(|e| ConfigError::ParseToml {
        path: PathBuf::from("<string>"),
        source: e,
    })?;
    validate_account_identities(&mut config.accounts)?;
    Ok(config)
}

/// Checks every configured tag and normalises it to uppercase. Two accounts
/// cannot share a configured tag. The error names the one that got there first
/// in key order. Tags derived from names are not checked here: the daemon
/// derives those around the configured ones.
pub fn validate_account_identities(
    accounts: &mut HashMap<String, AccountConfig>,
) -> Result<(), ConfigError> {
    let mut keys: Vec<String> = accounts.keys().cloned().collect();
    keys.sort();
    let mut holders: HashMap<String, String> = HashMap::new();
    for key in keys {
        let Some(account) = accounts.get_mut(&key) else {
            continue;
        };
        if let Some(raw_color) = &account.color {
            raw_color
                .parse::<mxr_core::AccountColor>()
                .map_err(|error| ConfigError::AccountIdentity {
                    key: key.clone(),
                    message: error.to_string(),
                })?;
        }
        let Some(raw_tag) = account.tag.take() else {
            continue;
        };
        let tag = mxr_core::parse_account_tag(&raw_tag).map_err(|error| {
            ConfigError::AccountIdentity {
                key: key.clone(),
                message: error.to_string(),
            }
        })?;
        if let Some(holder) = holders.get(&tag) {
            return Err(ConfigError::AccountIdentity {
                key: key.clone(),
                message: format!("tag '{tag}' is already set on account '{holder}'"),
            });
        }
        holders.insert(tag.clone(), key.clone());
        account.tag = Some(tag);
    }
    Ok(())
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
