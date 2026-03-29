// Consolidated snooze logic lives in mxr_config::snooze.
// This module re-exports for backward compatibility.
pub use mxr_config::snooze::{
    format_preset, next_weekday_at, parse_snooze_until, resolve_snooze_time, SnoozeOption,
    SnoozePreset, SNOOZE_PRESETS,
};
