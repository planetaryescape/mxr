use crate::ui::compose_picker::ComposePicker;
use mxr_core::id::AccountId;
use mxr_core::types::InlineCalendarReply;
use mxr_core::{DraftIntent, MessageId};
use mxr_protocol::{CalendarInviteActionData, ReplyContext};
use std::collections::HashMap;

/// Draft waiting for user confirmation after editor closes.
pub struct PendingSend {
    pub account_id: AccountId,
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

impl PendingSend {
    /// True iff the latest safety check verdict is Blocked. Drives
    /// the `[s] send` gate and the visibility of the `[Ctrl-O]
    /// override` affordance in the modal.
    pub fn is_blocked(&self) -> bool {
        matches!(
            self.safety_report.as_ref().map(|r| r.verdict),
            Some(mxr_core::DraftSafetyVerdict::Blocked)
        )
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
    pub pending_draft_cleanup: Vec<std::path::PathBuf>,
    pub pending_compose: Option<ComposeAction>,
    pub pending_send_confirm: Option<PendingSend>,
    pub pending_send_at_input: Option<String>,
    pub pending_remind_at_input: Option<String>,
    pub compose_picker: ComposePicker,
    /// Reply contexts prewarmed when the user opens a message, so
    /// pressing `r`/`a` opens the editor without waiting on the daemon.
    /// Bodies are immutable post-sync, so no invalidation is needed.
    pub reply_context_cache: HashMap<MessageId, ReplyContextPair>,
    /// The message we last kicked off a prewarm for. Prevents firing
    /// duplicate prewarm tasks every tick while the viewing envelope
    /// stays the same.
    pub last_prewarmed_message_id: Option<MessageId>,
    /// Reply/reply-all the user invoked while a prewarm was mid-flight
    /// for the same message. The `ReplyContextWarmed` handler drains
    /// this into `pending_compose` once the cache fills.
    pub deferred_compose: Option<DeferredCompose>,
}
