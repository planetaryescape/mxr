use crate::types::*;

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            editor: None,
            default_account: None,
            sync_interval: 60,
            hook_timeout: 30,
            attachment_dir: crate::resolve::data_dir().join("attachments"),
        }
    }
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            html_command: None,
            reader_mode: true,
            show_reader_stats: true,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_sort: SortOrder::DateDesc,
            max_results: 200,
            default_mode: mxr_core::SearchMode::Lexical,
            semantic: SemanticConfig::default(),
        }
    }
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_download_models: true,
            active_profile: mxr_core::SemanticProfile::BgeSmallEnV15,
            max_pending_jobs: 256,
            query_timeout_ms: 1500,
        }
    }
}

impl Default for SnoozeConfig {
    fn default() -> Self {
        Self {
            morning_hour: 9,
            evening_hour: 18,
            weekend_day: "saturday".to_string(),
            weekend_hour: 10,
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            sidebar: true,
            date_format: "%b %d".to_string(),
            date_format_full: "%Y-%m-%d %H:%M".to_string(),
            subject_max_width: 60,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            max_size_mb: 250,
            max_files: 10,
            stderr: true,
            event_retention_days: 90,
        }
    }
}
