//! Transport address parsing and resolution.
//!
//! Docker's precedent: a small scheme names an endpoint (`unix://<path>`). Only
//! `unix://` exists this phase; `tcp://` and `cmd://` arrive in phase 5.
//!
//! ## `unix://` grammar
//!
//! Everything after the literal `unix://` prefix is the socket path, taken
//! verbatim:
//! - **Empty** (`unix://`) is rejected — a path is required.
//! - **Absolute** (`unix:///run/mxr.sock` → `/run/mxr.sock`) and **relative**
//!   (`unix://run/mxr.sock` → `run/mxr.sock`, resolved against the process CWD
//!   by the OS at connect/bind time) are both accepted; relative paths are not
//!   rewritten.
//! - **Percent-escapes are literal**: `unix://a%20b` is the 4-char path `a%20b`,
//!   not `a b`. This is a filesystem path, not a URL — no percent-decoding.
//!
//! ## Resolution precedence
//!
//! `MXR_DAEMON_ADDR` (if set and non-empty) **>** the caller's default socket
//! path (which itself honors `MXR_SOCKET_PATH` **>** the per-instance default).
//! When `MXR_DAEMON_ADDR` is unset, resolution returns the default unchanged —
//! today's behavior. (A config-file tier slots in below the env var once a
//! daemon-address config field exists.)

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
    fn relative_unix_path_is_kept_verbatim() {
        // Relative paths are accepted as-is (the OS resolves them against CWD
        // at connect/bind time); they are not rewritten to absolute.
        assert_eq!(
            TransportAddr::parse("unix://run/mxr.sock").unwrap(),
            TransportAddr::Unix(PathBuf::from("run/mxr.sock"))
        );
    }

    #[test]
    fn percent_escapes_are_literal_not_decoded() {
        // A filesystem path, not a URL: `%20` stays four characters.
        assert_eq!(
            TransportAddr::parse("unix://a%20b.sock").unwrap(),
            TransportAddr::Unix(PathBuf::from("a%20b.sock"))
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

    /// One serialized test owns the `MXR_DAEMON_ADDR` mutation window, so it
    /// never races another test reading the global env. Covers both precedence
    /// directions: env unset → default returned verbatim; env set → it wins over
    /// the default.
    #[test]
    fn resolve_precedence_env_over_default() {
        let previous = std::env::var(DAEMON_ADDR_ENV).ok();
        let default = PathBuf::from("/run/mxr/mxr.sock");

        // Unset → default returned unchanged.
        std::env::remove_var(DAEMON_ADDR_ENV);
        assert_eq!(
            TransportAddr::resolve(default.clone()).unwrap(),
            TransportAddr::Unix(default.clone())
        );

        // Set → MXR_DAEMON_ADDR wins over the default.
        std::env::set_var(DAEMON_ADDR_ENV, "unix:///tmp/override.sock");
        assert_eq!(
            TransportAddr::resolve(default.clone()).unwrap(),
            TransportAddr::Unix(PathBuf::from("/tmp/override.sock"))
        );

        // Blank/whitespace → treated as unset.
        std::env::set_var(DAEMON_ADDR_ENV, "   ");
        assert_eq!(
            TransportAddr::resolve(default.clone()).unwrap(),
            TransportAddr::Unix(default)
        );

        match previous {
            Some(value) => std::env::set_var(DAEMON_ADDR_ENV, value),
            None => std::env::remove_var(DAEMON_ADDR_ENV),
        }
    }
}
