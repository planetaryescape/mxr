#![expect(
    clippy::unwrap_used,
    reason = "test fixtures unwrap temp-dir and file operations and fail loudly"
)]

use mxr_config::{update_config_at_path, ConfigError, MxrConfig};

const WRITERS: u64 = 8;
const EACH: u64 = 25;

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
