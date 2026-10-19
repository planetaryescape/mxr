//! Isolated subprocess fixture. No environment changes, real credentials or
//! Microsoft calls; each test has its own executable and response files.

use super::MicrosoftTodoCli;
use mxr_todo::linked::LinkedTodoFields;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub(crate) struct FakeCli {
    directory: tempfile::TempDir,
}

impl FakeCli {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let script = format!(
            r#"#!/bin/sh
cd '{}'
printf '%s\000' "$@" >> args
shift 5
if [ -f verbose-stderr ]; then dd if=/dev/zero bs=65536 count=32 >&2 2>/dev/null || exit 1; fi
if [ "$1" = schema ]; then
  if [ -f fail-schema ]; then exit "$(cat fail-schema)"; fi
  if [ -f schema.json ]; then cat schema.json; else printf '{{"schema_version":2,"output":{{"required":["defer_until"]}}}}'; fi
  exit
fi
verb="$2"
printf '%s\n' "$verb" >> calls
if [ -f "fail-$verb" ]; then exit "$(cat "fail-$verb")"; fi
case "$verb" in
show)
  cat remote.json
  if [ -f fail-after-show ]; then mv fail-after-show fail-show; fi
  ;;
add|edit|complete|reopen)
  if [ "$verb" = add ]; then cat > stdin; fi
  if [ -f next.json ]; then cp next.json remote.json; fi
  printf '{{"schema_version":2,"op_id":"fake-operation","items":['
  cat remote.json
  printf ']}}'
  ;;
*) printf '{{"schema_version":2}}' ;;
esac
"#,
            directory.path().display()
        );
        let executable = directory.path().join("ms-todo");
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = Self { directory };
        result.set_remote(&result.fields());
        result
    }

    pub(crate) fn path(&self) -> &Path {
        self.directory.path()
    }
    pub(crate) fn cli(&self) -> MicrosoftTodoCli {
        MicrosoftTodoCli::at(self.path().join("ms-todo"))
    }
    pub(crate) fn fields(&self) -> LinkedTodoFields {
        LinkedTodoFields {
            title: "Pay council tax".into(),
            due: None,
            scheduled: None,
            completed: false,
        }
    }
    fn json(fields: &LinkedTodoFields) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2, "id": "remote-task", "title": fields.title,
            "status": if fields.completed { "completed" } else { "notStarted" },
            "sync_state": "synced", "defer_until": fields.scheduled,
            "dueDateTime": fields.due.map(|day| serde_json::json!({"dateTime": format!("{day}T00:00:00"), "timeZone":"UTC"})),
            "body": {"content": "Keep my Microsoft notes"},
            "importance": "high", "categories": ["Personal"]
        })
    }
    pub(crate) fn set_remote(&self, fields: &LinkedTodoFields) {
        self.set_remote_json(&Self::json(fields));
    }
    pub(crate) fn set_remote_json(&self, json: &serde_json::Value) {
        std::fs::write(
            self.path().join("remote.json"),
            serde_json::to_vec(json).unwrap(),
        )
        .unwrap();
    }
    pub(crate) fn set_next(&self, fields: &LinkedTodoFields) {
        std::fs::write(
            self.path().join("next.json"),
            serde_json::to_vec(&Self::json(fields)).unwrap(),
        )
        .unwrap();
    }
    pub(crate) fn fail(&self, verb: &str, code: i32) {
        std::fs::write(self.path().join(format!("fail-{verb}")), code.to_string()).unwrap();
    }
    pub(crate) fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
