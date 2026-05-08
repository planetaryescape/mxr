//! Managed HTTP bridge — runs as a tokio task spawned from `mxr daemon`,
//! sharing the daemon's shutdown signal so its TCP listener releases
//! cleanly when the daemon exits.
//!
//! Standalone `mxr web` keeps using `mxr_web::serve` directly; this module
//! exists purely for the daemon-hosted code path.

use crate::server::BridgeOverrides;
use crate::state::AppState;
use mxr_config::{config_dir, load_config, BridgeConfig};
use mxr_web::WebServerConfig;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BridgeStartupError {
    #[error("config load: {0}")]
    Config(String),

    #[error(
        "bridge bind {bind} is non-loopback; refusing to start without TLS \
         (set [bridge].bind = \"127.0.0.1\" or run `mxr daemon --no-bridge`)"
    )]
    NonLoopbackWithoutTls { bind: String },

    #[error("bridge bind address {bind} is not a valid IP: {error}")]
    InvalidBind { bind: String, error: String },

    #[error("bridge listener bind {addr}: {error}")]
    Bind { addr: SocketAddr, error: String },

    #[error("bridge token file {path}: {error}")]
    Token { path: PathBuf, error: String },
}

/// Spawn the bridge loop or return `None` if disabled in config.
pub async fn spawn_bridge_loop(
    state: Arc<AppState>,
    overrides: &BridgeOverrides,
) -> Result<Option<JoinHandle<()>>, BridgeStartupError> {
    let config = load_config().map_err(|e| BridgeStartupError::Config(e.to_string()))?;
    let mut bridge_cfg = config.bridge;

    if let Some(port) = overrides.port {
        bridge_cfg.port = port;
    }

    if !bridge_cfg.enabled {
        return Ok(None);
    }

    enforce_non_loopback_safety(&bridge_cfg)?;

    let bind: IpAddr = bridge_cfg
        .bind
        .parse()
        .map_err(
            |error: std::net::AddrParseError| BridgeStartupError::InvalidBind {
                bind: bridge_cfg.bind.clone(),
                error: error.to_string(),
            },
        )?;
    let addr = SocketAddr::new(bind, bridge_cfg.port);

    let token = load_or_create_token(&bridge_cfg)?;
    let socket_path = AppState::socket_path();
    let web_config = WebServerConfig::new(socket_path, token)
        .with_cors_allowlist(bridge_cfg.cors_allowlist.clone())
        .with_host_allowlist(bridge_cfg.host_allowlist.clone());

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|error| BridgeStartupError::Bind {
            addr,
            error: error.to_string(),
        })?;
    let bound = listener.local_addr().unwrap_or(addr);
    tracing::info!("HTTP bridge listening on {bound}");

    let mut shutdown_rx = state.shutdown_receiver();
    let handle = tokio::spawn(async move {
        let server = mxr_web::serve(listener, web_config);
        tokio::select! {
            result = server => {
                if let Err(error) = result {
                    tracing::error!("bridge server stopped with error: {error}");
                }
            }
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("bridge_loop exiting: shutdown requested");
                }
            }
        }
    });

    Ok(Some(handle))
}

fn enforce_non_loopback_safety(cfg: &BridgeConfig) -> Result<(), BridgeStartupError> {
    if cfg.is_loopback_bind() {
        return Ok(());
    }
    // Non-loopback binds require TLS to avoid leaking the bridge token in
    // plaintext. TLS termination isn't shipped yet (out of scope for
    // v0.5), so refuse the bind here with a clear message.
    Err(BridgeStartupError::NonLoopbackWithoutTls {
        bind: cfg.bind.clone(),
    })
}

