use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuiLocalState {
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

pub(crate) fn save_to_path(path: &std::path::Path, state: &TuiLocalState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".into());
    std::fs::write(path, content)
}

fn file_path() -> PathBuf {
    mxr_config::config_dir().join("tui-state.json")
}
