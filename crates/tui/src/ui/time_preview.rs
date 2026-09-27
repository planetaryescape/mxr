//! Live preview for typed times: what a phrase resolves to, what was assumed,
//! and the other readings when it is ambiguous. Shared by the snooze panel
//! and the send-at / remind prompts so every TUI time field reads the same.

use mxr_config::SnoozeConfig;
use mxr_core::natural_time::{
    resolve_time_local, TimeResolution, TimeResolveError, TimeResolveErrorKind,
};

/// Resolve a typed time the way the daemon's `ResolveTime` does: local time
/// with the user's snooze hours.
pub fn resolve(input: &str, config: &SnoozeConfig) -> Result<TimeResolution, TimeResolveError> {
    resolve_time_local(input, &config.time_prefs())
}

/// The choice the user picked with Tab, clamped to the choices that exist.
pub fn chosen(resolution: &TimeResolution, choice: usize) -> chrono::DateTime<chrono::Utc> {
    resolution
        .choices
        .get(choice)
        .map_or(resolution.at, |picked| picked.at)
}

/// Move the Tab selection to the next reading, wrapping around.
pub fn next_choice(input: &str, config: &SnoozeConfig, choice: usize) -> usize {
    match resolve(input, config) {
        Ok(resolution) if resolution.choices.len() > 1 => (choice + 1) % resolution.choices.len(),
        _ => 0,
    }
}

/// Preview lines for `input`. Empty input shows nothing so the prompt stays
/// quiet until the user types.
pub fn lines(result: &Result<TimeResolution, TimeResolveError>, choice: usize) -> Vec<String> {
    match result {
        Err(error) if error.kind == TimeResolveErrorKind::Empty => Vec::new(),
        Err(error) => vec![error.message.clone()],
        Ok(resolution) => {
            let index = choice.min(resolution.choices.len().saturating_sub(1));
            let Some(picked) = resolution.choices.get(index) else {
                return Vec::new();
            };
            let mut out = vec![format!("→ {}", picked.summary())];
            if resolution.is_ambiguous() {
                let options = resolution
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if i == index {
                            format!("[{}]", c.label)
                        } else {
                            c.label.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ");
                out.push(format!("Tab to switch: {options}"));
            }
            let assumed = picked.assumed();
            if !assumed.is_empty() {
                out.push(format!("Assumed {assumed}"));
            }
            if let Some(note) = &picked.note {
                out.push(note.clone());
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};
    use mxr_core::natural_time::{resolve_time, TimePrefs};

    fn at_tuesday(input: &str) -> Result<TimeResolution, TimeResolveError> {
        let tz = FixedOffset::east_opt(3600).expect("valid offset");
        let now = tz
            .with_ymd_and_hms(2024, 5, 7, 14, 0, 0)
            .single()
            .expect("valid time");
        resolve_time(input, &now, &TimePrefs::default())
    }

    #[test]
    fn ambiguous_phrase_lists_choices_with_the_selection_marked() {
        let result = at_tuesday("fri 3");
        assert_eq!(
            lines(&result, 0),
            [
                "→ Friday 10 May, 15:00 (in 3 days)",
                "Tab to switch: [15:00]  03:00",
                "Assumed am or pm",
            ]
        );
        assert_eq!(lines(&result, 1)[1], "Tab to switch: 15:00  [03:00]");
        let resolution = result.expect("resolves");
        assert_eq!(chosen(&resolution, 1), resolution.choices[1].at);
        assert_eq!(chosen(&resolution, 9), resolution.at);
    }

    #[test]
    fn errors_show_the_parser_message_and_empty_input_shows_nothing() {
        assert_eq!(
            lines(&at_tuesday("frday"), 0),
            ["Didn't catch \"frday\". Try \"fri 3pm\" or \"in 2d\"."]
        );
        assert!(lines(&at_tuesday(""), 0).is_empty());
    }
}
