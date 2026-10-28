//! The label a search reply carries when it was answered from SQLite because
//! the lexical index was unavailable.
//!
//! The wording lives here, next to the wire type, so the CLI, TUI, web app and
//! the daemon's own refusals all say the same sentence.
use serde::{Deserialize, Serialize};

/// What the limited search covers, in the order the sentence names them.
pub const LIMITED_SEARCH_FIELDS: [&str; 3] = ["subject", "sender", "preview"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SearchDegradedReason {
    /// The index worker has stopped. Only a daemon restart brings it back.
    IndexStopped,
    /// The index is being wiped and rebuilt from SQLite.
    IndexRebuilding,
    /// The last rebuild failed or was interrupted, so the index holds only
    /// part of the mail until a rebuild finishes.
    IndexIncomplete,
}

impl SearchDegradedReason {
    fn headline(self) -> &'static str {
        match self {
            Self::IndexStopped => "Search is limited while the index is unavailable",
            Self::IndexRebuilding => "Search is limited while the index is rebuilding",
            Self::IndexIncomplete => "Search is limited because the last index rebuild failed",
        }
    }

    fn remedy(self) -> &'static str {
        match self {
            Self::IndexStopped => "Restart the daemon or run `mxr doctor --reindex`.",
            Self::IndexRebuilding => "Full search returns when the rebuild finishes.",
            Self::IndexIncomplete => {
                "Run `mxr doctor --reindex` or restart the daemon to retry the rebuild."
            }
        }
    }
}

/// Set on a search reply that came from the SQLite fallback instead of the
/// lexical index. Absent on a reply from the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchDegraded {
    pub reason: SearchDegradedReason,
    /// The parts of a message the limited search looked at.
    pub searched_fields: Vec<String>,
}

impl SearchDegraded {
    pub fn new(reason: SearchDegradedReason) -> Self {
        Self {
            reason,
            searched_fields: LIMITED_SEARCH_FIELDS
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }

    /// The one line every client shows above limited results.
    pub fn notice(&self) -> String {
        format!(
            "{}: {} only. {}",
            self.reason.headline(),
            natural_list(&self.searched_fields),
            self.reason.remedy()
        )
    }

    /// The reply to a request that has no limited form at all, such as a
    /// count: `what` is the thing that needs the full index.
    pub fn needs_full_index(reason: SearchDegradedReason, what: &str) -> String {
        let searched = natural_list(&Self::new(reason).searched_fields);
        format!(
            "{}: {searched} only. {what} needs the full index. {}",
            reason.headline(),
            reason.remedy()
        )
    }

    /// The reply to a query the limited search cannot answer without
    /// loosening it. `unsupported` names the parts of the query it cannot run.
    pub fn refusal(reason: SearchDegradedReason, unsupported: &[String]) -> String {
        let searched = natural_list(&Self::new(reason).searched_fields);
        format!(
            "{}: {searched} only. This query uses {}, which limited search cannot run. {}",
            reason.headline(),
            unsupported.join(", "),
            reason.remedy()
        )
    }
}

fn natural_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture serialization for direct failures"
    )]

    use super::*;

    #[test]
    fn notice_for_a_stopped_index_names_the_fields_and_the_remedy() {
        assert_eq!(
            SearchDegraded::new(SearchDegradedReason::IndexStopped).notice(),
            "Search is limited while the index is unavailable: subject, sender and preview only. Restart the daemon or run `mxr doctor --reindex`."
        );
    }

    #[test]
    fn refusal_names_the_unsupported_parts() {
        let message = SearchDegraded::refusal(
            SearchDegradedReason::IndexStopped,
            &["after:2024-01-01".to_string(), "is:unread".to_string()],
        );
        assert!(message.contains("after:2024-01-01, is:unread"), "{message}");
        assert!(message.contains("mxr doctor --reindex"), "{message}");
    }

    #[test]
    fn the_reason_is_a_snake_case_string_on_the_wire() {
        let json = serde_json::to_value(SearchDegraded::new(SearchDegradedReason::IndexRebuilding))
            .unwrap();
        assert_eq!(json["reason"], "index_rebuilding");
        assert_eq!(
            json["searched_fields"],
            serde_json::json!(["subject", "sender", "preview"])
        );
    }
}
