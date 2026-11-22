use crate::ui::compose_picker::ComposePicker;
use mxr_core::id::AccountId;
use mxr_core::types::InlineCalendarReply;
use mxr_core::{DraftIntent, MessageId};
use mxr_protocol::{CalendarInviteActionData, ReplyContext};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PendingSend {
    pub account_id: AccountId,
    pub draft_revision: Option<i64>,
    pub fm: mxr_compose::frontmatter::ComposeFrontmatter,
    pub body: String,
    pub draft_path: std::path::PathBuf,
    pub intent: DraftIntent,
    pub mode: PendingSendMode,
    pub safety_report: Option<mxr_core::DraftSafetyReport>,
    pub safety_check_failed: Option<String>,
    pub override_token: Option<String>,
    pub suggested_collaborators: Vec<mxr_protocol::SuggestedRecipientData>,
    pub invite_reply: Option<InlineCalendarReply>,
    pub resolved_from: Option<mxr_core::types::Address>,
    pub stored_draft: Option<Box<mxr_core::Draft>>,
}

#[derive(Debug, Clone)]
pub struct ComposeAssistReview {
    pub pending: PendingSend,
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
        if let Some(stored) = &self.stored_draft {
            return stored.id.clone();
        }
        if let Some(source) = &self.fm.forward_source {
            return source.draft_id.clone();
        }
        mxr_core::DraftId::from_provider_id("compose-file", &self.draft_path.to_string_lossy())
    }

    pub fn is_blocked(&self) -> bool {
        matches!(
            self.safety_report.as_ref().map(|r| r.verdict),
            Some(mxr_core::DraftSafetyVerdict::Blocked)
        )
    }

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
pub struct ReplyDraftResume {
    pub message_id: mxr_core::MessageId,
    pub reply_all: bool,
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
    InviteReplyWithComment {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
        action: CalendarInviteActionData,
    },
}

#[derive(Debug, Clone)]
pub struct ReplyContextPair {
    pub reply: Option<ReplyContext>,
    pub reply_all: Option<ReplyContext>,
}

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
    pub remind_after_editor: bool,
    pub model_picker: Option<ComposeModelPicker>,
    pub pending_send_at_input: Option<String>,
    pub pending_remind_at_input: Option<String>,
    pub pending_time: crate::ui::time_preview::TimePreview,
    pub compose_picker: ComposePicker,
    pub reply_context_cache: HashMap<MessageId, ReplyContextPair>,
    pub reply_context_generation: u64,
    pub last_prewarmed_message_id: Option<MessageId>,
    pub deferred_compose: Option<DeferredCompose>,
}

impl ComposeState {
    pub fn invalidate_reply_contexts(&mut self) {
        self.reply_context_cache.clear();
        self.last_prewarmed_message_id = None;
        self.reply_context_generation = self.reply_context_generation.wrapping_add(1);
    }

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
