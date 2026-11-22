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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
        if name == "target"
            || name == "target-cli"
            || name == "node_modules"
            || name.starts_with('.')
        {
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
    let make_search = || Request::Search {
        limit_from_config: false,
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
        window: None,
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
            let property = format!("\"{forbidden_key}\":");
            assert!(
                !lower.contains(&property),
                "request {req:?} produced context with forbidden property {forbidden_key}: {serialized}"
            );
        }
    }
}

#[tokio::test]
async fn activity_privacy_config_filters_real_mapper_shapes_before_persistence() {
    use mxr::activity::{mapper::map_request, Recorder};
    use mxr_protocol::{ClientKind, Request, ScreenerDispositionData};
    use std::sync::Arc;

    let config: mxr_config::MxrConfig = toml::from_str(
        "[activity]\ntrack_subjects = false\ntrack_recipient_handles = false\ntrack_search_queries = false\n",
    ).unwrap();
    let store = Arc::new(mxr_store::Store::in_memory().await.unwrap());
    let recorder = Recorder::spawn(store.clone(), &config.activity);
    let requests = [
        Request::Search {
            limit_from_config: false,
            query: "PRIVATE_QUERY".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        },
        Request::SetScreenerDecision {
            account_id: mxr_core::AccountId::new(),
            sender_email: "private@example.com".into(),
            disposition: ScreenerDispositionData::Allow,
            route_label: None,
        },
    ];
    for request in requests {
        recorder.record(map_request(&request, ClientKind::Agent, None, true).unwrap());
    }
    recorder.pause(None);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !recorder.pause_status().0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let rows = store
        .list_activity(&mxr_store::ActivityFilter::default(), 10, None)
        .await
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.source, "agent");
        assert!(row.target_id.is_none());
        let context: serde_json::Value =
            serde_json::from_str(row.context_json.as_deref().unwrap()).unwrap();
        assert!(context.get("query").is_none());
        assert!(context.get("sender_email").is_none());
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
        "/tests/integration/activity_invariants.rs",
        "/benches/user_activity.rs",
    ];

    let mut offenders: Vec<String> = Vec::new();
    for rs in collect_rs_files(&root) {
        let path_str = rs.display().to_string();
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
