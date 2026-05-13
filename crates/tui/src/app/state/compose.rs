use crate::ui::compose_picker::ComposePicker;
use mxr_core::id::AccountId;
use mxr_core::DraftIntent;

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
    /// Single-use override token minted when the latest check
    /// returned a Blocked verdict; consumed when the user confirms.
    pub override_token: Option<String>,
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
    },
    ReplyAll {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
    },
    Forward {
        message_id: mxr_core::MessageId,
        account_id: AccountId,
    },
}

#[derive(Default)]
pub struct ComposeState {
    pub pending_draft_cleanup: Vec<std::path::PathBuf>,
    pub pending_compose: Option<ComposeAction>,
    pub pending_send_confirm: Option<PendingSend>,
    pub pending_send_at_input: Option<String>,
    pub compose_picker: ComposePicker,
}
