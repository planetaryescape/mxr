//! Transport address parsing and resolution.
//!
//! Docker's precedent: a small scheme names an endpoint (`unix://<path>`). Only
//! `unix://` exists this phase; `tcp://` and `cmd://` arrive in phase 5.
//! Resolution order: explicit env (`MXR_DAEMON_ADDR`) → default per-instance
//! socket path. (A config-file tier slots in between once a daemon-address
//! config field exists.) When the env var is unset, resolution returns the
//! caller's default path unchanged — today's behavior.

use std::path::PathBuf;

use crate::error::{Result, TransportError};

/// Environment variable that overrides the daemon address.
pub const DAEMON_ADDR_ENV: &str = "MXR_DAEMON_ADDR";

/// A parsed daemon transport address. Single-variant this phase; additive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportAddr {
    /// A Unix domain socket at the given path (`unix://<path>`).
    Unix(PathBuf),
}

impl TransportAddr {
    /// Parse a transport address string. Only `unix://<path>` is accepted this
    /// phase.
    pub fn parse(value: &str) -> Result<Self> {
        if let Some(path) = value.strip_prefix("unix://") {
            if path.is_empty() {
                return Err(TransportError::Addr(
                    "unix:// requires a socket path".to_string(),
                ));
            }
            Ok(Self::Unix(PathBuf::from(path)))
        } else {
            Err(TransportError::Addr(format!(
                "unsupported transport address {value:?} (only unix://<path> is supported)"
            )))
        }
    }

    /// Resolve the daemon address: `MXR_DAEMON_ADDR` if set and non-empty, else
    /// the supplied default socket path (current behavior when unset).
    pub fn resolve(default_socket: PathBuf) -> Result<Self> {
        match std::env::var(DAEMON_ADDR_ENV) {
            Ok(value) if !value.trim().is_empty() => Self::parse(value.trim()),
            _ => Ok(Self::Unix(default_socket)),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::*;

    #[test]
    fn parses_unix_scheme() {
        assert_eq!(
            TransportAddr::parse("unix:///tmp/mxr.sock").unwrap(),
            TransportAddr::Unix(PathBuf::from("/tmp/mxr.sock"))
        );
    }

    #[test]
    fn rejects_empty_unix_path() {
        assert!(TransportAddr::parse("unix://").is_err());
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(TransportAddr::parse("tcp://127.0.0.1:9000").is_err());
    }

    #[test]
    fn resolve_falls_back_to_default_when_env_unset() {
        // Uses a unique var name assumption: this test must not run with
        // MXR_DAEMON_ADDR set. The default is returned verbatim.
        temp_env_remove(|| {
            let default = PathBuf::from("/run/mxr/mxr.sock");
            assert_eq!(
                TransportAddr::resolve(default.clone()).unwrap(),
                TransportAddr::Unix(default)
            );
        });
    }

    // Minimal scoped env guard so the resolve test does not depend on ambient
    // MXR_DAEMON_ADDR. Restores the previous value on drop.
    fn temp_env_remove(body: impl FnOnce()) {
        let previous = std::env::var(DAEMON_ADDR_ENV).ok();
        std::env::remove_var(DAEMON_ADDR_ENV);
        body();
        if let Some(value) = previous {
            std::env::set_var(DAEMON_ADDR_ENV, value);
        }
    }
}
