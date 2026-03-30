mod defaults;
mod resolve;
pub mod snooze;
mod types;

pub use resolve::{
    app_instance_name, config_dir, config_file_path, data_dir, load_config, load_config_from_path,
    load_config_from_str, save_config, save_config_to_path, socket_path, ConfigError,
};
pub use types::*;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_config_is_valid() {
        let config = MxrConfig::default();
        let serialized = toml::to_string(&config).expect("serialize default config");
        let deserialized: MxrConfig =
            toml::from_str(&serialized).expect("deserialize default config");
        assert_eq!(deserialized.general.sync_interval, 60);
        assert_eq!(deserialized.general.hook_timeout, 30);
        assert_eq!(deserialized.search.max_results, 200);
        assert_eq!(
            deserialized.search.default_mode,
            mxr_core::SearchMode::Lexical
        );
        assert!(!deserialized.search.semantic.enabled);
        assert_eq!(
            deserialized.search.semantic.active_profile,
            mxr_core::SemanticProfile::BgeSmallEnV15
        );
        assert_eq!(deserialized.logging.event_retention_days, 90);
        assert!(deserialized.accounts.is_empty());
        assert!(deserialized.render.html_remote_content);
    }

    #[test]
    fn full_toml_round_trip() {
        let toml_str = r#"
[general]
editor = "nvim"
default_account = "personal"
sync_interval = 120
hook_timeout = 45
attachment_dir = "/tmp/attachments"

[accounts.personal]
name = "Personal"
email = "me@example.com"

[accounts.personal.sync]
type = "gmail"
client_id = "abc123"
client_secret = "secret"
token_ref = "keyring:gmail-personal"

[accounts.personal.send]
type = "smtp"
host = "smtp.example.com"
port = 587
username = "me@example.com"
password_ref = "keyring:smtp-personal"
use_tls = true

[render]
html_command = "w3m -dump -T text/html"
reader_mode = false
show_reader_stats = false
html_remote_content = false

[search]
default_sort = "relevance"
max_results = 50
default_mode = "hybrid"

[search.semantic]
enabled = true
auto_download_models = false
active_profile = "multilingual-e5-small"
max_pending_jobs = 32
query_timeout_ms = 3000

[snooze]
morning_hour = 8
evening_hour = 20
weekend_day = "sunday"
weekend_hour = 11

[logging]
level = "debug"
max_size_mb = 100
max_files = 5
stderr = false
event_retention_days = 30

[appearance]
theme = "catppuccin"
sidebar = false
date_format = "%m/%d"
date_format_full = "%Y-%m-%d %H:%M:%S"
subject_max_width = 80
"#;

        let config: MxrConfig = toml::from_str(toml_str).expect("parse full toml");
        assert_eq!(config.general.editor.as_deref(), Some("nvim"));
        assert_eq!(config.general.sync_interval, 120);
        assert_eq!(config.general.hook_timeout, 45);
        assert_eq!(config.accounts.len(), 1);

        let personal = &config.accounts["personal"];
        assert_eq!(personal.email, "me@example.com");

        let serialized = toml::to_string(&config).expect("re-serialize");
        let round_tripped: MxrConfig = toml::from_str(&serialized).expect("round-trip deserialize");
        assert_eq!(round_tripped.search.max_results, 50);
        assert_eq!(
            round_tripped.search.default_mode,
            mxr_core::SearchMode::Hybrid
        );
        assert!(round_tripped.search.semantic.enabled);
        assert_eq!(
            round_tripped.search.semantic.active_profile,
            mxr_core::SemanticProfile::MultilingualE5Small
        );
        assert_eq!(round_tripped.logging.max_files, 5);
        assert_eq!(round_tripped.appearance.theme, "catppuccin");
        assert!(!round_tripped.render.html_remote_content);
    }

    #[test]
    fn partial_toml_uses_defaults() {
        let toml_str = r#"
[general]
editor = "emacs"
"#;

        let config = load_config_from_str(toml_str).expect("parse partial toml");
        assert_eq!(config.general.editor.as_deref(), Some("emacs"));
        // Rest should be defaults
        assert_eq!(config.general.sync_interval, 60);
        assert_eq!(config.general.hook_timeout, 30);
        assert!(config.render.reader_mode);
        assert_eq!(config.search.max_results, 200);
        assert_eq!(config.search.default_mode, mxr_core::SearchMode::Lexical);
        assert_eq!(
            config.search.semantic.active_profile,
            mxr_core::SemanticProfile::BgeSmallEnV15
        );
        assert_eq!(config.snooze.morning_hour, 9);
        assert_eq!(config.logging.event_retention_days, 90);
        assert_eq!(config.appearance.subject_max_width, 60);
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
    fn xdg_paths_correct() {
        temp_env::with_var("MXR_INSTANCE", None::<&str>, || {
            let cfg = config_dir();
            assert!(
                cfg.ends_with("mxr"),
                "config_dir should end with 'mxr': {:?}",
                cfg
            );

            let data = data_dir();
            assert!(
                data.ends_with(app_instance_name()),
                "data_dir should end with instance name '{}': {:?}",
                app_instance_name(),
                data
            );

            let file = config_file_path();
            assert!(
                file.ends_with("config.toml"),
                "config_file_path should end with 'config.toml': {:?}",
                file
            );

            let socket = socket_path();
            assert!(
                socket.ends_with("mxr.sock"),
                "socket_path should end with 'mxr.sock': {:?}",
                socket
            );
        });
    }

    #[test]
    fn instance_name_can_be_overridden() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr-test"), || {
            assert_eq!(app_instance_name(), "mxr-test");
            assert!(data_dir().ends_with("mxr-test"));
        });
    }

    #[test]
    fn path_overrides_can_be_set_via_env() {
        let tmp = TempDir::new().expect("create temp dir");
        let config_dir_override = tmp.path().join("cfg");
        let data_dir_override = tmp.path().join("data");
        let socket_path_override = tmp.path().join("sock").join("mxr.sock");

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir_override.as_os_str())),
                ("MXR_DATA_DIR", Some(data_dir_override.as_os_str())),
                ("MXR_SOCKET_PATH", Some(socket_path_override.as_os_str())),
            ],
            || {
                assert_eq!(config_dir(), config_dir_override);
                assert_eq!(config_file_path(), config_dir_override.join("config.toml"));
                assert_eq!(data_dir(), data_dir_override);
                assert_eq!(socket_path(), socket_path_override);
            },
        );
    }

    #[test]
    fn missing_file_returns_defaults() {
        let tmp = TempDir::new().expect("create temp dir");
        let config_path = tmp.path().join("nonexistent.toml");

        let config = temp_env::with_vars(
            [
                ("MXR_EDITOR", None::<&str>),
                ("MXR_SYNC_INTERVAL", None::<&str>),
                ("MXR_DEFAULT_ACCOUNT", None::<&str>),
                ("MXR_ATTACHMENT_DIR", None::<&str>),
                ("MXR_CONFIG_DIR", None::<&str>),
                ("MXR_DATA_DIR", None::<&str>),
                ("MXR_SOCKET_PATH", None::<&str>),
            ],
            || load_config_from_path(&config_path),
        )
        .expect("load missing file");
        assert_eq!(config.general.sync_interval, 60);
        assert!(config.accounts.is_empty());
        assert!(config.render.reader_mode);
    }

    #[test]
    fn invalid_toml_returns_error() {
        let tmp = TempDir::new().expect("create temp dir");
        let config_path = tmp.path().join("bad.toml");
        std::fs::write(&config_path, "this is not [valid toml {{{{").expect("write bad config");

        let result = load_config_from_path(&config_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ParseToml { path, .. } => {
                assert_eq!(path, config_path);
            }
            other => panic!("expected ParseToml, got: {:?}", other),
        }
    }

    #[test]
    fn account_config_variants() {
        let toml_str = r#"
[accounts.work]
name = "Work"
email = "work@corp.com"

[accounts.work.sync]
type = "gmail"
client_id = "work-client-id"
token_ref = "keyring:gmail-work"

[accounts.work.send]
type = "smtp"
host = "smtp.corp.com"
port = 465
username = "work@corp.com"
password_ref = "keyring:smtp-work"
use_tls = true

[accounts.newsletter]
name = "Newsletter"
email = "news@corp.com"

[accounts.newsletter.send]
type = "gmail"
"#;

        let config = load_config_from_str(toml_str).expect("parse account variants");
        assert_eq!(config.accounts.len(), 2);

        let work = &config.accounts["work"];
        assert!(matches!(work.sync, Some(SyncProviderConfig::Gmail { .. })));
        assert!(matches!(work.send, Some(SendProviderConfig::Smtp { .. })));

        if let Some(SendProviderConfig::Smtp { port, use_tls, .. }) = &work.send {
            assert_eq!(*port, 465);
            assert!(*use_tls);
        }

        let newsletter = &config.accounts["newsletter"];
        assert!(newsletter.sync.is_none());
        assert!(matches!(newsletter.send, Some(SendProviderConfig::Gmail)));
    }

    #[test]
    fn imap_sync_variant_parses() {
        let toml_str = r#"
[accounts.fastmail]
name = "Fastmail"
email = "me@fastmail.com"

[accounts.fastmail.sync]
type = "imap"
host = "imap.fastmail.com"
port = 993
username = "me@fastmail.com"
password_ref = "keyring:fastmail-imap"
use_tls = true

[accounts.fastmail.send]
type = "smtp"
host = "smtp.fastmail.com"
port = 465
username = "me@fastmail.com"
password_ref = "keyring:fastmail-smtp"
use_tls = true
"#;

        let config = load_config_from_str(toml_str).expect("parse imap account");
        let fastmail = &config.accounts["fastmail"];
        assert!(matches!(
            fastmail.sync,
            Some(SyncProviderConfig::Imap { .. })
        ));
        assert!(matches!(
            fastmail.send,
            Some(SendProviderConfig::Smtp { .. })
        ));
    }
}
