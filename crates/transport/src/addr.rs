//! Transport address parsing and resolution.
//!
//! Docker's precedent: a small scheme names an endpoint. Three schemes exist:
//! `unix://<path>` (the default), `tcp://<host:port>` (loopback + token, phase
//! 5a), and `cmd://<command line>` (spawn-and-pipe, phase 5c).
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
//! - **Percent-escapes are literal**: `unix://a%20b` is the 5-byte path `a%20b`
//!   (`a`, `%`, `2`, `0`, `b`), not `a b`. This is a filesystem path, not a URL
//!   — no percent-decoding.
//! - The path after `unix://` is taken **verbatim** — leading/trailing spaces
//!   in a socket path are legal and preserved (`unix:// x ` is the path `" x "`).
//!
//! ## Resolution precedence
//!
//! `MXR_DAEMON_ADDR` (if set and non-empty) **>** the caller's default socket
//! path (which itself honors `MXR_SOCKET_PATH` **>** the per-instance default).
//! When `MXR_DAEMON_ADDR` is unset, resolution returns the default unchanged —
//! today's behavior. (A config-file tier slots in below the env var once a
//! daemon-address config field exists.)

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::error::{Result, TransportError};

/// Environment variable that overrides the daemon address.
pub const DAEMON_ADDR_ENV: &str = "MXR_DAEMON_ADDR";

/// A parsed daemon transport address. Additive: variants grow per adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportAddr {
    /// A Unix domain socket at the given path (`unix://<path>`).
    Unix(PathBuf),
    /// A TCP endpoint (`tcp://<host:port>`). The connector dials it and, when it
    /// carries a token, authenticates in-band (phase 5a). The daemon binds
    /// loopback-only.
    Tcp(SocketAddr),
    /// A command to spawn whose stdio IS the byte stream (`cmd://<command>`).
    /// The Docker `connhelper` model: `cmd://ssh -T host mxr daemon dial-stdio`
    /// makes any exec-and-pipe bridge a transport (phase 5c). The argv is the
    /// command line split on ASCII whitespace — see [`Self::parse`].
    Cmd(Vec<String>),
}

impl TransportAddr {
    /// Parse a transport address string: `unix://<path>`, `tcp://<host:port>`,
    /// or `cmd://<command line>`.
    ///
    /// ## `tcp://`
    ///
    /// Everything after `tcp://` is parsed as a [`SocketAddr`] (`127.0.0.1:9000`
    /// or `[::1]:9000`). A bare host without a port is rejected.
    ///
    /// ## `cmd://`
    ///
    /// Everything after `cmd://` is split on ASCII whitespace into an argv
    /// vector (`cmd://ssh -T host mxr daemon dial-stdio` →
    /// `["ssh", "-T", "host", "mxr", "daemon", "dial-stdio"]`). This is a
    /// deliberately simple split — **no shell quoting, escapes, globbing, or
    /// variable expansion**. An argument that must contain whitespace cannot be
    /// expressed here; wrap such a command in a small script and point `cmd://`
    /// at the script instead. An empty command is rejected.
    pub fn parse(value: &str) -> Result<Self> {
        if let Some(path) = value.strip_prefix("unix://") {
            if path.is_empty() {
                return Err(TransportError::Addr(
                    "unix:// requires a socket path".to_string(),
                ));
            }
            Ok(Self::Unix(PathBuf::from(path)))
        } else if let Some(authority) = value.strip_prefix("tcp://") {
            let addr = authority.parse::<SocketAddr>().map_err(|error| {
                TransportError::Addr(format!(
                    "tcp:// requires a host:port socket address (got {authority:?}): {error}"
                ))
            })?;
            Ok(Self::Tcp(addr))
        } else if let Some(command) = value.strip_prefix("cmd://") {
            let argv: Vec<String> = command.split_whitespace().map(str::to_string).collect();
            if argv.is_empty() {
                return Err(TransportError::Addr(
                    "cmd:// requires a command to spawn".to_string(),
                ));
            }
            Ok(Self::Cmd(argv))
        } else {
            Err(TransportError::Addr(format!(
                "unsupported transport address {value:?} \
                 (expected unix://<path>, tcp://<host:port>, or cmd://<command>)"
            )))
        }
    }

