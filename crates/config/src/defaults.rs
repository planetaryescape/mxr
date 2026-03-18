use std::path::PathBuf;

use crate::types::*;

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            editor: None,
            default_account: None,
            sync_interval: 60,
            attachment_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join("mxr")
                .join("attachments"),
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
