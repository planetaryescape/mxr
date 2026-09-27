//! Live preview for typed times: what a phrase resolves to, what was assumed,
//! and the other readings when it is ambiguous. Shared by the snooze panel
//! and the send-at / remind prompts so every TUI time field reads the same.

use mxr_config::SnoozeConfig;
use mxr_core::natural_time::{
    resolve_time_local, TimeResolution, TimeResolveError, TimeResolveErrorKind,
};

/// A typed time's last resolution and the reading Tab selected. It is
/// updated when the text changes, never while drawing, and Enter commits
/// exactly the instant it holds, so what was previewed is what is stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimePreview {
    resolution: Option<Result<TimeResolution, TimeResolveError>>,
    choice: usize,
}

impl TimePreview {
    /// Resolve `input` the way the daemon's `ResolveTime` does: local time
    /// with the user's snooze hours. Resets the Tab selection.
    pub fn update(&mut self, input: &str, config: &SnoozeConfig) {
        self.resolution = Some(resolve_time_local(input, &config.time_prefs()));
        self.choice = 0;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Move the Tab selection to the next reading, wrapping around.
    pub fn next_choice(&mut self) {
        if let Some(Ok(resolution)) = &self.resolution {
            if resolution.choices.len() > 1 {
                self.choice = (self.choice + 1) % resolution.choices.len();
            }
        }
    }

    /// The previewed instant for the selected reading, or why there isn't one.
    pub fn chosen(&self) -> Result<chrono::DateTime<chrono::Utc>, String> {
        match &self.resolution {
            Some(Ok(resolution)) => Ok(resolution
                .choices
                .get(self.choice)
                .map_or(resolution.at, |picked| picked.at)),
            Some(Err(error)) => Err(error.message.clone()),
            None => Err("Type a time, like \"tomorrow 9am\" or \"in 2h\".".into()),
        }
    }

    /// Whether the text resolved to a time (cheap enough to call per frame).
    pub fn is_resolved(&self) -> bool {
        matches!(self.resolution, Some(Ok(_)))
    }

    /// The preview lines to draw under the prompt.
    pub fn lines(&self) -> Vec<String> {
        self.resolution
            .as_ref()
            .map(|result| lines(result, self.choice))
            .unwrap_or_default()
    }
}

/// Preview lines for `input`. Empty input shows nothing so the prompt stays
/// quiet until the user types.
fn lines(result: &Result<TimeResolution, TimeResolveError>, choice: usize) -> Vec<String> {
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
        let mut preview = TimePreview {
            resolution: Some(Ok(resolution.clone())),
            choice: 0,
        };
        preview.next_choice();
        assert_eq!(preview.chosen(), Ok(resolution.choices[1].at));
        preview.next_choice();
        assert_eq!(preview.chosen(), Ok(resolution.at));
        assert!(TimePreview::default().chosen().is_err());
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
