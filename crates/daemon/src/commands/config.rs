use crate::cli::{ConfigAction, OutputFormat};
use crate::output::resolve_format;

pub async fn run(action: Option<ConfigAction>) -> anyhow::Result<()> {
    match action.unwrap_or(ConfigAction::Show { format: None }) {
        ConfigAction::Show { format } => {
            let config = mxr_config::load_config().unwrap_or_default();
            match resolve_format(format) {
                OutputFormat::Json | OutputFormat::Jsonl => {
                    println!("{}", serde_json::to_string_pretty(&config)?);
                }
                _ => {
                    println!("{}", toml::to_string_pretty(&config)?);
                }
            }
        }
        ConfigAction::Path => {
            println!("{}", mxr_config::config_file_path().display());
        }
        ConfigAction::Edit => {
            let config_path = mxr_config::config_file_path();
            if !config_path.exists() {
                // Ensure config file exists with defaults before opening
                mxr_config::update_config(|_| Ok::<_, mxr_config::ConfigError>(()))?;
            }
            let editor = mxr_compose::editor::resolve_editor(None);
            let status = std::process::Command::new(&editor)
                .arg(&config_path)
                .status()
                .map_err(|e| anyhow::anyhow!("Failed to open editor '{editor}': {e}"))?;
            if !status.success() {
                anyhow::bail!("Editor exited with non-zero status");
            }
        }
        ConfigAction::Get { key } => {
            let config = mxr_config::load_config()?;
            let value = get_config_value(&config, &key)?;
            println!("{value}");
        }
        ConfigAction::Set { key, value } => {
            // Held from the read to the write: the running daemon saves config
            // too, and must not restore what this changes.
            mxr_config::update_config(|config| set_config_value(config, &key, &value))?;
            println!("Set {key} = {value}");
            if key.starts_with("llm.") {
                apply_llm_change_to_running_daemon().await;
            }
        }
    }
    Ok(())
}

/// The daemon keeps `[llm]` in memory, so a change written to config.toml
/// reaches it only when told. Only a daemon already running is asked: a
/// stopped one reads the file when it starts, and a config command must not
/// start one. If the daemon can't apply it, say how to.
async fn apply_llm_change_to_running_daemon() {
    use mxr_protocol::{Request, Response};

    let Ok(mut client) = crate::ipc_client::IpcClient::connect_to(&mxr_config::socket_path()).await
    else {
        return;
    };
    match client.request(Request::ReloadLlmConfig).await {
        Ok(Response::Ok { .. }) => println!("Applied to the running daemon."),
        Ok(Response::Error { message, .. }) => println!(
            "Saved, but the running daemon did not apply it ({message}). Restart the daemon to apply: mxr restart"
        ),
        Err(_) => println!(
            "Saved, but the running daemon did not answer. Restart the daemon to apply: mxr restart"
        ),
    }
}

