use crate::ui::compose_picker::ComposePicker;
use mxr_core::id::AccountId;
use mxr_core::types::InlineCalendarReply;
use mxr_core::{DraftIntent, MessageId};
use mxr_protocol::{CalendarInviteActionData, ReplyContext};
use std::collections::HashMap;

/// Draft waiting for user confirmation after editor closes.
#[derive(Debug, Clone)]
pub struct PendingSend {
    pub account_id: AccountId,
    /// Canonical draft revision this editor session was based on.
    pub draft_revision: Option<i64>,
    pub fm: mxr_compose::frontmatter::ComposeFrontmatter,
    pub body: String,
    pub draft_path: std::path::PathBuf,
    pub intent: DraftIntent,
    pub mode: PendingSendMode,
    /// Latest safety report, if a pre-send check has run.
    pub safety_report: Option<mxr_core::DraftSafetyReport>,
    /// Why the pre-send safety check produced no report (daemon error,
    /// dropped IPC worker, unexpected response). Rendered in the send
    /// modal as "Safety check unavailable: …" so the failure isn't
    /// silently indistinguishable from a clean run.
    pub safety_check_failed: Option<String>,
    /// Single-use override token minted when the latest check
    /// returned a Blocked verdict; consumed when the user confirms.
    pub override_token: Option<String>,
    /// Slice 5.3 (C2.7 cont): "maybe include" suggestions returned
    /// by `Request::SuggestCollaborators`. Rendered in the modal as
    /// a discoverable list; the user can press [Ctrl-A] to add the
    /// first one to Cc.
    pub suggested_collaborators: Vec<mxr_protocol::SuggestedRecipientData>,
    /// Populated for the invite-reply-with-comment compose path. When set,
    /// the resulting `Draft` carries the inline iCal REPLY payload so the
    /// outbound builder emits the proper `multipart/alternative` MIME layout
    /// and the daemon's post-send hook updates the local PARTSTAT.
    pub invite_reply: Option<InlineCalendarReply>,
    /// The effective From resolved by the daemon (`Request::ResolveSendFrom`)
    /// before the modal opens: the owned override, or the account primary when
    /// `from:` was cleared. Shown in the confirm modal so the user sees exactly
    /// which identity the message will send from. `None` only when the resolve
    /// couldn't run (daemon unreachable), in which case the modal falls back to
    /// the raw `from:` text.
    pub resolved_from: Option<mxr_core::types::Address>,
}

#[derive(Debug, Clone)]
pub struct ComposeAssistReview {
    pub pending: PendingSend,
    /// Proposals for this unchanged draft baseline, oldest first.
    pub proposals: Vec<mxr_protocol::ComposeProposalData>,
    pub proposal_models: Vec<Option<String>>,
    pub current_index: usize,
    pub note_input: Option<String>,
    pub proof_scroll_offset: u16,
    pub voice_source_index: usize,
    pub request_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeModelPickerPurpose {
    InitialDraft,
    RetryProof,
}

#[derive(Debug, Clone)]
pub struct ComposeModelPicker {
    pub choices: mxr_protocol::AssistantModelsData,
    pub selected_index: usize,
    pub pending: Box<PendingSend>,
    pub purpose: ComposeModelPickerPurpose,
}

impl ComposeAssistReview {
    pub fn proposal(&self) -> Option<&mxr_protocol::ComposeProposalData> {
        self.proposals.get(self.current_index)
    }

    pub fn select_version(&mut self, index: usize) {
        if index < self.proposals.len() {
            self.current_index = index;
            self.proof_scroll_offset = 0;
            self.voice_source_index = 0;
        }
    }

    pub fn push_proposal(
        &mut self,
        proposal: mxr_protocol::ComposeProposalData,
        model: Option<String>,
    ) {
        self.proposals.push(proposal);
        self.proposal_models.push(model);
        self.current_index = self.proposals.len().saturating_sub(1);
        self.note_input = None;
        self.voice_source_index = 0;
        self.proof_scroll_offset = 0;
        self.request_pending = false;
    }
}

impl PendingSend {
    pub(crate) fn draft_id(&self) -> mxr_core::DraftId {
        if let Some(source) = &self.fm.forward_source {
            return source.draft_id.clone();
        }
        mxr_core::DraftId::from_provider_id("compose-file", &self.draft_path.to_string_lossy())
    }

    /// True iff the latest safety check verdict is Blocked. Drives
    /// the `[s] send` gate and the visibility of the `[O]
    /// override` affordance in the modal.
    pub fn is_blocked(&self) -> bool {
        matches!(
            self.safety_report.as_ref().map(|r| r.verdict),
            Some(mxr_core::DraftSafetyVerdict::Blocked)
        )
    }

