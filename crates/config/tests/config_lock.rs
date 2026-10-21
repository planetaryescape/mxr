#![expect(
    clippy::unwrap_used,
    reason = "test fixtures unwrap temp-dir and file operations and fail loudly"
)]

//! Read-modify-write on the config file from several writers at once.

use mxr_config::{lock_config_file_at, update_config_at_path, ConfigError, MxrConfig};

const WRITERS: u64 = 8;
const EACH: u64 = 25;

#[derive(Debug, PartialEq)]
enum Failure {
    Config,
    Refused,
}

impl From<ConfigError> for Failure {
    fn from(_: ConfigError) -> Self {
        Self::Config
    }
}

#[test]
fn concurrent_updates_never_lose_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let start = MxrConfig::default().general.sync_interval;
    mxr_config::save_config_to_path(&MxrConfig::default(), &path).unwrap();

    let writers: Vec<_> = (0..WRITERS)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                for _ in 0..EACH {
                    update_config_at_path(&path, |config| {
                        let before = config.general.sync_interval;
                        // Widen the window between the read and the write.
                        std::thread::yield_now();
                        config.general.sync_interval = before + 1;
                        Ok::<_, ConfigError>(())
                    })
                    .unwrap();
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }

    let saved = mxr_config::load_config_from_path(&path).unwrap();
    assert_eq!(saved.general.sync_interval, start + WRITERS * EACH);
}

#[test]
fn a_failed_update_saves_nothing_and_releases_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    mxr_config::save_config_to_path(&MxrConfig::default(), &path).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    let failed: Result<(), Failure> = update_config_at_path(&path, |config| {
        config.general.sync_interval += 1;
        Err(Failure::Refused)
    });
    assert_eq!(failed, Err(Failure::Refused));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    // The lock is free again.
    drop(lock_config_file_at(&path).unwrap());
}

#[test]
fn the_lock_lives_beside_the_config_not_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let lock = lock_config_file_at(&path).unwrap();
    assert!(dir.path().join("config.toml.lock").exists());
    assert!(!path.exists());
    drop(lock);
}
