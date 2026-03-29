use crate::cli::ConfigAction;

pub fn run(action: Option<ConfigAction>) -> anyhow::Result<()> {
    match action.unwrap_or(ConfigAction::Edit) {
        ConfigAction::Path => {
            println!("{}", mxr_config::config_file_path().display());
        }
        ConfigAction::Edit => {
            let config_path = mxr_config::config_file_path();
            if !config_path.exists() {
                // Ensure config file exists with defaults before opening
                let config = mxr_config::load_config().unwrap_or_default();
                mxr_config::save_config(&config)?;
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
            let mut config = mxr_config::load_config()?;
            set_config_value(&mut config, &key, &value)?;
            mxr_config::save_config(&config)?;
            println!("Set {key} = {value}");
        }
    }
    Ok(())
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
        "appearance.sidebar" => Ok(config.appearance.sidebar.to_string()),
        "appearance.date_format" => Ok(config.appearance.date_format.clone()),
        "appearance.date_format_full" => Ok(config.appearance.date_format_full.clone()),
        "appearance.subject_max_width" => Ok(config.appearance.subject_max_width.to_string()),
        _ => anyhow::bail!("Unknown config key: {key}\n\nAvailable keys:\n  general.editor, general.default_account, general.sync_interval, general.hook_timeout, general.attachment_dir\n  render.html_command, render.reader_mode, render.show_reader_stats, render.html_remote_content\n  search.default_sort, search.max_results, search.semantic.enabled\n  snooze.morning_hour, snooze.evening_hour, snooze.weekend_day, snooze.weekend_hour\n  logging.level, logging.max_size_mb, logging.max_files, logging.stderr, logging.event_retention_days\n  appearance.theme, appearance.sidebar, appearance.date_format, appearance.date_format_full, appearance.subject_max_width"),
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
        "appearance.sidebar" => {
            config.appearance.sidebar = parse_bool(value)?;
        }
        "appearance.date_format" => config.appearance.date_format = value.to_string(),
        "appearance.date_format_full" => config.appearance.date_format_full = value.to_string(),
        "appearance.subject_max_width" => {
            config.appearance.subject_max_width = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid integer: {value}"))?;
        }
        _ => anyhow::bail!("Unknown or read-only config key: {key}\n\nRun 'mxr config get <key>' to see available keys."),
    }
    Ok(())
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("Invalid boolean: {value} (expected true/false)"),
    }
}