fn get_config_value(config: &mxr_config::MxrConfig, key: &str) -> anyhow::Result<String> {
    match key {
        // general
        "general.editor" => Ok(config
            .general
            .editor
            .clone()
            .unwrap_or_else(|| "(not set)".into())),
        "general.default_account" => Ok(config
            .general
            .default_account
            .clone()
            .unwrap_or_else(|| "(not set)".into())),
        "general.sync_interval" => Ok(config.general.sync_interval.to_string()),
        "general.hook_timeout" => Ok(config.general.hook_timeout.to_string()),
        "general.attachment_dir" => Ok(config.general.attachment_dir.display().to_string()),
        // render
        "render.html_command" => Ok(config
            .render
            .html_command
            .clone()
            .unwrap_or_else(|| "(not set)".into())),
        "render.reader_mode" => Ok(config.render.reader_mode.to_string()),
        "render.show_reader_stats" => Ok(config.render.show_reader_stats.to_string()),
        "render.html_remote_content" => Ok(config.render.html_remote_content.to_string()),
        // search
        "search.default_sort" => Ok(format!("{:?}", config.search.default_sort).to_lowercase()),
        "search.max_results" => Ok(config.search.max_results.to_string()),
        "search.semantic.enabled" => Ok(config.search.semantic.enabled.to_string()),
        // llm
        "llm.enabled" => Ok(config.llm.enabled.to_string()),
        "llm.base_url" => Ok(config.llm.base_url.clone()),
        "llm.model" => Ok(config.llm.model.clone()),
        "llm.api_key_env" => Ok(config.llm.api_key_env.clone()),
        "llm.context_window" => Ok(config.llm.context_window.to_string()),
        "llm.request_timeout_secs" => Ok(config.llm.request_timeout_secs.to_string()),
        "llm.allow_cloud_relationship_data" => {
            Ok(config.llm.allow_cloud_relationship_data.to_string())
        }
        // snooze
        "snooze.morning_hour" => Ok(config.snooze.morning_hour.to_string()),
        "snooze.evening_hour" => Ok(config.snooze.evening_hour.to_string()),
        "snooze.weekend_day" => Ok(config.snooze.weekend_day.clone()),
        "snooze.weekend_hour" => Ok(config.snooze.weekend_hour.to_string()),
        // logging
        "logging.level" => Ok(config.logging.level.clone()),
        "logging.max_size_mb" => Ok(config.logging.max_size_mb.to_string()),
        "logging.max_files" => Ok(config.logging.max_files.to_string()),
        "logging.stderr" => Ok(config.logging.stderr.to_string()),
        "logging.event_retention_days" => Ok(config.logging.event_retention_days.to_string()),
        // appearance
        "appearance.theme" => Ok(config.appearance.theme.clone()),
        key if mxr_config::RETIRED_CONFIG_KEYS.contains(&key) => anyhow::bail!(retired_key_message(key)),
        _ => anyhow::bail!("Unknown config key: {key}\n\nAvailable keys:\n  general.editor, general.default_account, general.sync_interval, general.hook_timeout, general.attachment_dir\n  render.html_command, render.reader_mode, render.show_reader_stats, render.html_remote_content\n  search.default_sort, search.max_results, search.semantic.enabled\n  llm.enabled, llm.base_url, llm.model, llm.api_key_env, llm.context_window, llm.request_timeout_secs, llm.allow_cloud_relationship_data\n  snooze.morning_hour, snooze.evening_hour, snooze.weekend_day, snooze.weekend_hour\n  logging.level, logging.max_size_mb, logging.max_files, logging.stderr, logging.event_retention_days\n  appearance.theme"),
    }
}

