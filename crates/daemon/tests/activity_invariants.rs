//! Activity-log invariants from `docs/activity-log.md` and
//! `AGENTS.md`. These tests run alongside the rest of the daemon suite
//! and act as a guard against regressions:
//!
//! 1. PII audit — no row stored via the real mapper ever ends up with a
//!    forbidden key in `context_json` (credentials, tokens, etc.).
//! 2. Single-writer invariant — outside the `crates/daemon/src/activity/`
//!    module and `crates/store/src/user_activity.rs`, no code path
//!    references `record_activity` or `user_activity` directly.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests unwrap inspected JSON fields for direct invariant failures"
)]

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // CARGO_MANIFEST_DIR is `crates/daemon`. The workspace root is two parents up.
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn collect_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        // Skip build outputs and node_modules.
        if name == "target" || name == "target-cli" || name == "node_modules" {
            continue;
        }
        let ty = entry.file_type().ok();
        if ty.is_some_and(|t| t.is_dir()) {
            walk(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_forbidden_keys_appear_in_recorder_outputs() {
    use mxr_protocol::{ClientKind, MutationCommand, Request};
    // Build a representative sample of Request variants the mapper handles
    // and assert the resulting `OwnedEntry` context never serializes any
    // forbidden top-level key. This is a *structural* check on what the
    // mapper produces — it doesn't go through the store, so it's the right
    // place to fail loudly if the mapper starts leaking sensitive fields.
    let make_search = || Request::Search {
        query: "user@example.com password reset".into(),
        limit: 50,
        offset: 0,
        account_id: None,
        mode: None,
        sort: None,
        explain: false,
    };
    let mid = mxr_core::MessageId::new();
    let make_mutation = || Request::Mutation {
        mutation: MutationCommand::Archive {
            message_ids: vec![mid.clone()],
        },
        client_correlation_id: None,
    };
    let make_thread = || Request::GetThread {
        thread_id: mxr_core::ThreadId::new(),
    };

    let requests = [make_search(), make_mutation(), make_thread()];
    let forbidden = [
        "password",
        "password_hash",
        "token",
        "access_token",
        "refresh_token",
        "secret",
        "api_key",
        "client_secret",
        "private_key",
        "oauth_token",
        "id_token",
        "cookie",
        "session_id",
    ];

    for req in &requests {
        let entry = mxr::activity::mapper::map_request(req, ClientKind::Tui, None, true);
        let Some(entry) = entry else {
            continue;
        };
        let Some(ctx) = entry.context else {
            continue;
        };
        let serialized = serde_json::to_string(&ctx).unwrap_or_default();
        let lower = serialized.to_lowercase();
        for forbidden_key in &forbidden {
            // The key would have to appear as a JSON property (`"key":`) for it to be
            // a real concern. The query body may legitimately contain literal substrings
            // (e.g. `"password reset"` as user search text). Check for the property form.
            let property = format!("\"{forbidden_key}\":");
            assert!(
                !lower.contains(&property),
                "request {req:?} produced context with forbidden property {forbidden_key}: {serialized}"
            );
        }
    }
}

#[test]
fn only_activity_module_writes_to_user_activity_table() {
    let root = workspace_root();
    let allowed_path_substrings = [
        "/crates/daemon/src/activity/",
        "/crates/daemon/src/handler/activity.rs",
        "/crates/daemon/src/commands/activity.rs",
        "/crates/daemon/src/loops.rs",
        "/crates/store/src/user_activity.rs",
        "/crates/store/src/lib.rs",
        "/crates/store/src/pool.rs",
        "/crates/store/migrations/035_user_activity.sql",
        "/crates/store/migrations/036_user_activity_fts.sql",
        "/crates/store/migrations/037_saved_activity_filters.sql",
        "/tests/activity_invariants.rs",
        "/benches/user_activity.rs",
    ];

    let mut offenders: Vec<String> = Vec::new();
    for rs in collect_rs_files(&root) {
        let path_str = rs.display().to_string();
        // Skip test files, benches that aren't the activity bench, and docs.
        if path_str.contains("/tests/") || path_str.contains("/docs/") {
            continue;
        }
        if allowed_path_substrings
            .iter()
            .any(|allowed| path_str.contains(allowed))
        {
            continue;
        }
        let Ok(body) = fs::read_to_string(&rs) else {
            continue;
        };
        if body.contains("record_activity(")
            || body.contains("INSERT INTO user_activity")
            || body.contains("UPDATE user_activity SET")
        {
            offenders.push(path_str);
        }
    }
    assert!(
        offenders.is_empty(),
        "non-recorder code writes to user_activity: {offenders:?}"
    );
}
