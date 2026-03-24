use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuiLocalState {
    #[serde(default)]
    pub onboarding_seen: bool,
}

pub fn load() -> TuiLocalState {
    let path = file_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str::<TuiLocalState>(&content).ok())
        .unwrap_or_default()
}

pub fn save(state: &TuiLocalState) -> std::io::Result<()> {
    let path = file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".into());
    std::fs::write(path, content)
}

fn file_path() -> PathBuf {
    crate::mxr_config::config_dir().join("tui-state.json")
}
