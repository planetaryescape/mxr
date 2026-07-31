//! The only way this tool touches mail: by spawning the `mxr` binary.
//!
//! No SQLite handle, no credentials, no provider client, no daemon socket.
//! Every draft goes through `mxr compose`, so mxr's HTML validation, pre-send
//! safety pipeline, and account/alias ownership checks all apply — a companion
//! cannot weaken them because it never gets to skip them.

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Mxr {
    /// Binary to invoke. Overridable so tests can point at a stub.
    binary: String,
}

/// What `mxr compose --draft --format json` reports back.
#[derive(Debug, Clone)]
pub struct DraftCreated {
    pub draft_id: String,
}

impl Mxr {
    pub fn new(binary: Option<String>) -> Self {
        Self {
            binary: binary.unwrap_or_else(|| "mxr".to_string()),
        }
    }

    /// Create one local draft. Never sends: `--draft` and `--yes` are mutually
    /// exclusive in mxr's own CLI, so this call cannot transmit.
    #[allow(clippy::too_many_arguments)]
    pub fn compose_draft(
        &self,
        account: &str,
        to: &str,
        subject: &str,
        html_path: &Path,
        text_path: Option<&Path>,
        inline: &[(String, PathBuf)],
        attachments: &[PathBuf],
    ) -> anyhow::Result<DraftCreated> {
        let mut command = Command::new(&self.binary);
        command
            .arg("compose")
            .arg("--account")
            .arg(account)
            .arg("--to")
            .arg(to)
            .arg("--subject")
            .arg(subject)
            .arg("--html-file")
            .arg(html_path)
            .arg("--draft")
            .arg("--format")
            .arg("json");

        if let Some(text) = text_path {
            command.arg("--text-file").arg(text);
        }
        for (cid, path) in inline {
            command
                .arg("--inline")
                .arg(format!("{cid}={}", path.display()));
        }
        for attachment in attachments {
            command.arg("--attach").arg(attachment);
        }

        let output = command
            .output()
            .with_context(|| format!("running `{} compose`", self.binary))?;

        if !output.status.success() {
            // stderr may quote the rendered HTML back; keep it short and let
            // the caller decide what to surface.
            bail!(
                "{}",
                first_lines(&String::from_utf8_lossy(&output.stderr), 6)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).with_context(|| {
            format!(
                "parsing `mxr compose` JSON output: {}",
                first_lines(&stdout, 3)
            )
        })?;

        let draft_id = value
            .get("draft_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("`mxr compose` JSON had no draft_id"))?;

        Ok(DraftCreated {
            draft_id: draft_id.to_string(),
        })
    }

    /// Send one already-created draft through mxr's normal send path.
    pub fn send_draft(&self, draft_id: &str) -> anyhow::Result<()> {
        let output = Command::new(&self.binary)
            .arg("send")
            .arg(draft_id)
            .arg("--format")
            .arg("json")
            .output()
            .with_context(|| format!("running `{} send`", self.binary))?;

        if !output.status.success() {
            bail!(
                "{}",
                first_lines(&String::from_utf8_lossy(&output.stderr), 6)
            );
        }
        Ok(())
    }

    /// Confirm the mxr binary is reachable before a run starts.
    pub fn preflight(&self) -> anyhow::Result<()> {
        let output = Command::new(&self.binary)
            .arg("--version")
            .output()
            .with_context(|| {
                format!(
                    "`{}` is not on PATH — mxr-mailmerge drives the mxr CLI and cannot run without it",
                    self.binary
                )
            })?;
        if !output.status.success() {
            bail!("`{} --version` failed", self.binary);
        }
        Ok(())
    }
}

/// Longest error summary worth keeping. Long enough to diagnose, short enough
/// that a subprocess cannot dump a whole rendered body into the manifest.
const MAX_SUMMARY_CHARS: usize = 400;

/// Trim provider/CLI output to something safe and short to log.
fn first_lines(text: &str, count: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "(no output)".to_string();
    }
    let summary = trimmed.lines().take(count).collect::<Vec<_>>().join("; ");
    match summary.char_indices().nth(MAX_SUMMARY_CHARS) {
        Some((cut, _)) => format!("{}…", &summary[..cut]),
        None => summary,
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::*;

    #[test]
    fn error_output_is_truncated_for_logging() {
        let long = (1..=20)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(first_lines(&long, 3), "line 1; line 2; line 3");
        assert_eq!(first_lines("only one", 3), "only one");
    }

    #[test]
    fn a_single_enormous_line_is_still_truncated() {
        // stderr may quote the rendered body back at us on one line; without a
        // character cap the whole personalised message lands in the manifest.
        let body = "x".repeat(50_000);
        let summary = first_lines(&body, 6);
        assert!(
            summary.chars().count() <= MAX_SUMMARY_CHARS + 1,
            "{}",
            summary.len()
        );
        assert!(summary.ends_with('…'), "{summary}");
    }

    #[test]
    fn truncation_does_not_split_a_multibyte_character() {
        let summary = first_lines(&"é".repeat(1_000), 6);
        assert!(summary.starts_with("éé"), "{summary}");
    }

    #[test]
    fn empty_output_is_reported_rather_than_blank() {
        assert_eq!(first_lines("   ", 3), "(no output)");
        assert_eq!(first_lines("", 3), "(no output)");
    }

    #[test]
    fn a_missing_binary_explains_the_dependency() {
        let mxr = Mxr::new(Some("mxr-definitely-not-installed".into()));
        let err = mxr.preflight().unwrap_err();
        assert!(err.to_string().contains("drives the mxr CLI"), "{err}");
    }

    #[test]
    fn the_default_binary_is_the_mxr_on_path() {
        assert_eq!(Mxr::new(None).binary, "mxr");
        assert_eq!(Mxr::new(Some("/opt/mxr".into())).binary, "/opt/mxr");
    }

    #[cfg(unix)]
    mod against_a_stub_binary {
        use super::*;
        use crate::stub::Stub;

        #[test]
        fn compose_asks_for_a_draft_and_never_for_a_send() {
            let stub = Stub::new();
            let mxr = Mxr::new(Some(stub.binary()));
            let draft = mxr
                .compose_draft(
                    "notto@example.com",
                    "a@example.com",
                    "Your digest",
                    &stub.dir().join("body.html"),
                    None,
                    &[],
                    &[],
                )
                .unwrap();

            assert_eq!(draft.draft_id, "draft-1");
            let argv = stub.calls();
            assert_eq!(argv.len(), 1);
            assert_eq!(argv[0][0], "compose");
            assert!(argv[0].contains(&"--draft".to_string()), "{:?}", argv[0]);
            assert!(argv[0].contains(&"--format".to_string()));
            // The contract that keeps drafting and sending apart.
            assert!(!argv[0].contains(&"--yes".to_string()), "{:?}", argv[0]);
            assert!(!argv[0].contains(&"--send".to_string()), "{:?}", argv[0]);
            // Address, subject, and account reach mxr as single arguments.
            assert_eq!(stub.value_after(&argv[0], "--to"), Some("a@example.com"));
            assert_eq!(stub.value_after(&argv[0], "--subject"), Some("Your digest"));
            assert_eq!(
                stub.value_after(&argv[0], "--account"),
                Some("notto@example.com")
            );
        }

        #[test]
        fn optional_parts_are_passed_through_only_when_present() {
            let stub = Stub::new();
            let mxr = Mxr::new(Some(stub.binary()));
            mxr.compose_draft(
                "notto",
                "a@example.com",
                "Subject",
                &stub.dir().join("body.html"),
                Some(&stub.dir().join("body.txt")),
                &[("logo".to_string(), "/tmp/logo.png".into())],
                &[std::path::PathBuf::from("/tmp/brief.pdf")],
            )
            .unwrap();

            let argv = stub.calls().remove(0);
            assert_eq!(
                stub.value_after(&argv, "--inline"),
                Some("logo=/tmp/logo.png")
            );
            assert_eq!(stub.value_after(&argv, "--attach"), Some("/tmp/brief.pdf"));
            assert!(argv.contains(&"--text-file".to_string()));
        }

        #[test]
        fn a_failing_compose_surfaces_a_short_error_and_no_draft_id() {
            let stub = Stub::failing(
                &(1..=20)
                    .map(|n| format!("boom {n}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            let mxr = Mxr::new(Some(stub.binary()));
            let err = mxr
                .compose_draft(
                    "notto",
                    "a@example.com",
                    "Subject",
                    &stub.dir().join("body.html"),
                    None,
                    &[],
                    &[],
                )
                .unwrap_err()
                .to_string();
            assert!(err.starts_with("boom 1; boom 2"), "{err}");
            assert!(!err.contains("boom 7"), "{err}");
        }

        #[test]
        fn compose_output_that_is_not_a_draft_is_rejected() {
            for stdout in ["{}", "not json at all", "", r#"{"draft_id":42}"#] {
                let stub = Stub::replying(stdout);
                let mxr = Mxr::new(Some(stub.binary()));
                assert!(
                    mxr.compose_draft(
                        "notto",
                        "a@example.com",
                        "Subject",
                        &stub.dir().join("body.html"),
                        None,
                        &[],
                        &[],
                    )
                    .is_err(),
                    "accepted `{stdout}` as a created draft"
                );
            }
        }

        #[test]
        fn send_hands_the_draft_id_back_to_mxr() {
            let stub = Stub::new();
            let mxr = Mxr::new(Some(stub.binary()));
            mxr.send_draft("draft-7").unwrap();

            let argv = stub.calls().remove(0);
            assert_eq!(argv[0], "send");
            assert_eq!(argv[1], "draft-7");
        }

        #[test]
        fn a_failing_send_is_an_error() {
            let stub = Stub::failing("smtp said no");
            let err = Mxr::new(Some(stub.binary()))
                .send_draft("draft-7")
                .unwrap_err();
            assert!(err.to_string().contains("smtp said no"), "{err}");
        }

        #[test]
        fn preflight_accepts_a_binary_that_answers_version() {
            assert!(Mxr::new(Some(Stub::new().binary())).preflight().is_ok());
        }

        #[test]
        fn preflight_rejects_a_binary_that_fails() {
            let stub = Stub::failing("broken install");
            let err = Mxr::new(Some(stub.binary())).preflight().unwrap_err();
            assert!(err.to_string().contains("--version` failed"), "{err}");
        }
    }
}
