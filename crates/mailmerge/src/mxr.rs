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
            command.arg("--inline").arg(format!("{cid}={}", path.display()));
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
            bail!("{}", first_lines(&String::from_utf8_lossy(&output.stderr), 6));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .with_context(|| format!("parsing `mxr compose` JSON output: {}", first_lines(&stdout, 3)))?;

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
            bail!("{}", first_lines(&String::from_utf8_lossy(&output.stderr), 6));
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

/// Trim provider/CLI output to something safe and short to log.
fn first_lines(text: &str, count: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "(no output)".to_string();
    }
    trimmed
        .lines()
        .take(count)
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_output_is_truncated_for_logging() {
        let long = (1..=20)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let summary = first_lines(&long, 3);
        assert_eq!(summary, "line 1; line 2; line 3");
    }

    #[test]
    fn empty_output_is_reported_rather_than_blank() {
        assert_eq!(first_lines("   ", 3), "(no output)");
    }

    #[test]
    fn a_missing_binary_explains_the_dependency() {
        let mxr = Mxr::new(Some("mxr-definitely-not-installed".into()));
        let err = mxr.preflight().unwrap_err();
        assert!(err.to_string().contains("drives the mxr CLI"), "{err}");
    }
}
