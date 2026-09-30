use super::AiLocalityData;
use mxr_core::id::*;
use serde::{Deserialize, Serialize};

/// Where an AI draft came from: the model that actually answered, whether
/// the user's other mail reached it, and the messages it read, so clients
/// can say so and open each source. Describes the request as it ran (the
/// provider pinned for it), not the configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftProvenanceData {
    /// The model that wrote the draft, as its endpoint named it.
    pub model: String,
    pub locality: AiLocalityData,
    /// Something from the user's other mail reached the model: their past
    /// emails, the writing habits drawn from them, or the relationship
    /// summary. False when the privacy gate kept it back (a cloud model
    /// without `llm.allow_cloud_relationship_data`) or there was none.
    pub history_used: bool,
    /// The user's own past emails the model was shown to copy their voice,
    /// in the order the prompt carried them.
    #[serde(default)]
    pub voice_examples: Vec<DraftSourceData>,
    /// Messages of the conversation being answered that fit in the prompt,
    /// oldest first. Empty for a new message or a refine.
    #[serde(default)]
    pub conversation: Vec<DraftSourceData>,
    /// The humanizer's second model pass (to read less machine-made): who
    /// ran it and whether its text was used. Absent when no pass was due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite: Option<DraftRewriteProvenanceData>,
}

/// One message a draft was written from, in the user's own mailbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftSourceData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub date: chrono::DateTime<chrono::Utc>,
    /// The user sent it.
    pub from_me: bool,
    /// The other person's address: who the user wrote to, or who wrote it.
    pub person: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_name: Option<String>,
}

/// The model pass that rewrites a draft after it was written.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftRewriteProvenanceData {
    /// The model that answered the rewrite (or would have, when skipped).
    pub model: String,
    pub locality: AiLocalityData,
    /// The rewrite saw the user's habits and past emails, to keep the voice.
    pub history_used: bool,
    #[serde(default)]
    pub outcome: DraftRewriteOutcomeData,
    /// A later pass, after the text that was kept, whose answer was
    /// dropped: this model saw the draft too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected_by: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftRewriteOutcomeData {
    /// The rewritten text is what the user got.
    #[default]
    Applied,
    /// The model was called and saw the draft, but its text wasn't better,
    /// so the original was kept.
    Rejected,
    /// Not called: the draft was written from the user's history and the
    /// rewrite model is a cloud one without their opt-in.
    Skipped,
}

impl DraftProvenanceData {
    /// One quiet line for clients: "Local model gemma4 · used 5 of your
    /// emails to Maya · history used". The web app words it the same way.
    pub fn summary_line(&self) -> String {
        let mut parts = vec![model_label(self.locality, &self.model)];
        if let Some(first) = self.voice_examples.first() {
            let count = self.voice_examples.len();
            let same_person = self
                .voice_examples
                .iter()
                .all(|source| source.person.eq_ignore_ascii_case(&first.person));
            parts.push(if same_person && !first.person.is_empty() {
                format!("used {count} of your emails to {}", first.short_person())
            } else {
                format!("used {count} of your emails")
            });
        }
        parts.push(
            if self.history_used {
                "history used"
            } else {
                "history not used"
            }
            .to_string(),
        );
        if let Some(rewrite) = &self.rewrite {
            parts.push(rewrite.label());
        }
        parts.join(" · ")
    }

    /// Each source under a heading, with the command that opens it, for
    /// terminal clients:
    /// "  2026-09-29 · you to Maya  mxr cat <message-id>".
    pub fn source_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (heading, sources) in [
            ("Your emails it matched the voice of:", &self.voice_examples),
            (
                "Messages it read from this conversation:",
                &self.conversation,
            ),
        ] {
            if sources.is_empty() {
                continue;
            }
            lines.push(heading.to_string());
            lines.extend(sources.iter().map(DraftSourceData::terminal_line));
        }
        lines
    }
}

impl DraftRewriteProvenanceData {
    /// "rewritten by local model gemma4", "rewrite attempted by cloud model
    /// gpt-4o-mini, not used", or why it was skipped.
    pub fn label(&self) -> String {
        let place = match self.locality {
            AiLocalityData::Local => "local",
            AiLocalityData::Cloud => "cloud",
        };
        let model = &self.model;
        match self.outcome {
            DraftRewriteOutcomeData::Applied => match &self.rejected_by {
                Some(later) => {
                    format!("rewritten by {place} model {model}, a later pass by {later} not used")
                }
                None => format!("rewritten by {place} model {model}"),
            },
            DraftRewriteOutcomeData::Rejected => {
                format!("rewrite attempted by {place} model {model}, not used")
            }
            DraftRewriteOutcomeData::Skipped => {
                format!("rewrite by {place} model {model} skipped to keep your history local")
            }
        }
    }
}

