use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Actions a rule can perform on a matching message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    AddLabel {
        label: String,
    },
    RemoveLabel {
        label: String,
    },
    Archive,
    Trash,
    Star,
    MarkRead,
    MarkUnread,
    Snooze {
        duration: SnoozeDuration,
    },
    /// Run external command with message JSON on stdin.
    ShellHook {
        command: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SnoozeDuration {
    Hours { count: u32 },
    Days { count: u32 },
    Until { date: DateTime<Utc> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_roundtrip_through_json() {
        let actions = vec![
            RuleAction::Archive,
            RuleAction::AddLabel {
                label: "important".into(),
            },
            RuleAction::ShellHook {
                command: "notify-send 'New mail'".into(),
            },
            RuleAction::Snooze {
                duration: SnoozeDuration::Hours { count: 4 },
            },
        ];

        let json = serde_json::to_string(&actions).unwrap();
        let parsed: Vec<RuleAction> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 4);
    }

    #[test]
    fn snooze_duration_variants_serialize() {
        let durations = vec![
            SnoozeDuration::Hours { count: 2 },
            SnoozeDuration::Days { count: 7 },
            SnoozeDuration::Until {
                date: chrono::Utc::now(),
            },
        ];

        for d in durations {
            let json = serde_json::to_string(&d).unwrap();
            let _: SnoozeDuration = serde_json::from_str(&json).unwrap();
        }
    }
}
