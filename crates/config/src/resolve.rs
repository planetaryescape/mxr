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
    #[error("failed to re-read the serialized TOML config for {path}")]
    ReparseToml {
        path: PathBuf,
        source: toml_edit::TomlError,
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

/// The default config file as written on disk, without environment overrides.
/// Every read-modify-write must start here: a value taken from `MXR_*` in the
/// environment would otherwise be saved into the file.
pub fn load_config_as_written() -> Result<MxrConfig, ConfigError> {
    load_config_file_as_written(&config_file_path())
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
    let mut config = load_config_file_as_written(path)?;
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
    warn_legacy_draft_models_once(&table);
    let keys = retired_config_keys_in(&table);
    if keys.is_empty() {
        return;
    }
    WARNED.call_once(|| {
        tracing::warn!("config keys ignored: no longer used: {}", keys.join(", "));
    });
}

fn warn_legacy_draft_models_once(table: &toml::Table) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    let Some(overrides) = table
        .get("llm")
        .and_then(toml::Value::as_table)
        .and_then(|llm| llm.get("overrides"))
        .and_then(toml::Value::as_table)
    else {
        return;
    };
    let keys = [
        "draft_assist",
        "draft_new",
        "draft_refine",
        "humanize_rewrite",
    ];
    let found: Vec<_> = keys
        .iter()
        .copied()
        .filter(|key| overrides.contains_key(*key))
        .collect();
    if !found.is_empty() {
        WARNED.call_once(|| {
            tracing::warn!(
                "legacy drafting model overrides ({}) are still loaded; use [llm.compose] model",
                found.join(", ")
            );
        });
    }
}

/// Load config from a specific file path, with environment overrides applied.
/// Returns defaults if the file doesn't exist.
pub fn load_config_from_path(path: &Path) -> Result<MxrConfig, ConfigError> {
    let mut config = load_config_file_as_written(path)?;
    apply_env_overrides(&mut config);
    Ok(config)
}

