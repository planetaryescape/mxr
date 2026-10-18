//! The optional CLI is the only Microsoft To Do transport. Never invoke a
//! shell, parse task text, prompt for credentials, or retry an uncertain write.

use chrono::NaiveDate;
use mxr_todo::linked::LinkedTodoFields;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(all(test, unix))]
pub(super) mod test_support;

pub(super) const INSTALL_URL: &str = "https://github.com/planetaryescape/ms-todo#installation";
pub(super) const INSTALL_HINT: &str = "Install the optional ms-todo CLI on the machine running Mixer to manage linked tasks in Microsoft To Do.";

#[derive(Clone)]
pub(super) struct MicrosoftTodoCli {
    executable: PathBuf,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use test_support::FakeCli;

    #[tokio::test]
    async fn task_text_is_literal_and_context_only_goes_to_stdin() {
        let fake = FakeCli::new();
        let fields = LinkedTodoFields {
            title: "--due=tomorrow $(touch forbidden) every mon #Work".into(),
            due: None,
            scheduled: None,
            completed: false,
        };
        fake.set_remote(&fields);
        let cli = fake.cli();
        cli.add(
            &fields,
            "Untrusted context\nnot a shell command",
            "one-create",
        )
        .await
        .unwrap();
        let args = std::fs::read(fake.path().join("args")).unwrap();
        let args: Vec<_> = args
            .split(|b| *b == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8(arg.to_vec()).unwrap())
            .collect();
        assert_eq!(
            &args[..5],
            ["--context", "none", "--format", "json", "--quiet"]
        );
        assert!(args.iter().any(|arg| arg == "--no-parse"));
        assert_eq!(&args[args.len() - 2..], ["--", &fields.title]);
        assert!(!args.iter().any(|arg| arg.contains("Untrusted context")));
        assert_eq!(
            std::fs::read_to_string(fake.path().join("stdin")).unwrap(),
            "Untrusted context\nnot a shell command"
        );
    }

    #[tokio::test]
    async fn partial_success_cannot_be_classified_as_definitely_not_applied() {
        let fake = FakeCli::new();
        let mut fields = fake.fields();
        fields.title = "Changed title".into();
        fields.completed = true;
        let mut after_fields = fields.clone();
        after_fields.completed = false;
        fake.set_next(&after_fields);
        fake.fail("complete", 4);
        let error = fake
            .cli()
            .apply("remote-task", &fake.fields(), &fields)
            .await
            .unwrap_err();
        assert!(!error.definitely_not_applied());
        assert_eq!(fake.calls(), ["show", "edit", "show", "complete"]);
    }

    #[tokio::test]
    async fn patch_only_sends_changed_shared_fields() {
        let fake = FakeCli::new();
        let original = fake.fields();
        let mut changed = original.clone();
        changed.title = "Changed literally #Work every mon".into();
        fake.set_next(&changed);
        fake.cli()
            .apply("remote-task", &original, &changed)
            .await
            .unwrap();
        let args = std::fs::read(fake.path().join("args")).unwrap();
        let args = String::from_utf8(args).unwrap();
        assert!(args.contains("--title=Changed literally #Work every mon"));
        for untouched in [
            "--body",
            "--clear-reminder",
            "--recur",
            "--category",
            "--list",
            "--due",
            "--defer",
        ] {
            assert!(!args.contains(untouched), "unexpected patch: {untouched}");
        }
        assert_eq!(fake.calls(), ["show", "edit"]);
    }

    #[tokio::test]
    async fn changed_peer_snapshot_is_not_overwritten() {
        let fake = FakeCli::new();
        let original = fake.fields();
        let mut peer = original.clone();
        peer.title = "New phone edit".into();
        fake.set_remote(&peer);
        let mut desired = original.clone();
        desired.title = "Mixer edit".into();
        assert!(matches!(
            fake.cli().apply("remote-task", &original, &desired).await,
            Err(CliError::Changed)
        ));
        assert_eq!(fake.calls(), ["show"]);
    }