impl DraftSourceData {
    fn terminal_line(&self) -> String {
        let who = if self.from_me {
            format!("you to {}", self.short_person())
        } else {
            self.short_person()
        };
        format!(
            "  {} · {who}  mxr cat {}",
            self.date.format("%Y-%m-%d"),
            self.message_id
        )
    }

    /// The person's first name, else their address.
    pub fn short_person(&self) -> String {
        self.person_name
            .as_deref()
            .and_then(|name| name.split_whitespace().next())
            .map_or_else(|| self.person.clone(), str::to_string)
    }
}

/// "Local model gemma4" / "Cloud model gpt-4o-mini".
fn model_label(locality: AiLocalityData, model: &str) -> String {
    match locality {
        AiLocalityData::Local => format!("Local model {model}"),
        AiLocalityData::Cloud => format!("Cloud model {model}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(person: &str, name: Option<&str>) -> DraftSourceData {
        DraftSourceData {
            message_id: MessageId::new(),
            thread_id: ThreadId::new(),
            date: chrono::Utc::now(),
            from_me: true,
            person: person.to_string(),
            person_name: name.map(str::to_string),
        }
    }

    fn provenance(voice_examples: Vec<DraftSourceData>) -> DraftProvenanceData {
        DraftProvenanceData {
            model: "gemma4".to_string(),
            locality: AiLocalityData::Local,
            history_used: true,
            voice_examples,
            conversation: Vec::new(),
            rewrite: None,
        }
    }

    #[test]
    fn the_line_names_the_person_when_every_example_was_to_them() {
        let line = provenance(vec![
            source("maya@x.com", Some("Maya Chen")),
            source("MAYA@x.com", Some("Maya Chen")),
        ])
        .summary_line();
        assert_eq!(
            line,
            "Local model gemma4 · used 2 of your emails to Maya · history used"
        );
    }

    #[test]
    fn mixed_recipients_and_a_rewrite_are_said_plainly() {
        let mut data = provenance(vec![source("maya@x.com", None), source("sam@x.com", None)]);
        data.rewrite = Some(DraftRewriteProvenanceData {
            model: "gpt-4o-mini".to_string(),
            locality: AiLocalityData::Cloud,
            history_used: false,
            outcome: DraftRewriteOutcomeData::Applied,
            rejected_by: None,
        });
        assert_eq!(
            data.summary_line(),
            "Local model gemma4 · used 2 of your emails · history used · rewritten by cloud model gpt-4o-mini"
        );
    }

    #[test]
    fn no_examples_and_no_history_is_said_too() {
        let mut data = provenance(Vec::new());
        data.locality = AiLocalityData::Cloud;
        data.history_used = false;
        assert_eq!(data.summary_line(), "Cloud model gemma4 · history not used");
    }

    #[test]
    fn a_source_without_a_name_falls_back_to_the_address() {
        let data: DraftSourceData = serde_json::from_value(serde_json::json!({
            "message_id": MessageId::new(),
            "thread_id": ThreadId::new(),
            "date": "2026-09-30T10:00:00Z",
            "from_me": false,
            "person": "maya@x.com"
        }))
        .expect("a source without a name parses");
        assert_eq!(data.short_person(), "maya@x.com");
    }

    #[test]
    fn a_rewrite_that_was_not_used_or_not_run_says_so() {
        let mut data = provenance(Vec::new());
        let mut rewrite = DraftRewriteProvenanceData {
            model: "gpt-4o-mini".to_string(),
            locality: AiLocalityData::Cloud,
            history_used: false,
            outcome: DraftRewriteOutcomeData::Rejected,
            rejected_by: None,
        };
        data.rewrite = Some(rewrite.clone());
        assert!(data
            .summary_line()
            .ends_with("rewrite attempted by cloud model gpt-4o-mini, not used"));
        rewrite.outcome = DraftRewriteOutcomeData::Skipped;
        data.rewrite = Some(rewrite);
        assert!(data
            .summary_line()
            .ends_with("rewrite by cloud model gpt-4o-mini skipped to keep your history local"));
    }

    #[test]
    fn a_draft_from_an_older_daemon_has_no_provenance() {
        let data: crate::ResponseData = serde_json::from_value(serde_json::json!({
            "kind": "DraftSuggestion",
            "body": "Friday works.",
            "model": "gemma4"
        }))
        .expect("a draft without provenance parses");
        assert!(matches!(
            data,
            crate::ResponseData::DraftSuggestion {
                provenance: None,
                ..
            }
        ));
    }
}
