use crate::ui::compose_picker::ComposePicker;
use mxr_core::id::AccountId;

/// Draft waiting for user confirmation after editor closes.
pub struct PendingSend {
    pub account_id: AccountId,
    pub fm: mxr_compose::frontmatter::ComposeFrontmatter,
    pub body: String,
    pub draft_path: std::path::PathBuf,
    pub mode: PendingSendMode,
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
    pub compose_picker: ComposePicker,
}
