use crate::action::Action;
use crate::app::Screen;

/// Debug-only action trace.
///
/// This is a debugging aid, not a re-runner; full session replay would require
/// deterministic IPC responses, which mxr does not currently provide.
#[cfg(debug_assertions)]
pub struct ActionRecorder {
    buffer: std::collections::VecDeque<RecordedAction>,
    capacity: usize,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
pub struct RecordedAction {
    pub timestamp: std::time::SystemTime,
    pub screen: String,
    pub action: String,
}

#[cfg(debug_assertions)]
impl ActionRecorder {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn record(&mut self, action: &Action, screen: &Screen) {
        if self.capacity == 0 {
            return;
        }
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(RecordedAction {
            timestamp: std::time::SystemTime::now(),
            screen: format!("{screen:?}"),
            action: truncate_action(format!("{action:?}")),
        });
    }

    pub fn snapshot(&self) -> Vec<RecordedAction> {
        self.buffer.iter().cloned().collect()
    }

    pub fn flush_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let snapshot = self.snapshot();
        for record in &snapshot {
            serde_json::to_writer(&mut file, &JsonRecordedAction::from(record))?;
            use std::io::Write as _;
            writeln!(file)?;
        }
        Ok(())
    }
}

#[cfg(debug_assertions)]
#[derive(serde::Serialize)]
struct JsonRecordedAction<'a> {
    timestamp: String,
    screen: &'a str,
    action: &'a str,
}

#[cfg(debug_assertions)]
impl<'a> From<&'a RecordedAction> for JsonRecordedAction<'a> {
    fn from(record: &'a RecordedAction) -> Self {
        let timestamp: chrono::DateTime<chrono::Utc> = record.timestamp.into();
        Self {
            timestamp: timestamp.to_rfc3339(),
            screen: &record.screen,
            action: &record.action,
        }
    }
}

#[cfg(debug_assertions)]
fn truncate_action(mut action: String) -> String {
    const MAX_ACTION_LEN: usize = 4096;
    if action.len() > MAX_ACTION_LEN {
        action.truncate(MAX_ACTION_LEN);
        action.push_str("...");
    }
    action
}

#[cfg(not(debug_assertions))]
pub struct ActionRecorder;

#[cfg(not(debug_assertions))]
impl ActionRecorder {
    pub fn new(_capacity: usize) -> Self {
        Self
    }

    pub fn record(&mut self, _action: &Action, _screen: &Screen) {}

    #[allow(dead_code)]
    pub fn flush_to(&self, _path: &std::path::Path) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[test]
    fn recorder_keeps_latest_actions_in_order() {
        let mut recorder = ActionRecorder::new(2);

        recorder.record(&Action::MoveDown, &Screen::Mailbox);
        recorder.record(&Action::MoveUp, &Screen::Search);
        recorder.record(&Action::Help, &Screen::Rules);

        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].action, "MoveUp");
        assert_eq!(snapshot[0].screen, "Search");
        assert_eq!(snapshot[1].action, "Help");
        assert_eq!(snapshot[1].screen, "Rules");
    }

    #[test]
    fn recorder_flushes_jsonl() {
        let mut recorder = ActionRecorder::new(2);
        recorder.record(&Action::MoveDown, &Screen::Mailbox);
        recorder.record(&Action::Help, &Screen::Diagnostics);
        let path = std::env::temp_dir().join(format!(
            "mxr-action-recorder-test-{}.jsonl",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        recorder.flush_to(&path).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["screen"], "Mailbox");
        assert_eq!(first["action"], "MoveDown");
        let _ = std::fs::remove_file(path);
    }
}
