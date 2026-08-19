//! End-to-end regression test for `mxr demo` — issue #179.
//!
//! `mxr demo` is the README's front door, and it used to die with
//! `IPC request timed out after 120 seconds` because every long call it makes
//! (seed sync, analytics rebuild, semantic reindex) went over the CLI's
//! wall-clock-capped request path while the daemon was still working. The
//! failure only showed up on a machine slower — or a mailbox larger — than the
//! one the deadlines were tuned on.
//!
//! This test runs the real binary end to end and holds the fix in place two
//! ways: it asserts the seed sync streamed the daemon's operation events (only
//! the no-deadline request path receives them, so this catches a regression
//! however fast the machine is), and it runs the whole journey under a
//! deliberately small `MXR_IPC_TIMEOUT_SECS`.

#![expect(
    clippy::panic,
    reason = "integration tests panic with command output when daemon-backed journeys fail"
)]

use mxr_test_support::daemon::{
    daemon_lock, link_mxr_binary, run_json_with_env, run_with_env, run_with_env_until_line,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// Enough messages to span several sync batches and give the analytics and
/// Wrapped prewarms real work, while still finishing quickly on a 4-vCPU CI
/// runner.
const DEMO_MESSAGES: usize = 2_000;

/// Half the 120s default, so a long call that regresses back onto the capped
/// path trips here rather than shipping. Not tighter than that: the short
/// calls this still bounds (status polls, the surface-seeding mutations) can
/// take seconds on a loaded CI runner, and the streamed-events assertion below
/// is the guard that does not depend on timing at all.
const IPC_TIMEOUT_SECS: &str = "60";

/// Marker `mxr demo` prints once every message is in the store, i.e. the end
/// of the seed phase.
const SEED_DONE_MARKER: &str = "Demo mailbox contains";

/// Wall budget for the seed phase — a regression tripwire, not a benchmark.
///
/// Provenance: on a 16-core dev Mac under load, a debug build reaches the
/// marker in 5.5s for these 2,000 messages (daemon start, rules, one paged
/// sync per account). Assuming a 4-vCPU CI runner is 3-5x slower, a healthy
/// run there lands around 17-28s, so 60s trips on a 2-3x regression while
/// still needing an ~11x slowdown to flake on a developer machine.
///
/// What it does not do: at 2,000 messages the pre-paging code was fast too,
/// so this cannot stand in for the 50,000-message numbers the seeding work
/// was measured against — it catches a catastrophic regression (the eager
/// fixture materialisation or the per-message commit path coming back),
/// while the paging and batched-write behaviour is pinned by unit tests.
const SEED_BUDGET: Duration = Duration::from_secs(60);

fn search_results<'a>(value: &'a Value, context: &str) -> &'a [Value] {
    value
        .as_array()
        .or_else(|| value.get("results").and_then(Value::as_array))
        .map_or_else(|| panic!("{context}; got: {value:#}"), Vec::as_slice)
}

/// A unix socket path is capped at ~104 bytes on macOS and temp dirs under
/// `/var/folders` blow straight past that, so the socket gets its own short
/// path under `/tmp` while the profile itself lives in the temp HOME.
struct ShortSocket {
    path: PathBuf,
}

impl ShortSocket {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("now")
            .as_nanos();
        Self {
            path: PathBuf::from(format!("/tmp/mxrd-{}-{stamp}.sock", std::process::id())),
        }
    }
}

impl Drop for ShortSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Shuts the demo daemon down however the test ends.
///
/// Built before the demo is started, not after: `mxr demo` leaves a detached
/// daemon behind, and a panic in any assertion below would otherwise leak it
/// for the lifetime of the CI job. `Drop` runs on a panicking unwind, which is
/// what the ordinary failure path here is.
struct DemoTeardown {
    bin: PathBuf,
    env: Vec<(String, String)>,
    data_dir: PathBuf,
    socket: PathBuf,
}