    /// Resolve the daemon address: `MXR_DAEMON_ADDR` if set and not
    /// whitespace-only, else the supplied default socket path (current behavior
    /// when unset). The env value is parsed **verbatim** — only empty /
    /// whitespace-only is rejected, so a socket path with leading/trailing
    /// spaces survives.
    pub fn resolve(default_socket: PathBuf) -> Result<Self> {
        match std::env::var(DAEMON_ADDR_ENV) {
            Ok(value) if !value.trim().is_empty() => Self::parse(&value),
            _ => Ok(Self::Unix(default_socket)),
        }
    }

    /// Resolve `MXR_DAEMON_ADDR` to a Unix socket path for clients that speak
    /// only `unix://` today — the TUI, the web bridge, and the MCP server. They
    /// route through the same [`resolve`](Self::resolve) as the CLI so every
    /// client agrees on the socket, but a `tcp://` / `cmd://` value is rejected
    /// with a clear message rather than silently ignored. (Full `tcp://`/`cmd://`
    /// support for these clients can follow demand; the CLI has it today.)
    pub fn resolve_unix_socket(default_socket: PathBuf) -> Result<PathBuf> {
        match Self::resolve(default_socket)? {
            Self::Unix(path) => Ok(path),
            Self::Tcp(_) => Err(TransportError::Addr(format!(
                "{DAEMON_ADDR_ENV} is a tcp:// address, but this client supports only unix://; \
                 use the mxr CLI for tcp:// access or unset {DAEMON_ADDR_ENV}"
            ))),
            Self::Cmd(_) => Err(TransportError::Addr(format!(
                "{DAEMON_ADDR_ENV} is a cmd:// address, but this client supports only unix://; \
                 use the mxr CLI for cmd:// access or unset {DAEMON_ADDR_ENV}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests assert directly on fixtures"
    )]

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
        // A filesystem path, not a URL: `%20` stays three literal bytes, so
        // `a%20b` is 5 bytes — never decoded to `a b`.
        let parsed = TransportAddr::parse("unix://a%20b").unwrap();
        assert_eq!(parsed, TransportAddr::Unix(PathBuf::from("a%20b")));
        let TransportAddr::Unix(path) = parsed else {
            panic!("expected a unix:// address");
        };
        assert_eq!(path.as_os_str().len(), 5, "`a%20b` is 5 literal bytes");
    }

    #[test]
    fn path_is_verbatim_including_surrounding_spaces() {
        // Only whitespace-*only* values are rejected; a path that happens to
        // contain spaces is legal and preserved byte-for-byte.
        assert_eq!(
            TransportAddr::parse("unix:// spaced path ").unwrap(),
            TransportAddr::Unix(PathBuf::from(" spaced path "))
        );
    }

    #[test]
    fn rejects_empty_unix_path() {
        assert!(TransportAddr::parse("unix://").is_err());
    }

    #[test]
    fn parses_tcp_scheme() {
        assert_eq!(
            TransportAddr::parse("tcp://127.0.0.1:9000").unwrap(),
            TransportAddr::Tcp("127.0.0.1:9000".parse().unwrap())
        );
        assert_eq!(
            TransportAddr::parse("tcp://[::1]:9000").unwrap(),
            TransportAddr::Tcp("[::1]:9000".parse().unwrap())
        );
    }

    #[test]
    fn rejects_tcp_without_port() {
        assert!(TransportAddr::parse("tcp://127.0.0.1").is_err());
        assert!(TransportAddr::parse("tcp://").is_err());
    }

    #[test]
    fn parses_cmd_scheme_whitespace_split() {
        // Whitespace-split argv, no shell quoting — runs of whitespace collapse.
        assert_eq!(
            TransportAddr::parse("cmd://ssh -T host  mxr daemon dial-stdio").unwrap(),
            TransportAddr::Cmd(vec![
                "ssh".into(),
                "-T".into(),
                "host".into(),
                "mxr".into(),
                "daemon".into(),
                "dial-stdio".into(),
            ])
        );
    }

    #[test]
    fn rejects_empty_cmd() {
        assert!(TransportAddr::parse("cmd://").is_err());
        assert!(TransportAddr::parse("cmd://   ").is_err());
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(TransportAddr::parse("ws://127.0.0.1:9000").is_err());
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
