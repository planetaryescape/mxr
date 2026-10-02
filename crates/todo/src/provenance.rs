//! Where each field of a to-do came from. Stored as JSON in
//! `todos.field_sources` and shown by `mxr todo why`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    /// schema.org markup the sender put in the email.
    Schema,
    /// The calendar invite (ICS) attached to the email.
    Ics,
    /// A pattern in the subject or body.
    Rule,
    /// The fixed lead-time or window table for the kind.
    Table,
    /// A model, such as the one that found a promise.
    Model,
    /// You.
    User,
}

impl FieldSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Ics => "ics",
            Self::Rule => "rule",
            Self::Table => "table",
            Self::Model => "model",
            Self::User => "user",
        }
    }

    /// "schema.org markup", for sentences.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Schema => "schema.org markup in the email",
            Self::Ics => "the calendar invite",
            Self::Rule => "a pattern in the email",
            Self::Table => "the lead-time table",
            Self::Model => "a model",
            Self::User => "you",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldProvenance {
    pub source: FieldSource,
    /// False when the value is a guess the user should confirm, such as a
    /// numeric date that reads differently in UK and US order.
    #[serde(default = "checked_default", skip_serializing_if = "is_true")]
    pub checked: bool,
    /// The words the value was read from, verbatim, or how it was worked
    /// out ("due minus 3 working days").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

fn checked_default() -> bool {
    true
}

// serde's skip_serializing_if passes a reference.
fn is_true(value: &bool) -> bool {
    *value
}

impl FieldProvenance {
    pub fn new(source: FieldSource) -> Self {
        Self {
            source,
            checked: true,
            evidence: None,
        }
    }

    pub fn with_evidence(source: FieldSource, evidence: impl Into<String>) -> Self {
        Self {
            source,
            checked: true,
            evidence: Some(evidence.into()),
        }
    }

    #[must_use]
    pub fn unchecked(mut self) -> Self {
        self.checked = false;
        self
    }
}

/// Field name to provenance, in a stable order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldSources(pub BTreeMap<String, FieldProvenance>);

impl FieldSources {
    pub fn set(&mut self, field: &str, provenance: FieldProvenance) {
        self.0.insert(field.to_string(), provenance);
    }

    pub fn get(&self, field: &str) -> Option<&FieldProvenance> {
        self.0.get(field)
    }

    pub fn any_user(&self) -> bool {
        self.0
            .values()
            .any(|provenance| provenance.source == FieldSource::User)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Unreadable JSON reads as no provenance rather than failing a list.
    pub fn from_json(value: &str) -> Self {
        serde_json::from_str(value).unwrap_or_default()
    }
}