/// Load the token from `bridge.token_path` (default
/// `~/.config/mxr/bridge-token`). If the file doesn't exist, generate a
/// random UUID, write it with mode 0600, and return it.
fn load_or_create_token(cfg: &BridgeConfig) -> Result<String, BridgeStartupError> {
    let path = cfg
        .token_path
        .clone()
        .unwrap_or_else(|| config_dir().join("bridge-token"));

    if let Ok(bytes) = std::fs::read(&path) {
        let token = String::from_utf8_lossy(&bytes).trim().to_string();
        if !token.is_empty() {
            ensure_strict_permissions(&path)?;
            return Ok(token);
        }
    }

    let token = Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| BridgeStartupError::Token {
            path: path.clone(),
            error: error.to_string(),
        })?;
    }
    write_token_file(&path, &token)?;
    Ok(token)
}

#[cfg(unix)]
fn write_token_file(path: &PathBuf, token: &str) -> Result<(), BridgeStartupError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true).mode(0o600);
    let mut file = opts.open(path).map_err(|error| BridgeStartupError::Token {
        path: path.clone(),
        error: error.to_string(),
    })?;
    use std::io::Write;
    file.write_all(token.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| BridgeStartupError::Token {
            path: path.clone(),
            error: error.to_string(),
        })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_token_file(path: &PathBuf, token: &str) -> Result<(), BridgeStartupError> {
    std::fs::write(path, format!("{token}\n")).map_err(|error| BridgeStartupError::Token {
        path: path.clone(),
        error: error.to_string(),
    })
}

#[cfg(unix)]
fn ensure_strict_permissions(path: &PathBuf) -> Result<(), BridgeStartupError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path).map_err(|error| BridgeStartupError::Token {
        path: path.clone(),
        error: error.to_string(),
    })?;
    let mode = metadata.permissions().mode() & 0o777;
    if mode == 0o600 {
        return Ok(());
    }
    let mut perms = metadata.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms).map_err(|error| BridgeStartupError::Token {
        path: path.clone(),
        error: error.to_string(),
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn ensure_strict_permissions(_path: &PathBuf) -> Result<(), BridgeStartupError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_config::BridgeConfig;
    use tempfile::TempDir;

    #[test]
    fn enforce_non_loopback_safety_accepts_loopback_addresses() {
        for bind in ["127.0.0.1", "::1", "[::1]", "localhost"] {
            let cfg = BridgeConfig {
                bind: bind.into(),
                ..BridgeConfig::default()
            };
            assert!(enforce_non_loopback_safety(&cfg).is_ok(), "{bind}");
        }
    }

    #[test]
    fn enforce_non_loopback_safety_rejects_external_addresses() {
        let cfg = BridgeConfig {
            bind: "0.0.0.0".into(),
            ..BridgeConfig::default()
        };
        let error = enforce_non_loopback_safety(&cfg).unwrap_err();
        assert!(
            matches!(error, BridgeStartupError::NonLoopbackWithoutTls { .. }),
            "expected non-loopback rejection, got {error:?}"
        );
    }

    #[test]
    fn token_file_round_trips_existing_value() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("bridge-token");
        std::fs::write(&path, "existing-token-abc\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let cfg = BridgeConfig {
            token_path: Some(path.clone()),
            ..BridgeConfig::default()
        };
        let token = load_or_create_token(&cfg).unwrap();
        assert_eq!(token, "existing-token-abc");
    }

    #[test]
    fn token_file_is_generated_with_mode_600() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("subdir").join("bridge-token");
        let cfg = BridgeConfig {
            token_path: Some(path.clone()),
            ..BridgeConfig::default()
        };
        let token = load_or_create_token(&cfg).unwrap();
        assert!(!token.is_empty());
        assert!(path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "bridge-token must be 0600");
        }
    }

    #[test]
    fn token_file_permissions_are_repaired_if_too_open() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("bridge-token");
        std::fs::write(&path, "loose-token\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let cfg = BridgeConfig {
            token_path: Some(path.clone()),
            ..BridgeConfig::default()
        };
        let token = load_or_create_token(&cfg).unwrap();
        assert_eq!(token, "loose-token");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "loose token-file permissions must be tightened on load"
            );
        }
    }
}
