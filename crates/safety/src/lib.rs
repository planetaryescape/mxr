//! Deterministic pre-send safety checks for mxr drafts.
//!
//! Six checks per `docs/ai-email/01-pre-send-safety.md`:
//! - missing attachment regex (`attachment`)
//! - PII / secrets detector (`pii`)
//! - reply-all heuristic (`reply_all`)
//! - recipient history / domain checks (`recipients`)
//! - tone mismatch using `mxr-relationship` stylometry (`tone`)
//!
//! Answer-coverage is LLM-backed and lives in `answer_coverage` (Slice 1.4).
//!
//! The crate produces `mxr_core::types::DraftSafetyReport`; daemon, CLI, and
//! TUI all consume the same shape.

pub mod attachment;
pub mod commitments;
pub mod config;
pub mod pii;
pub mod recipients;
pub mod reply_all;
pub mod tone;

pub use config::{SafetyConfig, SafetyRecipientConfig, SafetyToneConfig};

pub use mxr_core::types::{
    CitationRef, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetyReport, DraftSafetySeverity,
    DraftSafetyVerdict,
};

use mxr_core::types::Draft;

/// Mode in which the safety pipeline is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyMode {
    /// User invoked `mxr send --check`. No persistence.
    Check,
    /// Pre-send gate from `SendDraft` / `SendStoredDraft`.
    Send,
    /// Scheduled-send flusher.
    ScheduledFlush,
}

/// Known contact summary used for typo detection, first-time-external, and
/// tone baseline lookups. Loaded by the caller from `mxr_store::contacts`.
#[derive(Debug, Clone)]
pub struct KnownContact {
    pub email: String,
    pub display_name: Option<String>,
    pub total_inbound: u64,
    pub total_outbound: u64,
}

impl KnownContact {
    pub fn is_strong(&self) -> bool {
        // a contact we have meaningful prior signal on
        self.total_inbound + self.total_outbound >= 3
    }
}

/// Lightweight contact-style baseline used for tone checks. Produced by
/// the caller from existing `mxr-relationship` stylometry.
#[derive(Debug, Clone)]
pub struct ContactStyleBaseline {
    pub email: String,
    pub baseline: mxr_relationship::StylometryMetrics,
    pub baseline_sample_count: u32,
}

/// Context for context-dependent checks (reply-all, recipients, tone).
/// Sync; daemon does the I/O once and passes the result.
#[derive(Debug, Clone, Default)]
pub struct SafetyContext {
    pub mode_reply_all: bool,
    pub self_addresses: Vec<String>,
    pub known_contacts: Vec<KnownContact>,
    pub contact_styles: Vec<ContactStyleBaseline>,
    /// Display-name vocatives that should be ignored in reply-all body
    /// inspection because they appear in quoted context.
    pub thread_display_names: Vec<String>,
}

/// Run every deterministic safety check. Sync; no I/O. The caller must
/// have built `SafetyContext` from store data.
pub fn check_draft_deterministic(
    draft: &Draft,
    ctx: &SafetyContext,
    config: &SafetyConfig,
) -> DraftSafetyReport {
    let mut issues = Vec::new();
    issues.extend(attachment::check(draft));
    issues.extend(pii::check(draft));
    if ctx.mode_reply_all {
        issues.extend(reply_all::check(draft, ctx));
    }
    issues.extend(recipients::check(draft, ctx, &config.recipients));
    issues.extend(tone::check(draft, ctx, &config.tone));
    DraftSafetyReport::from_issues(issues)
}
