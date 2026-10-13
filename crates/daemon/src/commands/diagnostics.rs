pub(crate) fn selected_target_metadata() -> anyhow::Result<(serde_json::Value, bool)> {
    let configured =
        std::env::var(mxr_transport::DAEMON_ADDR_ENV).is_ok_and(|value| !value.trim().is_empty());
    let target = crate::server::selected_daemon_target()?;
    let (kind, display) = if target.is_paired_remote() {
        let origin = mxr_transport::https_origin(&target.description())
            .map_err(|_| anyhow::anyhow!("saved remote origin is invalid"))?;
        ("paired_remote", origin)
    } else {
        let display = target.description();
        let kind = if display.starts_with("tcp://") {
            "tcp"
        } else if display.starts_with("cmd://") {
            "cmd"
        } else {
            "unix"
        };
        (kind, display)
    };
    Ok((
        serde_json::json!({"kind": kind, "display": display, "configured": configured}),
        configured || target.is_remote(),
    ))
}

pub(super) fn client_local_metadata() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "runtime_instance": mxr_config::app_instance_name(),
        "config_path": mxr_config::config_file_path(),
        "data_dir": mxr_config::data_dir(),
        // MXR_DAEMON_ADDR may select a different Unix socket. This field
        // always describes the client's own default profile.
        "socket_path": mxr_config::socket_path(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_config::connection_state::{self, ConnectionRegistration};
    use mxr_core::RemoteSecret;
    use std::{path::Path, process::Command};

    const CHILD_MODE: &str = "MXR_DIAGNOSTICS_TARGET_TEST";
    const CHILD_TEST: &str = "commands::diagnostics::tests::selected_target_metadata_child";

    fn saved_remote() -> ConnectionRegistration {
        ConnectionRegistration {
            capabilities: vec![mxr_core::RemoteCapability::MailRead],
            origin: "https://fixture.example".into(),
            client_id: "fixture-client".into(),
            host_id: "fixture-host".into(),
            workspace_id: "fixture-workspace".into(),
            recovery_generation: "1".into(),
            mail_secret: RemoteSecret::new("a".repeat(64)),
            cleanup_secret: RemoteSecret::new("b".repeat(64)),
        }
    }

    fn run_isolated_child(mode: &str) {
        let directory = tempfile::tempdir().expect("isolated diagnostics profile");
        let config_dir = directory.path().join("config");
        let data_dir = directory.path().join("data");
        let socket = directory.path().join("explicit-local.sock");
        std::fs::create_dir_all(&config_dir).expect("create config directory");
        let state_path = config_dir.join("client-connection.json");
        if mode != "local_default" {
            connection_state::update(&state_path, Some(0), move |state| {
                state.generation = 1;
                state.selected = Some(saved_remote());
                Ok(())
            })
            .expect("write saved remote selection");
        }

        let index = data_dir.join("search_index");
        std::fs::create_dir_all(&index).expect("create local index sentinel");
        std::fs::write(index.join("sentinel"), b"keep local index")
            .expect("write local index sentinel");

        let mut child = Command::new(std::env::current_exe().expect("test executable"));
        child
            .args(["--exact", CHILD_TEST])
            .env(CHILD_MODE, mode)
            .env("MXR_CONFIG_DIR", &config_dir)
            .env("MXR_DATA_DIR", &data_dir)
            .env("MXR_SOCKET_PATH", &socket);
        if mode == "explicit_local" {
            child.env(
                mxr_transport::DAEMON_ADDR_ENV,
                format!("unix://{}", socket.display()),
            );
        } else {
            child.env_remove(mxr_transport::DAEMON_ADDR_ENV);
        }
        let output = child.output().expect("run isolated diagnostics child");
        assert!(
            output.status.success(),
            "isolated diagnostics child failed ({mode})\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(index.join("sentinel")).expect("read local index sentinel"),
            b"keep local index"
        );
    }

    async fn assert_reindex_is_refused_without_local_db() {
        let data_dir = std::env::var_os("MXR_DATA_DIR").expect("isolated data dir");
        let error = crate::commands::doctor::run(crate::commands::doctor::DoctorRunOptions {
            reindex: true,
            reindex_semantic: false,
            backfill_semantic: false,
            check: false,
            semantic_status: false,
            verbose: false,
            index_stats: false,
            store_stats: false,
            rebuild_analytics: false,
            refresh_contacts: false,
            recompute_link_counts: false,
            format: None,
        })
        .await
        .expect_err("selected non-default target must prevent local reindex");
        assert!(error
            .to_string()
            .contains("only supports the local default profile"));
        assert!(!Path::new(&data_dir).join("mxr.db").exists());
    }

    #[test]
    fn selected_saved_remote_prevents_local_reindex_and_explicit_override_wins() {
        run_isolated_child("saved_remote");
        run_isolated_child("explicit_local");
        run_isolated_child("local_default");
    }

    #[tokio::test]
    async fn selected_target_metadata_child() {
        let Ok(mode) = std::env::var(CHILD_MODE) else {
            return;
        };
        let (metadata, remote) = selected_target_metadata().expect("resolve selected target");
        assert!(matches!(
            mode.as_str(),
            "saved_remote" | "explicit_local" | "local_default"
        ));
        match mode.as_str() {
            "saved_remote" => {
                assert!(remote);
                assert_eq!(metadata["kind"], "paired_remote");
                assert_eq!(metadata["display"], "https://fixture.example");
                assert_eq!(metadata["configured"], false);
                assert!(!metadata.to_string().contains(&"a".repeat(64)));
                assert!(!metadata.to_string().contains(&"b".repeat(64)));
                assert_reindex_is_refused_without_local_db().await;
            }
            "explicit_local" => {
                assert!(remote);
                assert_eq!(metadata["kind"], "unix");
                assert!(metadata["display"]
                    .as_str()
                    .unwrap()
                    .ends_with("explicit-local.sock"));
                assert_eq!(metadata["configured"], true);
                assert_reindex_is_refused_without_local_db().await;
            }
            "local_default" => {
                assert!(!remote);
                assert_eq!(metadata["kind"], "unix");
                assert_eq!(metadata["configured"], false);
            }
            _ => {}
        }
    }
}