impl Drop for DemoTeardown {
    fn drop(&mut self) {
        let mut stop = std::process::Command::new(&self.bin);
        for (key, value) in &self.env {
            stop.env(key, value);
        }
        let _ = stop
            .args(["demo", "stop"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        // `demo stop` needs a reachable daemon; fall back to the pid file it
        // leaves behind so a wedged daemon still gets cleaned up.
        if self.socket.exists() {
            if let Ok(raw) = std::fs::read_to_string(self.data_dir.join("daemon.pid")) {
                if let Ok(pid) = raw.trim().parse::<u32>() {
                    let _ = std::process::Command::new("kill")
                        .arg(pid.to_string())
                        .status();
                }
            }
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Where the demo profile's runtime files land, which is not the same place on
/// every OS: `dirs::data_dir()` is `$XDG_DATA_HOME` on Linux and
/// `$HOME/Library/Application Support` on macOS.
fn demo_data_dir(home: &Path, data_home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("mxr-demo")
    } else {
        data_home.join("mxr-demo")
    }
}

/// Pre-create the demo profile's config with semantic search off.
///
/// `mxr demo` merges its account/LLM settings into whatever config already
/// exists, so this survives the demo's own rewrite. On a `semantic-local`
/// build the daemon would otherwise fetch a ~130 MB embedding model into the
/// throwaway HOME and embed every seeded message — a network dependency and
/// minutes of work that say nothing about the timeout this test guards.
///
/// Written to both candidate roots because `dirs::config_dir()` resolves to
/// `$XDG_CONFIG_HOME` on Linux and `$HOME/Library/Application Support` on
/// macOS. Writing only the XDG path leaves the file unread on a developer's
/// Mac, so the test would quietly measure something different from CI.
fn write_demo_config_without_semantic(home: &Path, config_home: &Path) {
    let contents = "[search.semantic]\nenabled = false\nauto_download_models = false\n\n[bridge]\nenabled = false\n";
    for root in [
        config_home.to_path_buf(),
        home.join("Library").join("Application Support"),
    ] {
        let dir = root.join("mxr-demo");
        std::fs::create_dir_all(&dir).expect("demo config dir");
        std::fs::write(dir.join("config.toml"), contents).expect("write demo config");
    }
}

#[test]
fn demo_seeds_a_usable_mailbox_and_stops_cleanly() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let home = temp.path().join("home");
    let config_home = home.join("config");
    let data_home = home.join("data");
    let runtime_dir = home.join("run");
    for dir in [&home, &config_home, &data_home, &runtime_dir] {
        std::fs::create_dir_all(dir).expect("create isolated dir");
    }
    write_demo_config_without_semantic(&home, &config_home);

    // `mxr demo` is pinned to the `mxr-demo` instance, and the daemon's
    // pid-file fallback finds a running demo daemon by scanning `ps` for
    // `<exe name> daemon --instance mxr-demo`. Driving the stock `mxr` binary
    // would therefore let this test adopt and shut down a developer's real
    // demo daemon (or another checkout's), and let theirs do the same to ours.
    let bin = link_mxr_binary(&temp.path().join("mxr-demo-e2e"));

    let socket = ShortSocket::new();
    let socket_str = socket.path.display().to_string();
    let env: Vec<(&str, &str)> = vec![
        // macOS resolves config/data from HOME; Linux from the XDG vars.
        ("HOME", home.to_str().expect("utf8 home")),
        (
            "XDG_CONFIG_HOME",
            config_home.to_str().expect("utf8 config home"),
        ),
        ("XDG_DATA_HOME", data_home.to_str().expect("utf8 data home")),
        (
            "XDG_RUNTIME_DIR",
            runtime_dir.to_str().expect("utf8 runtime dir"),
        ),
        ("MXR_SOCKET_PATH", socket_str.as_str()),
        ("MXR_KEYCHAIN", "off"),
        ("MXR_IPC_TIMEOUT_SECS", IPC_TIMEOUT_SECS),
    ];
    let messages = DEMO_MESSAGES.to_string();

    // Armed before the demo starts, so every exit path below tears the daemon
    // down — including a panic inside the demo run itself.
    let _teardown = DemoTeardown {
        bin: bin.clone(),
        env: env
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
        data_dir: demo_data_dir(&home, &data_home),
        socket: socket.path.clone(),
    };

    let started_at = Instant::now();
    let (demo, seed_elapsed) = run_with_env_until_line(
        &bin,
        &env,
        &["demo", "--no-tui", "--messages", messages.as_str()],
        SEED_DONE_MARKER,
    );
    let elapsed = started_at.elapsed();
    let seed_elapsed = seed_elapsed.unwrap_or_else(|| {
        panic!(
            "demo should report the seeded mailbox; stdout={}\nstderr={}",
            demo.stdout, demo.stderr
        )
    });
    assert!(
        seed_elapsed <= SEED_BUDGET,
        "seeding {DEMO_MESSAGES} messages took {seed_elapsed:?}, over the {SEED_BUDGET:?} budget; \
         the demo seed has regressed (whole run {elapsed:?})"
    );
    assert!(
        demo.stdout.contains("Demo mode is now active"),
        "demo should announce sticky mode; stdout={}\nstderr={}",
        demo.stdout,
        demo.stderr
    );
    // Daemon operation events only reach the client on the no-deadline
    // request path, so seeing the sync's own `OperationStarted` line is proof
    // that `SyncNow` did not regress onto the wall-clock-capped path. This
    // holds regardless of how fast the machine seeds.
    assert!(
        demo.stderr.contains("Starting sync"),
        "the seed sync must stream daemon progress events; stderr={}",
        demo.stderr
    );

    let status = run_json_with_env(&bin, &env, &["status", "--format", "json"]);
    assert!(
        status["daemon_pid"].as_u64().is_some(),
        "demo daemon should be running: {status:#}"
    );

    assert_eq!(
        status["total_messages"].as_u64(),
        Some(DEMO_MESSAGES as u64),
        "demo should seed exactly {DEMO_MESSAGES} messages in {elapsed:?}; got: {status:#}"
    );

    let demo_status = run_with_env(&bin, &env, &["demo", "status"]);
    assert!(
        demo_status.stdout.contains("Demo mode: active"),
        "demo status should report active; got: {}",
        demo_status.stdout
    );

    // The seeded mailbox has to be readable through the ordinary commands,
    // not just countable: search, then open one of its hits.
    let inbox = run_json_with_env(
        &bin,
        &env,
        &["search", "label:inbox", "--format", "json", "--limit", "5"],
    );
    let results = search_results(&inbox, "expected demo inbox search results");
    assert!(
        !results.is_empty(),
        "demo mailbox should be searchable; got: {inbox:#}"
    );
    let message_id = results[0]["message_id"]
        .as_str()
        .unwrap_or_else(|| panic!("search hit should carry a message_id; got: {inbox:#}"));
    let cat = run_json_with_env(&bin, &env, &["cat", message_id, "--format", "json"]);
    assert_eq!(
        cat["message_id"].as_str(),
        Some(message_id),
        "cat should open a demo message; got: {cat:#}"
    );

    // Demo surfaces are seeded too, so the first click on any of them shows
    // something. Snippets stand in for the rest of the batch.
    let snippets = run_with_env(&bin, &env, &["snippets", "list"]);
    assert!(
        snippets.stdout.contains("thanks"),
        "demo should seed snippets; got: {}",
        snippets.stdout
    );

    let stopped = run_with_env(&bin, &env, &["demo", "stop"]);
    assert!(
        stopped.stdout.contains("Demo mode stopped"),
        "demo stop should tear the profile down; got: {}",
        stopped.stdout
    );
    let after_stop = run_with_env(&bin, &env, &["demo", "status"]);
    assert!(
        after_stop.stdout.contains("Demo mode: inactive"),
        "demo should be inactive after stop; got: {}",
        after_stop.stdout
    );
}