/// Load the config file at `path` exactly as written. Returns defaults if the
/// file doesn't exist. Environment overrides are not applied.
pub fn load_config_file_as_written(path: &Path) -> Result<MxrConfig, ConfigError> {
    let config = match std::fs::read_to_string(path) {
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

    Ok(config)
}

/// Save config to a specific path.
///
/// The file is rewritten in place of the old one, not replaced wholesale: keys
/// the config still sets take their new values and keep their comments and
/// layout, and keys it no longer sets are removed. The write goes through a
/// temporary file and a rename, so an interrupted save leaves the old file.
pub fn save_config_to_path(config: &MxrConfig, path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConfigError::ReadFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let existing = match std::fs::read_to_string(path) {
        Ok(contents) => Some(contents),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(ConfigError::ReadFile {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    let contents = render_config(config, existing.as_deref(), path)?;
    write_file_atomically(path, contents.as_bytes())
}

/// The file text for `config`, laid over `existing` when that parses. An
/// existing file that does not parse is replaced: nothing in it could be kept.
fn render_config(
    config: &MxrConfig,
    existing: Option<&str>,
    path: &Path,
) -> Result<String, ConfigError> {
    let serialized =
        toml::to_string_pretty(config).map_err(|source| ConfigError::SerializeToml {
            path: path.to_path_buf(),
            source,
        })?;
    let fresh: toml_edit::DocumentMut =
        serialized
            .parse()
            .map_err(|source| ConfigError::ReparseToml {
                path: path.to_path_buf(),
                source,
            })?;
    let Some(mut document) = existing.and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
    else {
        return Ok(fresh.to_string());
    };
    merge_table(document.as_table_mut(), fresh.as_table());
    Ok(document.to_string())
}

/// Makes `target` hold the keys and values of `source`. A value that is
/// already equal is left alone, so its formatting and comments survive; a
/// changed one keeps its decor (leading comment and trailing comment).
fn merge_table(target: &mut toml_edit::Table, source: &toml_edit::Table) {
    let stale: Vec<String> = target
        .iter()
        .map(|(key, _)| key.to_string())
        .filter(|key| !source.contains_key(key))
        .collect();
    for key in stale {
        target.remove(&key);
    }
    for (key, item) in source.iter() {
        match (target.get_mut(key), item) {
            (Some(toml_edit::Item::Table(existing)), toml_edit::Item::Table(fresh)) => {
                merge_table(existing, fresh);
            }
            (Some(existing), item) => replace_item(existing, item),
            (None, item) => {
                target.insert(key, item.clone());
            }
        }
    }
}

fn replace_item(existing: &mut toml_edit::Item, fresh: &toml_edit::Item) {
    match (existing.as_value_mut(), fresh.as_value()) {
        (Some(existing_value), Some(fresh_value)) => {
            let mut bare_existing = existing_value.clone();
            *bare_existing.decor_mut() = toml_edit::Decor::default();
            let mut bare_fresh = fresh_value.clone();
            *bare_fresh.decor_mut() = toml_edit::Decor::default();
            if bare_existing.to_string() == bare_fresh.to_string() {
                return;
            }
            let decor = existing_value.decor().clone();
            let mut replacement = fresh_value.clone();
            *replacement.decor_mut() = decor;
            *existing_value = replacement;
        }
        _ => *existing = fresh.clone(),
    }
}

/// Writes `contents` to `path` through a temporary file in the same directory:
/// write, fsync, rename, then fsync the directory so the rename is durable. A
/// failure before the rename removes the temporary file and leaves `path` as it was.
fn write_file_atomically(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    write_file_atomically_with(path, contents, || Ok(()))
}

/// [`write_file_atomically`] with a hook that runs after the temporary file is
/// durable and before the rename. The hook is the seam tests use to fail an
/// interrupted save.
fn write_file_atomically_with(
    path: &Path,
    contents: &[u8],
    before_rename: impl FnOnce() -> std::io::Result<()>,
) -> Result<(), ConfigError> {
    use std::io::Write;

    let io = |source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    };
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    name.push(format!(".tmp-{}-{stamp}", std::process::id()));
    let temp = dir.join(name);

    let write = || -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Ok(existing) = std::fs::metadata(path) {
            file.set_permissions(existing.permissions())?;
        }
        file.write_all(contents)?;
        file.sync_all()?;
        before_rename()?;
        std::fs::rename(&temp, path)
    };
    if let Err(source) = write() {
        let _ = std::fs::remove_file(&temp);
        return Err(io(source));
    }
    if let Ok(directory) = std::fs::File::open(dir) {
        let _ = directory.sync_all();
    }
    Ok(())
}

/// Load config from a TOML string.
pub fn load_config_from_str(toml_str: &str) -> Result<MxrConfig, ConfigError> {
    let config: MxrConfig = toml::from_str(toml_str).map_err(|e| ConfigError::ParseToml {
        path: PathBuf::from("<string>"),
        source: e,
    })?;
    Ok(config)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_assistant_catalogue_key_loads_and_saves_as_models() {
        let config = load_config_from_str(
            "[llm]\nassistant_models = [\"local-one\"]\n[llm.compose]\nmodel = \"local-one\"\n",
        )
        .expect("legacy catalogue config loads");
        assert_eq!(config.llm.models, ["local-one"]);
        assert_eq!(
            config
                .llm
                .compose
                .as_ref()
                .expect("compose config is retained")
                .model
                .as_deref(),
            Some("local-one")
        );
        let saved = toml::to_string(&config).expect("config serializes");
        assert!(saved.contains("models = [\"local-one\"]"));
        assert!(!saved.contains("assistant_models"));
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;

    #[test]
    fn env_override_is_not_saved_by_a_file_only_read_modify_write() {
        let dir = tempfile::tempdir().expect("config fixture");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[accounts.work]\nname = \"Work\"\nemail = \"w@example.com\"\nenabled = true\n",
        )
        .expect("config fixture");

        temp_env::with_var("MXR_DEFAULT_ACCOUNT", Some("work"), || {
            update_config_at_path(&path, |config| {
                config
                    .accounts
                    .get_mut("work")
                    .expect("config fixture")
                    .enabled = false;
                Ok::<_, ConfigError>(())
            })
            .expect("config fixture");
        });

        let saved = std::fs::read_to_string(&path).expect("config fixture");
        let written = load_config_from_str(&saved).expect("config fixture");
        assert_eq!(written.general.default_account, None, "{saved}");
        assert!(!written.accounts["work"].enabled);
    }

    #[test]
    fn a_save_keeps_comments_and_layout_of_keys_it_keeps() {
        let dir = tempfile::tempdir().expect("config fixture");
        let path = dir.path().join("config.toml");
        let original = "# my mail setup\n[general]\n# the editor I use\neditor = \"nvim\"\nsync_interval = 60\n";
        std::fs::write(&path, original).expect("config fixture");

        let mut config = load_config_file_as_written(&path).expect("config fixture");
        config.general.sync_interval = 120;
        save_config_to_path(&config, &path).expect("config fixture");

        let saved = std::fs::read_to_string(&path).expect("config fixture");
        assert!(saved.contains("# my mail setup"), "{saved}");
        assert!(saved.contains("# the editor I use"), "{saved}");
        assert!(saved.contains("sync_interval = 120"), "{saved}");
        assert!(saved.contains("editor = \"nvim\""), "{saved}");
    }

    #[test]
    fn a_failed_write_before_the_rename_leaves_the_old_file_and_no_temp_file() {
        let dir = tempfile::tempdir().expect("config fixture");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[general]\neditor = \"vim\"\n").expect("config fixture");

        let result = write_file_atomically_with(&path, b"[general]\neditor = \"emacs\"\n", || {
            Err(std::io::Error::other("interrupted"))
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("config fixture"),
            "[general]\neditor = \"vim\"\n"
        );
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("config fixture")
            .map(|entry| entry.expect("config fixture").file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("config.toml")]);
    }
}
