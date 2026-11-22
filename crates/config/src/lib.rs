pub mod connection_state;
mod defaults;
mod hosting;
mod resolve;
mod secret_store;
pub mod snooze;
mod types;

pub use hosting::{hosting_daemon_desired_stopped, lock_hosting_daemon_startup};
pub use resolve::{
    app_instance_name, apply_env_overrides, bridge_port_path, bridge_token_path, clear_bridge_port,
    config_dir, config_file_path, daemon_token_path, data_dir, gmail_oauth_keychain_service,
    is_demo_instance, load_config, load_config_as_written, load_config_file_as_written,
    load_config_from_path, load_config_from_str, lock_config_file, lock_config_file_at,
    read_bridge_port, read_or_create_bridge_token, remote_bridge_token_path, resolve_daemon_token,
    retired_config_keys_in, save_config, save_config_to_path, scoped_credential_service,
    secrets_file_path, socket_path, token_dir, unknown_llm_override_keys_in, update_config,
    update_config_at_path, write_bridge_port, ConfigError, ConfigFileLock, DAEMON_TOKEN_ENV,
    DEMO_INSTANCE_NAME, DEV_INSTANCE_NAME, LLM_OVERRIDE_FEATURES, LLM_OVERRIDE_SETTINGS,
    PROD_INSTANCE_NAME, RETIRED_CONFIG_KEYS,
};
pub use secret_store::SecretStore;
pub use types::*;

/// Read a file only this user may read, refusing a symlink and tightening
/// looser permissions, as the secrets file is read. `Ok(None)` when missing.
pub fn read_private_file(path: &std::path::Path) -> std::io::Result<Option<String>> {
    secret_store::read_secrets_file(path)
}