    #[tokio::test]
    async fn old_schema_and_oversized_responses_are_refused_before_a_write() {
        let fake = FakeCli::new();
        for task in [
            serde_json::json!({"schema_version": 1}),
            serde_json::json!({"schema_version": 2, "title": "x".repeat(1_048_577)}),
        ] {
            fake.set_remote_json(&task);
            assert!(matches!(
                fake.cli()
                    .apply("remote-task", &fake.fields(), &fake.fields())
                    .await,
                Err(CliError::BeforeWrite(error)) if matches!(*error, CliError::InvalidResponse)
            ));
        }
        assert_eq!(fake.calls(), ["show", "show"]);
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum CliError {
    #[error("{INSTALL_HINT}")]
    Missing,
    #[error("Sign in with `ms-todo auth login`, then try again.")]
    SignIn,
    #[error("ms-todo refused the request (exit {0}); no task was changed.")]
    Refused(i32),
    #[error("The Microsoft task changed while syncing. Refresh and review both versions; no write was started.")]
    Changed,
    #[error("The outcome is unconfirmed. Check `ms-todo outbox list --format json`; do not create the task again.")]
    Unknown,
    #[error("Couldn't read ms-todo's response. Update the CLI; task changes have been paused.")]
    InvalidResponse,
    #[error("No task write was started: {0}")]
    BeforeWrite(Box<CliError>),
}

impl CliError {
    pub(super) fn definitely_not_applied(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::SignIn | Self::Refused(_) | Self::Changed | Self::BeforeWrite(_)
        )
    }

    fn before_write(self) -> Self {
        if self.definitely_not_applied() {
            self
        } else {
            Self::BeforeWrite(Box::new(self))
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteTask {
    pub id: String,
    pub title: String,
    pub status: String,
    pub sync_state: String,
    #[serde(rename = "dueDateTime")]
    due: Option<RemoteDate>,
    // Missing is not the same as explicitly cleared: older schema-2 CLIs
    // predate deferral and must never erase a linked schedule.
    #[serde(deserialize_with = "Option::<NaiveDate>::deserialize")]
    pub defer_until: Option<NaiveDate>,
    pub recurrence: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RemoteDate {
    #[serde(rename = "dateTime")]
    date_time: String,
}

impl RemoteTask {
    pub(super) fn fields(&self) -> Result<LinkedTodoFields, CliError> {
        let completed = match self.status.as_str() {
            "completed" => true,
            "notStarted" | "inProgress" | "waitingOnOthers" | "deferred" => false,
            _ => return Err(CliError::InvalidResponse),
        };
        let due = self
            .due
            .as_ref()
            .map(|date| {
                date.date_time
                    .get(..10)
                    .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
                    .ok_or(CliError::InvalidResponse)
            })
            .transpose()?;
        Ok(LinkedTodoFields {
            title: self.title.clone(),
            due,
            scheduled: self.defer_until,
            completed,
        })
    }
}

#[derive(Deserialize)]
struct Applied {
    op_id: String,
    items: Vec<RemoteTask>,
}

impl MicrosoftTodoCli {
    pub(super) fn discover() -> Option<Self> {
        which::which("ms-todo")
            .ok()
            .map(|executable| Self { executable })
    }

    #[cfg(test)]
    pub(super) fn at(executable: PathBuf) -> Self {
        Self { executable }
    }

    async fn run(
        &self,
        args: &[String],
        input: Option<&str>,
    ) -> Result<serde_json::Value, CliError> {
        let mut command = tokio::process::Command::new(&self.executable);
        command
            .args(["--context", "none", "--format", "json", "--quiet"])
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| CliError::Missing)?;
        let stdout = child.stdout.take().ok_or(CliError::Unknown)?;
        let stderr = child.stderr.take().ok_or(CliError::Unknown)?;
        let stdin = child.stdin.take();
        let operation = async {
            let write = async {
                if let Some(input) = input {
                    let mut stdin = stdin.ok_or(CliError::Unknown)?;
                    stdin
                        .write_all(input.as_bytes())
                        .await
                        .map_err(|_| CliError::Unknown)?;
                    stdin.shutdown().await.map_err(|_| CliError::Unknown)?;
                }
                Ok(())
            };
            let read = async {
                let mut bytes = Vec::new();
                stdout
                    .take(1_048_577)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| CliError::Unknown)?;
                if bytes.len() > 1_048_576 {
                    return Err(CliError::InvalidResponse);
                }
                Ok(bytes)
            };
            let drain = async {
                tokio::io::copy(&mut stderr.take(65_536), &mut tokio::io::sink())
                    .await
                    .map_err(|_| CliError::Unknown)?;
                Ok(())
            };
            let wait = async { child.wait().await.map_err(|_| CliError::Unknown) };
            let (_, stdout, _, status) = tokio::try_join!(write, read, drain, wait)?;
            match status.code() {
                Some(0) => {}
                Some(4) => return Err(CliError::SignIn),
                Some(code @ (2 | 3 | 5 | 6 | 7)) => return Err(CliError::Refused(code)),
                _ => return Err(CliError::Unknown),
            }
            let value: serde_json::Value =
                serde_json::from_slice(&stdout).map_err(|_| CliError::InvalidResponse)?;
            if value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                != Some(2)
            {
                return Err(CliError::InvalidResponse);
            }
            Ok(value)
        };
        tokio::time::timeout(std::time::Duration::from_secs(30), operation)
            .await
            .map_err(|_| CliError::Unknown)?
    }

    pub(super) async fn refresh(&self) -> Result<(), CliError> {
        self.run(&["sync".into(), "--wait".into()], None).await?;
        Ok(())
    }

    /// Schema 2 predates deferral. Inspect the advertised task contract before
    /// creating anything, rather than discovering an old CLI after a write.
    pub(super) async fn check_compatible(&self) -> Result<(), CliError> {
        let schema = self
            .run(&["schema".into(), "tasks".into(), "show".into()], None)
            .await?;
        let supports_schedule = schema
            .pointer("/output/required")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|fields| {
                fields
                    .iter()
                    .any(|field| field.as_str() == Some("defer_until"))
            });
        if !supports_schedule {
            return Err(CliError::InvalidResponse);
        }
        Ok(())
    }

    pub(super) async fn show(&self, task_id: &str) -> Result<RemoteTask, CliError> {
        let value = self
            .run(
                &["tasks".into(), "show".into(), "--".into(), task_id.into()],
                None,
            )
            .await?;
        let task: RemoteTask =
            serde_json::from_value(value).map_err(|_| CliError::InvalidResponse)?;
        if task.id != task_id {
            return Err(CliError::InvalidResponse);
        }
        Ok(task)
    }

    pub(super) async fn add(
        &self,
        fields: &LinkedTodoFields,
        notes: &str,
        key: &str,
    ) -> Result<(String, RemoteTask), CliError> {
        let mut args = vec![
            "tasks".into(),
            "add".into(),
            "--no-parse".into(),
            "--body-file".into(),
            "-".into(),
            "--idempotency-key".into(),
            key.into(),
        ];
        if let Some(due) = fields.due {
            args.push(format!("--due={due}"));
        }
        if let Some(scheduled) = fields.scheduled {
            args.push(format!("--defer={scheduled}"));
        }
        args.extend(["--".into(), fields.title.clone()]);
        self.write(&args, Some(notes)).await
    }

    async fn write(
        &self,
        args: &[String],
        input: Option<&str>,
    ) -> Result<(String, RemoteTask), CliError> {
        let applied: Applied = serde_json::from_value(self.run(args, input).await?)
            .map_err(|_| CliError::InvalidResponse)?;
        if applied.items.len() != 1 || applied.op_id.is_empty() {
            return Err(CliError::InvalidResponse);
        }
        let task = applied
            .items
            .into_iter()
            .next()
            .ok_or(CliError::InvalidResponse)?;
        if task.id.is_empty() {
            return Err(CliError::InvalidResponse);
        }
        Ok((applied.op_id, task))
    }

    /// Patch only shared fields. Never replace notes, recurrence, reminders,
    /// steps, categories, importance, list membership or linked resources.
    pub(super) async fn apply(
        &self,
        task_id: &str,
        remote: &LinkedTodoFields,
        desired: &LinkedTodoFields,
    ) -> Result<(), CliError> {
        self.guard(task_id, remote)
            .await
            .map_err(CliError::before_write)?;
        let attempt = uuid::Uuid::now_v7().to_string();
        let changed_fields = remote.title != desired.title
            || remote.due != desired.due
            || remote.scheduled != desired.scheduled;
        if changed_fields {
            let mut args = vec![
                "tasks".into(),
                "edit".into(),
                "--idempotency-key".into(),
                format!("mxr:{attempt}:fields"),
            ];
            if remote.title != desired.title {
                args.push(format!("--title={}", desired.title));
            }
            if remote.due != desired.due {
                args.push(
                    desired
                        .due
                        .map_or_else(|| "--clear-due".into(), |day| format!("--due={day}")),
                );
            }
            if remote.scheduled != desired.scheduled {
                args.push(
                    desired
                        .scheduled
                        .map_or_else(|| "--clear-defer".into(), |day| format!("--defer={day}")),
                );
            }
            args.extend(["--".into(), task_id.into()]);
            let (_, task) = self.write(&args, None).await?;
            if task.id != task_id {
                return Err(CliError::InvalidResponse);
            }
            let mut expected = desired.clone();
            expected.completed = remote.completed;
            if task.recurrence.is_some() || task.fields()? != expected {
                return Err(CliError::Unknown);
            }
        }
        if remote.completed != desired.completed {
            let verb = if desired.completed {
                "complete"
            } else {
                "reopen"
            };
            let mut expected = desired.clone();
            expected.completed = remote.completed;
            self.guard(task_id, &expected).await.map_err(|error| {
                if changed_fields {
                    CliError::Unknown
                } else {
                    error.before_write()
                }
            })?;
            let (_, task) = self
                .write(
                    &[
                        "tasks".into(),
                        verb.into(),
                        "--idempotency-key".into(),
                        format!("mxr:{attempt}:state"),
                        "--".into(),
                        task_id.into(),
                    ],
                    None,
                )
                .await
                .map_err(|error| {
                    if changed_fields {
                        CliError::Unknown
                    } else {
                        error
                    }
                })?;
            if task.id != task_id {
                return Err(CliError::InvalidResponse);
            }
        }
        Ok(())
    }

    async fn guard(&self, task_id: &str, expected: &LinkedTodoFields) -> Result<(), CliError> {
        let current = self.show(task_id).await?;
        if current.recurrence.is_some()
            || !matches!(current.sync_state.as_str(), "synced" | "pending")
            || current.fields()? != *expected
        {
            return Err(CliError::Changed);
        }
        Ok(())
    }
}
