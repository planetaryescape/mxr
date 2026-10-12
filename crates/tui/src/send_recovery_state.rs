use mxr_core::{AccountId, DraftId, SendIdentity};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Write, path::Path, sync::Mutex};

static WRITER: Mutex<()> = Mutex::new(());
#[derive(Default, Serialize, Deserialize)]
struct Recovery {
    #[serde(default)]
    owners: HashMap<DraftId, mxr_protocol::ArchiveWorkspaceData>,
    #[serde(default)]
    compose: Option<(AccountId, std::path::PathBuf)>,
    #[serde(default)]
    compose_files: Vec<(AccountId, std::path::PathBuf)>,
    #[serde(default)]
    query: Option<String>,
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
pub(crate) fn last_query() -> std::io::Result<Option<String>> {
    Ok(load_at(&path())?.query)
}
#[cfg(not(test))]
pub(crate) fn retain_schedule_query(
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
) -> std::io::Result<()> {
    retain_query_at(&path(), account, draft, identity)
}
fn retain_query_at(
    path: &Path,
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
) -> std::io::Result<()> {
    let _guard = WRITER
        .lock()
        .map_err(|_| std::io::Error::other("send recovery lock poisoned"))?;
    let mut recovery = load_at(path)?;
    if recovery.last.as_ref() == Some(&(account.clone(), identity.clone()))
        && recovery.operations.get(draft) == recovery.last.as_ref()
    {
        recovery.query = Some(format!("draft:{draft}"));
        write_recovery(path, &recovery)?;
    }
    Ok(())
}
pub(crate) fn last_compose_excluding(
    owned: &[std::path::PathBuf],
) -> std::io::Result<Option<(AccountId, std::path::PathBuf)>> {
    let recovery = load_at(&path())?;
    Ok(recovery
        .compose_files
        .into_iter()
        .rev()
        .chain(recovery.compose)
        .find(|(_, file)| file.is_file() && !owned.contains(file)))
}
#[cfg(test)]
fn reopenable_compose(recovery: Recovery) -> Option<(AccountId, std::path::PathBuf)> {
    recovery
        .compose_files
        .into_iter()
        .rev()
        .find(|(_, path)| path.is_file())
        .or_else(|| recovery.compose.filter(|(_, path)| path.is_file()))
}
pub(crate) fn for_draft(draft: &DraftId) -> std::io::Result<Option<(AccountId, SendIdentity)>> {
    Ok(load_at(&path())?.operations.get(draft).cloned())
}
#[cfg(not(test))]
pub(crate) fn retain_reviewed(
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
) -> std::io::Result<()> {
    retain_at_owned(&path(), account, draft, identity, None, false)
}
#[cfg(not(test))]
pub(crate) fn retain_compose(
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
    file: &Path,
) -> std::io::Result<()> {
    retain_at(&path(), account, draft, identity, Some(file))
}
#[cfg(not(test))]
pub(crate) fn retain_reviewed_compose(
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
    file: &Path,
) -> std::io::Result<()> {
    retain_at_owned(&path(), account, draft, identity, Some(file), false)
}
fn retain_at(
    path: &Path,
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
    compose: Option<&Path>,
) -> std::io::Result<()> {
    retain_at_owned(path, account, draft, identity, compose, true)
}
fn retain_at_owned(
    path: &Path,
    account: &AccountId,
    draft: &DraftId,
    identity: &SendIdentity,
    compose: Option<&Path>,
    promote: bool,
) -> std::io::Result<()> {
    let _guard = WRITER
        .lock()
        .map_err(|_| std::io::Error::other("send recovery writer poisoned"))?;
    let mut recovery = load_at(path)?;
    if let Some(file) = compose {
        if promote
            || recovery.compose.is_none()
            || recovery
                .compose
                .as_ref()
                .is_some_and(|(_, retained)| retained == file)
        {
            recovery.compose = Some((account.clone(), file.to_path_buf()));
        }
        if promote
            || !recovery
                .compose_files
                .iter()
                .any(|(_, retained)| retained == file)
        {
            recovery
                .compose_files
                .retain(|(_, retained)| retained != file);
            recovery
                .compose_files
                .push((account.clone(), file.to_path_buf()));
        }
    }
    let previous = recovery.operations.get(draft);
    if !promote
        && previous
            .is_some_and(|(_, retained)| retained.expected_revision > identity.expected_revision)
    {
        return Err(std::io::Error::other(
            "A newer reviewed revision is already retained",
        ));
    }
    if promote || recovery.last.is_none() || recovery.last.as_ref() == previous {
        recovery.last = Some((account.clone(), identity.clone()));
        recovery.query = None;
    }
    recovery
        .operations
        .insert(draft.clone(), (account.clone(), identity.clone()));
    write_recovery(path, &recovery)
}
fn write_recovery(path: &Path, recovery: &Recovery) -> std::io::Result<()> {
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
        retain_at(
            &path,
            &account,
            &draft,
            &identity,
            Some(Path::new("/tmp/reviewed-compose.md")),
        )
        .unwrap();
        identity.operation_id.push_str(":retry:latest");
        identity.expected_revision = 2;
        retain_at(
            &path,
            &account,
            &draft,
            &identity,
            Some(Path::new("/tmp/reviewed-compose.md")),
        )
        .unwrap();
        crate::local_state::save_to_path(&ui_path, &stale_ui).unwrap();
        let reloaded = load_at(&path).unwrap();
        assert_eq!(reloaded.last, Some((account, identity)));
        assert_eq!(
            reloaded.compose,
            Some((
                reloaded.last.as_ref().unwrap().0.clone(),
                Path::new("/tmp/reviewed-compose.md").to_path_buf()
            ))
        );
        assert_eq!(reloaded.operations[&draft].1.expected_revision, 2);
        std::fs::write(&path, "malformed").unwrap();
        assert!(retain_at(
            &path,
            &AccountId::new(),
            &draft,
            &SendIdentity::for_revision(&draft, 1),
            None,
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "malformed");
    }
    #[test]
    fn failed_send_file_remains_reopenable_after_a_later_success_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("recovery.json");
        let account = AccountId::new();
        let a = directory.path().join("A.md");
        let b = directory.path().join("B.md");
        for file in [&a, &b] {
            std::fs::write(file, "retained text").unwrap();
            let id = DraftId::from_provider_id("compose-file", &file.to_string_lossy());
            retain_at(
                &state,
                &account,
                &id,
                &SendIdentity::for_revision(&id, 1),
                Some(file),
            )
            .unwrap();
        }
        std::fs::remove_file(&b).unwrap();
        assert_eq!(
            reopenable_compose(load_at(&state).unwrap()),
            Some((account, a))
        );
    }
}

