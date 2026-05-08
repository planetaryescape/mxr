use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuiLocalState {
    // TUI-owned view state belongs here, not in daemon IPC.
    #[serde(default)]
    pub onboarding_seen: bool,
    /// Most-recent-first labels of palette commands the user has
    /// confirmed. Persisted by label rather than by `Action` enum because
    /// the enum's variants change shape across versions; labels are
    /// stable user-facing strings.
    #[serde(default)]
    pub recent_action_labels: Vec<String>,
}

pub fn load() -> TuiLocalState {
    let path = file_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str::<TuiLocalState>(&content).ok())
        .unwrap_or_default()
}

pub fn save(state: &TuiLocalState) -> std::io::Result<()> {
    save_to_path(&file_path(), state)
}

pub async fn save_async(state: TuiLocalState) -> std::io::Result<()> {
    let path = file_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".into());
    tokio::fs::write(path, content).await
}

fn save_to_path(path: &std::path::Path, state: &TuiLocalState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".into());
    std::fs::write(path, content)
}

fn file_path() -> PathBuf {
    mxr_config::config_dir().join("tui-state.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_action_labels_round_trip_through_disk_in_order() {
        let dir = std::env::temp_dir().join(format!(
            "mxr-local-state-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("tui-state.json");

        let state = TuiLocalState {
            onboarding_seen: true,
            recent_action_labels: vec![
                "Archive".to_string(),
                "Reply All".to_string(),
                "Star".to_string(),
            ],
        };
        save_to_path(&path, &state).expect("save state");

        let raw = std::fs::read_to_string(&path).expect("read saved state");
        let loaded: TuiLocalState = serde_json::from_str(&raw).expect("parse state");

        assert_eq!(loaded.onboarding_seen, true);
        assert_eq!(
            loaded.recent_action_labels,
            vec![
                "Archive".to_string(),
                "Reply All".to_string(),
                "Star".to_string(),
            ],
            "recent action labels round-trip preserves order"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_recent_action_labels_field_defaults_to_empty() {
        // Backwards-compat: an older state file written before this field
        // existed must still load cleanly with an empty recents list.
        let raw = r#"{ "onboarding_seen": true }"#;
        let loaded: TuiLocalState = serde_json::from_str(raw).expect("parse legacy state");
        assert_eq!(loaded.onboarding_seen, true);
        assert!(loaded.recent_action_labels.is_empty());
    }
}