    /// The send review's lit option, which a bare Enter runs: Send, unless
    /// the draft can't go as it is or the safety check found something (or
    /// couldn't run), where it is Edit again. The web's send confirm opens
    /// only in that second case, and on "Back to draft".
    pub fn opens_on_send(&self) -> bool {
        self.mode == PendingSendMode::SendOrSave
            && self.safety_check_failed.is_none()
            && self
                .safety_report
                .as_ref()
                .is_none_or(|report| report.allowed && report.issues.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingSendMode {
    SendOrSave,
    DraftOnlyNoRecipients,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAction {
    New {
        to: String,
        subject: String,
    },
    EditDraft {
        path: std::path::PathBuf,
        account_id: AccountId,
    },
    Reply {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
        /// Prewarmed reply context. When `Some`, `handle_compose_action`
        /// skips the daemon `PrepareReply` IPC and uses this directly —
        /// the "blazingly fast" path. Populated from `reply_context_cache`
        /// at action-apply time when a prewarm has completed.
        preloaded: Option<ReplyContext>,
    },
    ReplyAll {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
        preloaded: Option<ReplyContext>,
    },
    Forward {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
    },
    /// Compose a comment on an iCal invite REPLY. `handle_compose_action`
    /// dispatches `Request::PrepareInviteResponse`, gets the daemon-built
    /// preview, and pre-fills To, Subject, and the inline
    /// `text/calendar; method=REPLY` part. The user types a free-text comment
    /// which becomes the `text/plain` alternative.
    InviteReplyWithComment {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
        action: CalendarInviteActionData,
    },
}

/// Cached reply contexts for a single message. Filled by the prewarm
/// task that fires when a message becomes the viewing envelope.
#[derive(Debug, Clone)]
pub struct ReplyContextPair {
    pub reply: Option<ReplyContext>,
    pub reply_all: Option<ReplyContext>,
}

/// A compose action that the user invoked while a prewarm IPC for the
/// same message was still in flight. We park it here instead of firing
/// a duplicate `PrepareReply` IPC — the worker is serial and a duplicate
/// would queue *behind* the in-flight prewarm, doubling the wait. When
/// `ReplyContextWarmed` lands, we drain this into `pending_compose` with
/// the freshly-cached context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredCompose {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub reply_all: bool,
}

#[derive(Default)]
pub struct ComposeState {
    pub workspace_owner: Option<mxr_protocol::ArchiveWorkspaceData>,
    pub failed_send: Option<PendingSend>,
    pub pending_schedule_files: std::collections::HashSet<std::path::PathBuf>,
    pub last_send_query: Option<String>,
    pub pending_send_mutations: HashMap<crate::app::MutationId, (PendingSend, bool)>,
    pub last_send_identity: Option<(AccountId, mxr_core::SendIdentity)>,
    pub send_recovery_preview: Option<(AccountId, String, String)>,
    pub pending_draft_cleanup: Vec<std::path::PathBuf>,
    pub active_draft_cleanup: std::collections::HashSet<std::path::PathBuf>,
    pub pending_compose: Option<ComposeAction>,
    pub pending_send_confirm: Option<PendingSend>,
    pub assist_review: Option<ComposeAssistReview>,
    pub viewing_voice_source: bool,
    pub voice_source_request_id: u64,
    pub assist_request_id: u64,
    pub assist_after_editor: bool,
    pub model_picker: Option<ComposeModelPicker>,
    pub pending_send_at_input: Option<String>,
    pub pending_remind_at_input: Option<String>,
    /// The send-at / remind prompt's previewed time and selected reading.
    pub pending_time: crate::ui::time_preview::TimePreview,
    pub compose_picker: ComposePicker,
    /// Reply contexts prewarmed when the user opens a message, so
    /// pressing `r`/`a` opens the editor without waiting on the daemon.
    /// Dropped when the owner's addresses change (`invalidate_reply_contexts`);
    /// the recipients a reply offers depend on them.
    pub reply_context_cache: HashMap<MessageId, ReplyContextPair>,
    /// Bumped when the owned addresses change. A prewarm carries the generation
    /// it started under, and its result is dropped when that generation is old.
    pub reply_context_generation: u64,
    /// The message we last kicked off a prewarm for. Prevents firing
    /// duplicate prewarm tasks every tick while the viewing envelope
    /// stays the same.
    pub last_prewarmed_message_id: Option<MessageId>,
    /// Reply/reply-all the user invoked while a prewarm was mid-flight
    /// for the same message. The `ReplyContextWarmed` handler drains
    /// this into `pending_compose` once the cache fills.
    pub deferred_compose: Option<DeferredCompose>,
}

impl ComposeState {
    /// The owner's addresses changed, so every cached reply context may
    /// name the wrong recipients. Prewarms still in flight are invalidated by
    /// the generation bump.
    pub fn invalidate_reply_contexts(&mut self) {
        self.reply_context_cache.clear();
        self.last_prewarmed_message_id = None;
        self.reply_context_generation = self.reply_context_generation.wrapping_add(1);
    }

    /// Cache a prewarm result, unless it started under an older generation.
    /// Returns whether it was cached.
    pub fn cache_warmed_reply_context(
        &mut self,
        generation: u64,
        message_id: MessageId,
        pair: ReplyContextPair,
    ) -> bool {
        if generation != self.reply_context_generation {
            return false;
        }
        self.reply_context_cache.insert(message_id, pair);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_pair() -> ReplyContextPair {
        ReplyContextPair {
            reply: None,
            reply_all: None,
        }
    }

    #[test]
    fn a_prewarm_started_before_an_address_change_is_not_cached() {
        let mut compose = ComposeState::default();
        let message = MessageId::new();

        let started = compose.reply_context_generation;
        compose.invalidate_reply_contexts();
        assert!(!compose.cache_warmed_reply_context(started, message.clone(), empty_pair()));
        assert!(!compose.reply_context_cache.contains_key(&message));

        let started_after = compose.reply_context_generation;
        assert!(compose.cache_warmed_reply_context(started_after, message.clone(), empty_pair()));
        assert!(compose.reply_context_cache.contains_key(&message));
    }
}