#[cfg(test)]
mod revision_owner_tests {
    use super::*;
    #[test]
    fn late_compose_revision_keeps_newer_compose_identity_and_reopen_selection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recovery.json");
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        std::fs::write(&a, "A").unwrap();
        std::fs::write(&b, "B").unwrap();
        let account = AccountId::new();
        let draft_a = DraftId::new();
        let draft_b = DraftId::new();
        retain_at(
            &path,
            &account,
            &draft_a,
            &SendIdentity::for_revision(&draft_a, 1),
            Some(&a),
        )
        .unwrap();
        let b_identity = SendIdentity::for_revision(&draft_b, 1);
        retain_at(&path, &account, &draft_b, &b_identity, Some(&b)).unwrap();
        retain_at_owned(
            &path,
            &account,
            &draft_a,
            &SendIdentity::for_revision(&draft_a, 2),
            Some(&a),
            false,
        )
        .unwrap();
        let recovery = load_at(&path).unwrap();
        assert_eq!(recovery.last, Some((account.clone(), b_identity)));
        assert_eq!(recovery.operations[&draft_a].1.expected_revision, 2);
        assert_eq!(reopenable_compose(recovery), Some((account, b)));
    }
    #[test]
    fn scheduled_alias_survives_reload_and_late_send_cannot_replace_newer_selection() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("recovery.json");
        let account = AccountId::new();
        let draft_a = DraftId::new();
        let draft_b = DraftId::new();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        std::fs::write(&a, "A").unwrap();
        std::fs::write(&b, "B").unwrap();
        retain_at(
            &state,
            &account,
            &draft_a,
            &SendIdentity::for_revision(&draft_a, 1),
            Some(&a),
        )
        .unwrap();
        let identity = SendIdentity::for_revision(&draft_b, 2);
        retain_at(&state, &account, &draft_b, &identity, Some(&b)).unwrap();
        retain_query_at(&state, &account, &draft_b, &identity).unwrap();
        retain_at_owned(
            &state,
            &account,
            &draft_a,
            &SendIdentity::for_revision(&draft_a, 2),
            None,
            false,
        )
        .unwrap();
        retain_query_at(
            &state,
            &account,
            &draft_a,
            &SendIdentity::for_revision(&draft_a, 2),
        )
        .unwrap();
        let reloaded = load_at(&state).unwrap();
        assert_eq!(reloaded.query, Some(format!("draft:{draft_b}")));
        assert_eq!(reloaded.last, Some((account.clone(), identity)));
        assert_eq!(reopenable_compose(reloaded), Some((account, b)));
    }
}

#[cfg(not(test))]
pub(crate) fn retain_owner(
    draft: &DraftId,
    owner: &mxr_protocol::ArchiveWorkspaceData,
) -> std::io::Result<()> {
    let _lock = WRITER
        .lock()
        .map_err(|_| std::io::Error::other("send recovery lock poisoned"))?;
    let mut recovery = load_at(&path())?;
    if recovery
        .owners
        .get(draft)
        .is_some_and(|original| original != owner)
    {
        return Err(std::io::Error::other(
            "This private draft belongs to another workspace; return there to recover",
        ));
    }
    recovery.owners.insert(draft.clone(), owner.clone());
    write_recovery(&path(), &recovery)
}

pub(crate) fn owner_for_request(
    request: &mxr_protocol::Request,
) -> std::io::Result<Option<mxr_protocol::ArchiveWorkspaceData>> {
    use mxr_protocol::Request;
    let draft = match request {
        Request::GetDraft { draft_id }
        | Request::DeleteDraft { draft_id, .. }
        | Request::SendStoredDraft { draft_id, .. }
        | Request::ScheduleSend { draft_id, .. }
        | Request::CancelScheduledSend { draft_id } => Some(draft_id.clone()),
        Request::SaveDraft { draft }
        | Request::UpdateDraft { draft }
        | Request::SaveDraftToServer { draft }
        | Request::SuggestCollaborators { draft, .. }
        | Request::ExtractDraftCommitments { draft }
        | Request::SendDraft { draft, .. }
        | Request::CheckDraftSafety { draft, .. } => Some(draft.id.clone()),
        Request::GetSendOutcome { operation_id, .. } => operation_id
            .strip_prefix("draft:")
            .unwrap_or(operation_id)
            .split(':')
            .next()
            .and_then(|id| id.parse().ok()),
        _ => None,
    };
    let Some(draft) = draft else {
        return Ok(None);
    };
    let recovery = load_at(&path())?;
    if let Some(owner) = recovery.owners.get(&draft) {
        return Ok(Some(owner.clone()));
    }
    if recovery.operations.contains_key(&draft) {
        return Err(std::io::Error::other("Retained draft has no workspace owner; its private file remains preserved. Explicitly reopen and review the saved draft."));
    }
    Ok(None)
}
