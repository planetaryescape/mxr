use mxr_core::{AccountId, DraftId, SendIdentity};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Write, path::Path, sync::Mutex};

static WRITER: Mutex<()> = Mutex::new(());
#[derive(Default, Serialize, Deserialize)]
struct Recovery {
    last: Option<(AccountId, SendIdentity)>,
    operations: HashMap<DraftId, (AccountId, SendIdentity)>,
}
fn path() -> std::path::PathBuf {
    mxr_config::config_dir().join("tui-send-recovery.json")
}
fn load_at(path: &Path) -> std::io::Result<Recovery> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Recovery::default()),
        Err(error) => Err(error),
    }
}
pub(crate) fn last() -> std::io::Result<Option<(AccountId, SendIdentity)>> {
    Ok(load_at(&path())?.last)
}
pub(crate) fn for_draft(draft: &DraftId) -> std::io::Result<Option<(AccountId, SendIdentity)>> {
    Ok(load_at(&path())?.operations.get(draft).cloned())
}
#[cfg(not(test))]
pub(crate) fn retain(
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
) -> std::io::Result<()> {
    retain_at(&path(), account, draft, identity)
}
fn retain_at(
    path: &Path,
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
) -> std::io::Result<()> {
    let _guard = WRITER
        .lock()
        .map_err(|_| std::io::Error::other("send recovery writer poisoned"))?;
    let mut recovery = load_at(path)?;
    recovery.last = Some((account.clone(), identity.clone()));
    recovery
        .operations
        .insert(draft.clone(), (account.clone(), identity.clone()));
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing recovery directory"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".tui-send-recovery-{}.tmp", DraftId::new()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(&recovery).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn send_recovery_survives_delayed_stale_ui_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tui-send-recovery.json");
        let ui_path = dir.path().join("tui-state.json");
        let stale_ui = crate::local_state::TuiLocalState {
            recent_action_labels: vec!["Send".into()],
        };
        let account = AccountId::new();
        let draft = DraftId::new();
        let mut identity = SendIdentity::for_revision(&draft, 1);
        retain_at(&path, &account, &draft, &identity).unwrap();
        identity.operation_id.push_str(":retry:latest");
        identity.expected_revision = 2;
        retain_at(&path, &account, &draft, &identity).unwrap();
        crate::local_state::save_to_path(&ui_path, &stale_ui).unwrap();
        let reloaded = load_at(&path).unwrap();
        assert_eq!(reloaded.last, Some((account, identity)));
        assert_eq!(reloaded.operations[&draft].1.expected_revision, 2);
        std::fs::write(&path, "malformed").unwrap();
        assert!(retain_at(
            &path,
            &AccountId::new(),
            &draft,
            &SendIdentity::for_revision(&draft, 1)
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "malformed");
    }
}
