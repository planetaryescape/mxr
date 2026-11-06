//! Read-only client policy for an operator's persistent hosted-daemon stop.
use serde::Deserialize;
use std::{io, path::Path};

#[derive(Deserialize)]
struct DesiredState {
    version: u32,
    daemon_enabled: bool,
}

/// A missing hosting setup permits normal local autostart. An existing setup
/// must explicitly enable its daemon; malformed state is an error, never permission.
pub fn hosting_daemon_desired_stopped() -> io::Result<bool> {
    read_desired_stopped(&crate::config_dir().join("hosting.json"))
}
fn read_desired_stopped(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hosting desired state must be a regular file",
        ));
    }
    let state: DesiredState = serde_json::from_reader(file).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid hosting desired state; inspect hosting.json before starting a daemon",
        )
    })?;
    if state.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unsupported hosting desired-state version",
        ));
    }
    Ok(!state.daemon_enabled)
}

/// Serialize the brief transition from an initialized daemon to admitting
/// provider work with writes to its desired state. Release before serving.
pub fn lock_hosting_daemon_startup() -> io::Result<std::fs::File> {
    lock_startup_at(&crate::config_dir().join("hosting-startup.lock"))
}
fn lock_startup_at(path: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hosting startup gate must be a regular file",
        ));
    }
    file.lock()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "owned state fixtures assert setup and results"
    )]
    use super::*;
    #[test]
    fn missing_setup_allows_autostart_and_explicit_stopped_setup_does_not() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosting.json");
        assert!(!read_desired_stopped(&path).unwrap());
        assert!(!path.exists());
        std::fs::write(&path,r#"{"version":1,"daemon_enabled":false,"public_origin":"https://fixture.ts.net","enabled":false}"#).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        let bytes = std::fs::read(&path).unwrap();
        assert!(read_desired_stopped(&path).unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode(), mode);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::write(&path, r#"{"version":1,"daemon_enabled":true}"#).unwrap();
        assert!(!read_desired_stopped(&path).unwrap());
    }
    #[test]
    fn unknown_versions_missing_fields_and_invalid_values_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosting.json");
        for invalid in [
            "invalid",
            r#"{"version":2,"daemon_enabled":true}"#,
            r#"{"version":1}"#,
            r#"{"daemon_enabled":true}"#,
            r#"{"version":1,"daemon_enabled":"true"}"#,
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(read_desired_stopped(&path).is_err(), "{invalid}");
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(read_desired_stopped(&path).is_err());
    }
    #[test]
    fn stop_commit_wins_before_a_waiting_daemon_can_cross_its_start_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosting.json");
        let lock = dir.path().join("hosting-startup.lock");
        std::fs::write(&path, r#"{"version":1,"daemon_enabled":true}"#).unwrap();
        let operator = lock_startup_at(&lock).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let daemon = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let _startup = lock_startup_at(&lock).unwrap();
            result_tx
                .send(read_desired_stopped(&path).unwrap())
                .unwrap();
        });
        ready_rx.recv().unwrap();
        assert!(result_rx.try_recv().is_err());
        std::fs::write(
            dir.path().join("hosting.json"),
            r#"{"version":1,"daemon_enabled":false}"#,
        )
        .unwrap();
        drop(operator);
        assert!(result_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap());
        daemon.join().unwrap();
    }
    #[test]
    fn a_symlink_cannot_replace_the_operator_state() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.json");
        let path = dir.path().join("hosting.json");
        std::fs::write(&target, r#"{"version":1,"daemon_enabled":true}"#).unwrap();
        std::os::unix::fs::symlink(target, path.clone()).unwrap();
        assert!(read_desired_stopped(&path).is_err());
    }
}
