//! The only way this tool touches mail: by spawning the `mxr` binary.
//!
//! No SQLite handle, no credentials, no provider client, no daemon socket.
//! Every draft goes through `mxr compose`, so mxr's HTML validation, pre-send
//! safety pipeline, and account/alias ownership checks all apply — a companion
//! cannot weaken them because it never gets to skip them.

use crate::manifest::SendReview;
use anyhow::{bail, Context};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeState {
    Accepted,
    NotAccepted,
    Unknown,
}
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Mxr {
    /// Binary to invoke. Overridable so tests can point at a stub.
    binary: String,
}

#[derive(Debug, Clone)]
pub struct DraftCreated {
    pub expected_workspace: crate::manifest::WorkspaceOwner,
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
            expected_workspace: serde_json::from_value(value.get("expected_workspace").cloned().ok_or_else(|| anyhow::anyhow!("Compose receipt lacks workspace ownership; upgrade mxr before creating retained work"))?)?,
            draft_id: draft_id.to_string(),
        })
    }

    pub fn preview(
        &self,
        account: &str,
        draft_id: &str,
        owner: &crate::manifest::WorkspaceOwner,
    ) -> anyhow::Result<SendReview> {
        let output = Command::new(&self.binary)
            .args([
                "send",
                draft_id,
                "--account",
                account,
                "--dry-run",
                "--expected-workspace",
                &serde_json::to_string(owner)?,
                "--format",
                "json",
            ])
            .output()
            .context("running reviewed preview")?;
        anyhow::ensure!(output.status.success(), "reviewed preview failed");
        #[derive(Deserialize)]
        struct Identity {
            operation_id: String,
            expected_revision: i64,
        }
        #[derive(Deserialize)]
        struct Draft {
            id: String,
            account_id: String,
            revision: i64,
        }
        #[derive(Deserialize)]
        struct Preview {
            expected_workspace: crate::manifest::WorkspaceOwner,
            draft: Draft,
            send_identity: Identity,
        }
        let preview: Preview =
            serde_json::from_slice(&output.stdout).context("invalid reviewed preview")?;
        anyhow::ensure!(
            preview.expected_workspace == *owner
                && preview.draft.id == draft_id
                && preview.draft.revision == preview.send_identity.expected_revision
                && preview.draft.revision > 0
                && !preview.draft.account_id.is_empty()
                && !preview.send_identity.operation_id.is_empty(),
            "reviewed preview conflicts with selected draft"
        );
        Ok(SendReview {
            expected_workspace: Some(preview.expected_workspace),
            account_id: preview.draft.account_id,
            expected_revision: preview.draft.revision,
            operation_id: preview.send_identity.operation_id,
            attempt: 0,
            scheduled_at: None,
        })
    }
    pub fn outcome(
        &self,
        account: &str,
        operation: &str,
        owner: &crate::manifest::WorkspaceOwner,
    ) -> anyhow::Result<Option<OutcomeState>> {
        let output = Command::new(&self.binary)
            .args([
                "send-outcome",
                operation,
                "--expected-workspace",
                &serde_json::to_string(owner)?,
                "--account",
                account,
                "--format",
                "json",
            ])
            .output()
            .context("running send outcome query")?;
        anyhow::ensure!(output.status.success(), "send outcome query failed");
        #[derive(Deserialize)]
        struct Outcome {
            state: OutcomeState,
        }
        Ok(serde_json::from_slice::<Option<Outcome>>(&output.stdout)
            .context("invalid send outcome response")?
            .map(|o| o.state))
    }
    /// Submit only the manifest's durably retained reviewed command.
    pub fn send_draft(&self, draft_id: &str, review: &SendReview) -> anyhow::Result<()> {
        let mut command = Command::new(&self.binary);
        let owner = review.expected_workspace.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Retained review has no original workspace; preserve it for explicit reopen/review"
            )
        })?;
        command.args([
            "send",
            draft_id,
            "--expected-workspace",
            &serde_json::to_string(owner)?,
            "--account",
            &review.account_id,
            "--expected-revision",
            &review.expected_revision.to_string(),
            "--operation-id",
            &review.operation_id,
        ]);
        if let Some(at) = &review.scheduled_at {
            command.args(["--at", at]);
        }
        let output = command
            .args(["--format", "json"])
            .output()
            .context("running reviewed send")?;
        anyhow::ensure!(
            output.status.success(),
            "reviewed send or schedule did not acknowledge success"
        );
        Ok(())
    }

    /// Ask mxr to parse a send time without mutating a draft, then return the
    /// absolute UTC instant it reports. Resolving once keeps a batch-wide
    /// relative value such as `in 2h` identical for every draft.
    pub fn resolve_send_time(
        &self,
        account: &str,
        recipient: &str,
        at: &str,
    ) -> anyhow::Result<String> {
        let output = Command::new(&self.binary)
            .arg("send-time")
            .arg(recipient)
            .arg("--account")
            .arg(account)
            .arg("--at")
            .arg(at)
            .arg("--format")
            .arg("json")
            .output()
            .with_context(|| format!("running `{} send-time`", self.binary))?;

        if !output.status.success() {
            bail!(
                "{}",
                first_lines(&String::from_utf8_lossy(&output.stderr), 6)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).with_context(|| {
            format!(
                "parsing `mxr send-time` JSON output: {}",
                first_lines(&stdout, 3)
            )
        })?;
        value
            .get("proposed_at")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("`mxr send-time` JSON had no proposed_at"))
    }

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
            assert!(!argv[0].contains(&"--yes".to_string()), "{:?}", argv[0]);
            assert!(!argv[0].contains(&"--send".to_string()), "{:?}", argv[0]);
            assert_eq!(stub.value_after(&argv[0], "--to"), Some("a@example.com"));
            assert_eq!(stub.value_after(&argv[0], "--subject"), Some("Your digest"));
            assert_eq!(
                stub.value_after(&argv[0], "--account"),
                Some("notto@example.com")
            );
        }

        #[test]
        fn send_hands_the_draft_id_back_to_mxr() {
            let stub = Stub::new();
            let mxr = Mxr::new(Some(stub.binary()));
            mxr.send_draft(
                "draft-7",
                &SendReview {
                    expected_workspace: Some(crate::manifest::WorkspaceOwner {
                        workspace_id: "workspace-a".into(),
                        recovery_generation: "generation-a".into(),
                    }),
                    account_id: "notto".into(),
                    expected_revision: 1,
                    operation_id: "draft-7:1".into(),
                    attempt: 0,
                    scheduled_at: None,
                },
            )
            .unwrap();

            let argv = stub.calls().remove(0);
            assert_eq!(argv[0], "send");
            assert_eq!(argv[1], "draft-7");
            assert!(!argv.contains(&"--at".to_string()));
        }
    }
}