fn set_config_value(
    config: &mut mxr_config::MxrConfig,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
    match key {
        // general
        "general.editor" => config.general.editor = Some(value.to_string()),
        "general.default_account" => config.general.default_account = Some(value.to_string()),
        "general.sync_interval" => {
            config.general.sync_interval = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "general.hook_timeout" => {
            config.general.hook_timeout = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "general.attachment_dir" => {
            config.general.attachment_dir = std::path::PathBuf::from(value);
        }
        // render
        "render.html_command" => config.render.html_command = Some(value.to_string()),
        "render.reader_mode" => {
            config.render.reader_mode = parse_bool(value)?;
        }
        "render.show_reader_stats" => {
            config.render.show_reader_stats = parse_bool(value)?;
        }
        "render.html_remote_content" => {
            config.render.html_remote_content = parse_bool(value)?;
        }
        // search
        "search.max_results" => {
            config.search.max_results = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "search.semantic.enabled" => {
            config.search.semantic.enabled = parse_bool(value)?;
        }
        // llm
        "llm.enabled" => {
            config.llm.enabled = parse_bool(value)?;
        }
        "llm.base_url" => config.llm.base_url = value.to_string(),
        "llm.model" => config.llm.model = value.to_string(),
        "llm.api_key_env" => config.llm.api_key_env = value.to_string(),
        "llm.context_window" => {
            config.llm.context_window = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "llm.request_timeout_secs" => {
            config.llm.request_timeout_secs = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "llm.allow_cloud_relationship_data" => {
            config.llm.allow_cloud_relationship_data = parse_bool(value)?;
        }
        // snooze
        "snooze.morning_hour" => {
            config.snooze.morning_hour = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "snooze.evening_hour" => {
            config.snooze.evening_hour = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "snooze.weekend_day" => config.snooze.weekend_day = value.to_string(),
        "snooze.weekend_hour" => {
            config.snooze.weekend_hour = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        // logging
        "logging.level" => config.logging.level = value.to_string(),
        "logging.max_size_mb" => {
            config.logging.max_size_mb = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "logging.max_files" => {
            config.logging.max_files = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        "logging.stderr" => {
            config.logging.stderr = parse_bool(value)?;
        }
        "logging.event_retention_days" => {
            config.logging.event_retention_days = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        // appearance
        "appearance.theme" => config.appearance.theme = value.to_string(),
        key if mxr_config::RETIRED_CONFIG_KEYS.contains(&key) => anyhow::bail!(retired_key_message(key)),
        _ => anyhow::bail!("Unknown or read-only config key: {key}\n\nRun 'mxr config get <key>' to see available keys."),
    }
    Ok(())
}

fn retired_key_message(key: &str) -> String {
    format!("`{key}` was retired and no longer does anything; remove it from config.toml")
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("Invalid boolean: {value} (expected true/false)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_llm_config_values() {
        let mut config = mxr_config::MxrConfig::default();
        config.llm.enabled = true;
        config.llm.base_url = "http://127.0.0.1:8000/v1".into();
        config.llm.model = "Qwen3.6-35B-A3B-MLX-8bit".into();
        config.llm.context_window = 80_000;
        config.llm.allow_cloud_relationship_data = true;

        assert_eq!(
            get_config_value(&config, "llm.enabled").expect("llm.enabled value"),
            "true"
        );
        assert_eq!(
            get_config_value(&config, "llm.base_url").expect("llm.base_url value"),
            "http://127.0.0.1:8000/v1"
        );
        assert_eq!(
            get_config_value(&config, "llm.model").expect("llm.model value"),
            "Qwen3.6-35B-A3B-MLX-8bit"
        );
        assert_eq!(
            get_config_value(&config, "llm.context_window").expect("llm.context_window value"),
            "80000"
        );
        assert_eq!(
            get_config_value(&config, "llm.allow_cloud_relationship_data")
                .expect("llm.allow_cloud_relationship_data value"),
            "true"
        );
    }

    #[test]
    fn sets_llm_config_values() {
        let mut config = mxr_config::MxrConfig::default();

        set_config_value(&mut config, "llm.enabled", "true").expect("set llm.enabled");
        set_config_value(&mut config, "llm.base_url", "http://127.0.0.1:8000/v1")
            .expect("set llm.base_url");
        set_config_value(&mut config, "llm.model", "Qwen3.6-35B-A3B-MLX-8bit")
            .expect("set llm.model");
        set_config_value(&mut config, "llm.context_window", "80000")
            .expect("set llm.context_window");
        set_config_value(&mut config, "llm.request_timeout_secs", "30")
            .expect("set llm.request_timeout_secs");
        set_config_value(&mut config, "llm.allow_cloud_relationship_data", "true")
            .expect("set llm.allow_cloud_relationship_data");

        assert!(config.llm.enabled);
        assert_eq!(config.llm.base_url, "http://127.0.0.1:8000/v1");
        assert_eq!(config.llm.model, "Qwen3.6-35B-A3B-MLX-8bit");
        assert_eq!(config.llm.context_window, 80_000);
        assert_eq!(config.llm.request_timeout_secs, 30);
        assert!(config.llm.allow_cloud_relationship_data);
    }

    #[test]
    fn retired_appearance_keys_are_rejected_on_get_and_set() {
        let mut config = mxr_config::MxrConfig::default();
        for key in mxr_config::RETIRED_CONFIG_KEYS {
            let get_error = get_config_value(&config, key).expect_err("get must reject");
            assert!(
                get_error.to_string().contains("was retired"),
                "{key}: {get_error}"
            );
            let set_error = set_config_value(&mut config, key, "1").expect_err("set must reject");
            assert!(
                set_error.to_string().contains("was retired"),
                "{key}: {set_error}"
            );
        }
    }

    #[test]
    fn theme_is_still_settable() {
        let mut config = mxr_config::MxrConfig::default();
        set_config_value(&mut config, "appearance.theme", "midnight").expect("set theme");
        assert_eq!(
            get_config_value(&config, "appearance.theme").expect("get theme"),
            "midnight"
        );
    }
}
