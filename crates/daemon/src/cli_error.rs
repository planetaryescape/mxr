//! Typed CLI failures with a stable exit code and kind.
//!
//! Only the paths that know a failure's class construct these. Everything else
//! stays a plain `anyhow` error and exits 1, and clap usage errors exit 2
//! before any of this runs.

use crate::cli::{Cli, OutputFormat};
use clap::CommandFactory;
use mxr_protocol::IpcErrorKind;

pub const EXIT_DAEMON_UNAVAILABLE: u8 = 3;
pub const EXIT_AUTH: u8 = 4;

const KIND_DAEMON_UNAVAILABLE: &str = "daemon_unavailable";
const KIND_DAEMON_INCOMPATIBLE: &str = "daemon_incompatible";
const KIND_DAEMON_AUTH: &str = "daemon_auth";

/// A failure whose exit code and machine-readable kind are part of the CLI
/// contract. Its `Display` is the human message, so stderr text is unchanged
/// by wrapping an existing error in one.
#[derive(Debug)]
pub struct CliError {
    kind: &'static str,
    exit_code: u8,
    message: String,
}

impl CliError {
    /// The daemon is not running or cannot be reached, even after an autostart attempt.
    pub fn daemon_unavailable(message: impl Into<String>) -> anyhow::Error {
        Self::new(KIND_DAEMON_UNAVAILABLE, EXIT_DAEMON_UNAVAILABLE, message).into()
    }

    /// The daemon answers but cannot be used by this mxr binary.
    pub fn daemon_incompatible(message: impl Into<String>) -> anyhow::Error {
        Self::new(KIND_DAEMON_INCOMPATIBLE, EXIT_DAEMON_UNAVAILABLE, message).into()
    }

    /// The daemon rejected the token this client presented.
    pub fn daemon_auth(message: impl Into<String>) -> anyhow::Error {
        Self::new(KIND_DAEMON_AUTH, EXIT_AUTH, message).into()
    }

    /// A daemon response whose kind is `IpcErrorKind::Auth`.
    pub fn auth(message: impl Into<String>) -> anyhow::Error {
        Self::new(IpcErrorKind::Auth.as_code(), EXIT_AUTH, message).into()
    }

    fn new(kind: &'static str, exit_code: u8, message: impl Into<String>) -> Self {
        Self {
            kind,
            exit_code,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }

    /// The typed failure at any level of an error chain, if there is one.
    pub fn find(error: &anyhow::Error) -> Option<&Self> {
        error
            .chain()
            .find_map(|cause| cause.downcast_ref::<Self>())
    }

    /// The one-line stderr record for `--format json` and `jsonl`.
    pub fn json_line(&self) -> String {
        serde_json::json!({
            "error": {
                "kind": self.kind,
                "message": self.message,
                "exit_code": self.exit_code,
            }
        })
        .to_string()
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Whether the command named by `args` writes JSON to stdout, so a failure
/// should also be reported as JSON on stderr.
///
/// Reads the `format` argument clap parsed for the deepest subcommand that
/// declares one, and falls back to `resolve_format` for the non-TTY default,
/// the same rule the command itself applies.
pub fn json_output_requested(args: &[String]) -> bool {
    let Ok(matches) = Cli::command().try_get_matches_from(args) else {
        return false;
    };
    let mut explicit: Option<Option<OutputFormat>> = None;
    let mut current = &matches;
    loop {
        if let Ok(value) = current.try_get_one::<OutputFormat>("format") {
            explicit = Some(value.cloned());
        }
        match current.subcommand() {
            Some((_, sub)) => current = sub,
            None => break,
        }
    }
    explicit.is_some_and(|format| {
        matches!(
            crate::output::resolve_format(format),
            OutputFormat::Json | OutputFormat::Jsonl
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_string()).collect()
    }

    #[test]
    fn typed_failures_carry_their_exit_code_and_kind() {
        let unavailable = CliError::daemon_unavailable("down");
        let cli = CliError::find(&unavailable).expect("typed error");
        assert_eq!((cli.kind(), cli.exit_code()), ("daemon_unavailable", 3));
        assert_eq!(unavailable.to_string(), "down");

        let auth = CliError::auth("Unauthorized");
        let cli = CliError::find(&auth).expect("typed error");
        assert_eq!((cli.kind(), cli.exit_code()), ("auth", 4));
    }

    #[test]
    fn typed_failure_is_found_through_context() {
        let error = CliError::daemon_unavailable("down").context("status");
        assert_eq!(CliError::find(&error).map(CliError::exit_code), Some(3));
    }

    #[test]
    fn untyped_failure_has_no_exit_override() {
        let error = anyhow::anyhow!("boom");
        assert!(CliError::find(&error).is_none());
    }

    #[test]
    fn json_line_has_kind_message_and_exit_code() {
        let error = CliError::daemon_unavailable("Cannot connect");
        let cli = CliError::find(&error).expect("typed error");
        let value: serde_json::Value = serde_json::from_str(&cli.json_line()).unwrap();
        assert_eq!(value["error"]["kind"], "daemon_unavailable");
        assert_eq!(value["error"]["message"], "Cannot connect");
        assert_eq!(value["error"]["exit_code"], 3);
    }

    #[test]
    fn explicit_json_format_is_detected_on_the_subcommand() {
        assert!(json_output_requested(&args(&[
            "mxr", "search", "x", "--format", "json"
        ])));
        assert!(json_output_requested(&args(&[
            "mxr",
            "activity",
            "saved",
            "list",
            "--format=jsonl"
        ])));
        assert!(!json_output_requested(&args(&[
            "mxr", "search", "x", "--format", "table"
        ])));
    }

    #[test]
    fn commands_without_a_format_flag_are_not_json() {
        assert!(!json_output_requested(&args(&[
            "mxr",
            "daemon",
            "--foreground"
        ])));
    }

    #[test]
    fn unparseable_arguments_are_not_json() {
        assert!(!json_output_requested(&args(&["mxr", "--no-such-flag"])));
    }
}
