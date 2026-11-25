//! Background model work held back because its mail was backfilled.
//!
//! Background jobs send only mail that arrived live to the model; mail the
//! initial backfill downloaded reaches it only when the user asks. These
//! counts let `mxr llm status` say how much was held back since the daemon
//! started. Each input counts once however often a job meets it again.

use std::collections::HashSet;
use std::sync::Mutex;

/// A background job that holds backfilled mail back from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackfillHeldJob {
    /// A contact's relationship summary, held until mail with them arrives live.
    RelationshipSummary,
    /// Confirming a shortlisted delivery email.
    DeliveryExtraction,
    /// Checking whether a later email resolves an open to-do.
    TodoResolution,
}

impl BackfillHeldJob {
    /// Every job, in the order status lists them.
    pub const ALL: [Self; 3] = [
        Self::RelationshipSummary,
        Self::DeliveryExtraction,
        Self::TodoResolution,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::RelationshipSummary => "relationship_summary",
            Self::DeliveryExtraction => "delivery_extraction",
            Self::TodoResolution => "todo_resolution",
        }
    }

    const fn slot(self) -> usize {
        match self {
            Self::RelationshipSummary => 0,
            Self::DeliveryExtraction => 1,
            Self::TodoResolution => 2,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct BackfillHolds {
    inputs: [Mutex<HashSet<String>>; 3],
}

impl BackfillHolds {
    pub(crate) fn record(&self, job: BackfillHeldJob, input: &str) {
        let mut held = self.inputs[job.slot()]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !held.contains(input) {
            held.insert(input.to_string());
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<(BackfillHeldJob, u64)> {
        BackfillHeldJob::ALL
            .into_iter()
            .map(|job| {
                let held = self.inputs[job.slot()]
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (job, held.len() as u64)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_input_held_again_counts_once() {
        let holds = BackfillHolds::default();
        holds.record(BackfillHeldJob::DeliveryExtraction, "m1");
        holds.record(BackfillHeldJob::DeliveryExtraction, "m1");
        holds.record(BackfillHeldJob::DeliveryExtraction, "m2");
        holds.record(BackfillHeldJob::TodoResolution, "m1");
        assert_eq!(
            holds.snapshot(),
            vec![
                (BackfillHeldJob::RelationshipSummary, 0),
                (BackfillHeldJob::DeliveryExtraction, 2),
                (BackfillHeldJob::TodoResolution, 1),
            ]
        );
    }
}