/// Replace a file atomically (temp file, fsync, rename) with permissions for
/// this user only, as the secrets file is written.
pub fn write_private_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    secret_store::write_secrets_atomic(path, contents)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests use panic and unwrap for direct fixture failures"
    )]

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn retired_keys_still_load_and_are_reported() {
        let toml_str = r#"
[appearance]
theme = "catppuccin"
sidebar = false
date_format = "%m/%d"
date_format_full = "%Y-%m-%d %H:%M:%S"
subject_max_width = 80

[humanizer]
apply_to_summaries = true

[llm.overrides.voice_match]
model = "gemma4:latest"

[llm.overrides.expert]
model = "gemma4:latest"
"#;

        let config = load_config_from_str(toml_str).expect("retired keys must not break loading");
        assert_eq!(config.appearance.theme, "catppuccin");

        let table: toml::Table = toml_str.parse().expect("parse raw toml");
        assert_eq!(retired_config_keys_in(&table), RETIRED_CONFIG_KEYS);
    }

    #[test]
    fn unknown_override_features_and_settings_are_named() {
        let toml_str = r#"
[llm.overrides.summarize]
model = "gemma4:latest"
allow_cloud_relationship_data = true

[llm.overrides.sumarize]
model = "gemma4:latest"

[llm.overrides.voice_match]
model = "gemma4:latest"
"#;
        let config = load_config_from_str(toml_str).expect("unknown keys must not break loading");
        assert_eq!(
            config
                .llm
                .overrides
                .summarize
                .and_then(|summarize| summarize.model)
                .as_deref(),
            Some("gemma4:latest")
        );
        let table: toml::Table = toml_str.parse().unwrap();
        let mut unknown = unknown_llm_override_keys_in(&table);
        unknown.sort_unstable();
        assert_eq!(
            unknown,
            [
                "llm.overrides.sumarize",
                "llm.overrides.summarize.allow_cloud_relationship_data",
            ]
        );
    }

    #[test]
    fn env_override_sync_interval() {
        let tmp = TempDir::new().expect("create temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "[general]\nsync_interval = 60\n").expect("write config");

        let config = temp_env::with_var("MXR_SYNC_INTERVAL", Some("30"), || {
            load_config_from_path(&config_path)
        })
        .expect("load config");

        assert_eq!(config.general.sync_interval, 30);
    }

    #[test]
    fn agent_surface_profiles_parse_with_safe_defaults() {
        let toml_str = r#"
[agents.profiles.agent]
safety_policy = "draft-only"
allowed_accounts = ["work"]
allow_send = false
allow_destructive = false

[agents.profiles.mcp]
safety_policy = "read-only"
allowed_accounts = ["personal"]
"#;

        let config = load_config_from_str(toml_str).expect("parse agent profiles");
        let agent = &config.agent_surfaces.profiles["agent"];
        assert_eq!(agent.safety_policy, SafetyPolicy::DraftOnly);
        assert_eq!(agent.allowed_accounts, vec!["work"]);
        assert!(!agent.allow_send);
        assert!(!agent.allow_destructive);

        let mcp = &config.agent_surfaces.profiles["mcp"];
        assert_eq!(mcp.safety_policy, SafetyPolicy::ReadOnly);
        assert_eq!(mcp.allowed_accounts, vec!["personal"]);
        assert!(!mcp.allow_send);
        assert!(!mcp.allow_destructive);
    }

    #[test]
    fn credentials_are_scoped_for_non_prod_instances_only() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr"), || {
            assert_eq!(scoped_credential_service("mxr/work-imap"), "mxr/work-imap");
            assert_eq!(gmail_oauth_keychain_service(), "mxr-gmail-oauth");
        });

        temp_env::with_var("MXR_INSTANCE", Some("mxr-dev"), || {
            assert_eq!(
                scoped_credential_service("mxr/work-imap"),
                "mxr-dev/mxr/work-imap"
            );
            assert_eq!(gmail_oauth_keychain_service(), "mxr-dev-gmail-oauth");
        });
    }

    #[test]
    fn daemon_token_is_separate_from_the_bridge_token() {
        temp_env::with_vars(
            [
                ("MXR_DAEMON_TOKEN_PATH", None::<&std::ffi::OsStr>),
                ("MXR_BRIDGE_TOKEN_PATH", None),
                ("MXR_DAEMON_TOKEN", None),
            ],
            || {
                assert_ne!(daemon_token_path(), bridge_token_path());
            },
        );
    }

    #[test]
    fn daemon_token_created_with_mode_600_and_env_wins() {
        let tmp = TempDir::new().expect("create temp dir");
        let token_path = tmp.path().join("daemon-token");

        temp_env::with_vars(
            [
                (
                    "MXR_DAEMON_TOKEN_PATH",
                    Some(token_path.as_os_str().to_owned()),
                ),
                ("MXR_DAEMON_TOKEN", None),
            ],
            || {
                let created = resolve_daemon_token(true)
                    .expect("create token")
                    .expect("some");
                assert!(!created.is_empty());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = std::fs::metadata(&token_path).unwrap().permissions().mode() & 0o777;
                    assert_eq!(mode, 0o600, "daemon token file must be 0600");
                }
                let read_back = resolve_daemon_token(false)
                    .expect("read token")
                    .expect("some");
                assert_eq!(created, read_back);
            },
        );

        temp_env::with_vars(
            [
                (
                    "MXR_DAEMON_TOKEN_PATH",
                    Some(token_path.as_os_str().to_owned()),
                ),
                (
                    "MXR_DAEMON_TOKEN",
                    Some(std::ffi::OsString::from("env-wins")),
                ),
            ],
            || {
                assert_eq!(
                    resolve_daemon_token(false).unwrap().as_deref(),
                    Some("env-wins")
                );
            },
        );
    }

    #[test]
    fn daemon_token_read_tightens_too_open_permissions() {
        let tmp = TempDir::new().expect("create temp dir");
        let token_path = tmp.path().join("daemon-token");
        std::fs::write(&token_path, "loose-token\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        temp_env::with_vars(
            [
                (
                    "MXR_DAEMON_TOKEN_PATH",
                    Some(token_path.as_os_str().to_owned()),
                ),
                ("MXR_DAEMON_TOKEN", None),
            ],
            || {
                let token = resolve_daemon_token(false).unwrap().expect("some");
                assert_eq!(token, "loose-token");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = std::fs::metadata(&token_path).unwrap().permissions().mode() & 0o777;
                    assert_eq!(mode, 0o600, "a too-open token file is tightened on read");
                }
            },
        );
    }
}
