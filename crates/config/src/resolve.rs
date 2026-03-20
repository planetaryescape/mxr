use std::path::{Path, PathBuf};

use crate::types::MxrConfig;

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
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mxr")
}

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
        "mxr-dev".to_string()
    } else {
        "mxr".to_string()
    }
}

/// Returns the path to the main config file.
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the mxr data directory (e.g. `~/.local/share/mxr` on Linux).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_instance_name())
}

/// Returns the IPC socket path for the current instance.
pub fn socket_path() -> PathBuf {
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
}
