use crate::ui;
use mxr_config::RenderConfig;
use mxr_core::id::{AttachmentId, MessageId};
use mxr_core::types::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use throbber_widgets_tui::ThrobberState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingBrowserOpen {
    pub message_id: MessageId,
    pub document: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Sidebar,
    MailList,
    MessageView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailListMode {
    Threads,
    Messages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxView {
    Messages,
    Subscriptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Labels,
    SavedSearches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    TwoPane,
    ThreePane,
    FullScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource {
    Plain,
    Html,
    Fallback,
    Snippet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyViewMode {
    #[default]
    Text,
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyViewMetadata {
    pub mode: BodyViewMode,
    pub provenance: Option<BodyPartSource>,
    pub reader_applied: bool,
    pub flowed: bool,
    pub inline_images: bool,
    pub remote_content_available: bool,
    pub remote_content_enabled: bool,
    pub original_lines: Option<usize>,
    pub cleaned_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyViewState {
    Loading {
        preview: Option<String>,
    },
    Ready {
        raw: String,
        rendered: String,
        source: BodySource,
        metadata: BodyViewMetadata,
    },
    Empty {
        preview: Option<String>,
    },
    Error {
        message: String,
        preview: Option<String>,
    },
}

impl BodyViewState {
    pub fn display_text(&self) -> Option<&str> {
        match self {
            Self::Ready { rendered, .. } => Some(rendered.as_str()),
            Self::Loading { preview } => preview.as_deref(),
            Self::Empty { preview } => preview.as_deref(),
            Self::Error { preview, .. } => preview.as_deref(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MailListRow {
    pub thread_id: mxr_core::ThreadId,
    pub representative: Envelope,
    pub message_count: usize,
    pub unread_count: usize,
}

#[derive(Debug, Clone)]
pub struct SubscriptionEntry {
    pub summary: SubscriptionSummary,
    pub envelope: Envelope,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum SidebarItem {
    Account(mxr_protocol::AccountSummaryData),
    AllMail,
    Subscriptions,
    Label(Label),
    SavedSearch(mxr_core::SavedSearch),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SidebarSelectionKey {
    Account(String),
    AllMail,
    Subscriptions,
    Label(mxr_core::LabelId),
    SavedSearch(String),
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionsPageState {
    pub entries: Vec<SubscriptionEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentOperation {
    Open,
    Download,
}

#[derive(Debug, Clone, Default)]
pub struct AttachmentPanelState {
    pub visible: bool,
    pub message_id: Option<MessageId>,
    pub attachments: Vec<AttachmentMeta>,
    pub selected_index: usize,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingAttachmentAction {
    pub message_id: MessageId,
    pub attachment_id: AttachmentId,
    pub operation: AttachmentOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSummary {
    pub filename: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub(in crate::app) struct PendingPreviewRead {
    pub message_id: MessageId,
    pub due_at: Instant,
}

pub struct MailboxState {
    pub envelopes: Vec<Envelope>,
    pub all_envelopes: Vec<Envelope>,
    pub mailbox_view: MailboxView,
    pub labels: Vec<Label>,
    pub mail_list_mode: MailListMode,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: ActivePane,
    pub layout_mode: LayoutMode,
    pub body_view_state: BodyViewState,
    pub viewing_envelope: Option<Envelope>,
    pub viewed_thread: Option<Thread>,
    pub viewed_thread_messages: Vec<Envelope>,
    pub thread_selected_index: usize,
    pub message_scroll_offset: u16,
    pub body_cache: HashMap<MessageId, MessageBody>,
    pub queued_body_fetches: Vec<MessageId>,
    pub in_flight_body_requests: HashSet<MessageId>,
    pub pending_thread_fetch: Option<mxr_core::ThreadId>,
    pub in_flight_thread_fetch: Option<mxr_core::ThreadId>,
    pub thread_request_id: u64,
    pub pending_browser_open: Option<PendingBrowserOpen>,
    pub pending_browser_open_after_load: Option<MessageId>,
    pub sidebar_selected: usize,
    pub sidebar_section: SidebarSection,
    pub saved_searches: Vec<mxr_core::SavedSearch>,
    pub subscriptions_page: SubscriptionsPageState,
    pub active_label: Option<mxr_core::LabelId>,
    pub pending_label_fetch: Option<mxr_core::LabelId>,
    pub pending_active_label: Option<mxr_core::LabelId>,
    pub pending_labels_refresh: bool,
    pub pending_all_envelopes_refresh: bool,
    pub pending_subscriptions_refresh: bool,
    pub desired_system_mailbox: Option<String>,
    pub mailbox_loading_message: Option<String>,
    pub mailbox_loading_throbber: ThrobberState,
    pub(in crate::app) pending_preview_read: Option<PendingPreviewRead>,
    pub reader_mode: bool,
    pub html_view: bool,
    pub render_html_command: Option<String>,
    pub show_reader_stats: bool,
    pub remote_content_enabled: bool,
    pub signature_expanded: bool,
    pub attachment_panel: AttachmentPanelState,
    pub pending_attachment_action: Option<PendingAttachmentAction>,
    pub selected_set: HashSet<MessageId>,
    pub visual_mode: bool,
    pub visual_anchor: Option<usize>,
    pub pending_export_thread: Option<mxr_core::id::ThreadId>,
    pub sidebar_accounts_expanded: bool,
    pub sidebar_system_expanded: bool,
    pub sidebar_user_expanded: bool,
    pub sidebar_saved_searches_expanded: bool,
    pub url_modal: Option<ui::url_modal::UrlModalState>,
}

impl Default for MailboxState {
    fn default() -> Self {
        Self::from_render_config(&RenderConfig::default())
    }
}

impl MailboxState {
    pub fn from_render_config(render: &RenderConfig) -> Self {
        Self {
            envelopes: Vec::new(),
            all_envelopes: Vec::new(),
            mailbox_view: MailboxView::Messages,
            labels: Vec::new(),
            mail_list_mode: MailListMode::Threads,
            selected_index: 0,
            scroll_offset: 0,
            active_pane: ActivePane::MailList,
            layout_mode: LayoutMode::TwoPane,
            body_view_state: BodyViewState::Empty { preview: None },
            viewing_envelope: None,
            viewed_thread: None,
            viewed_thread_messages: Vec::new(),
            thread_selected_index: 0,
            message_scroll_offset: 0,
            body_cache: HashMap::new(),
            queued_body_fetches: Vec::new(),
            in_flight_body_requests: HashSet::new(),
            pending_thread_fetch: None,
            in_flight_thread_fetch: None,
            thread_request_id: 0,
            pending_browser_open: None,
            pending_browser_open_after_load: None,
            sidebar_selected: 0,
            sidebar_section: SidebarSection::Labels,
            saved_searches: Vec::new(),
            subscriptions_page: SubscriptionsPageState::default(),
            active_label: None,
            pending_label_fetch: None,
            pending_active_label: None,
            pending_labels_refresh: false,
            pending_all_envelopes_refresh: false,
            pending_subscriptions_refresh: false,
            desired_system_mailbox: None,
            mailbox_loading_message: None,
            mailbox_loading_throbber: ThrobberState::default(),
            pending_preview_read: None,
            reader_mode: render.reader_mode,
            html_view: true,
            render_html_command: render.html_command.clone(),
            show_reader_stats: render.show_reader_stats,
            remote_content_enabled: render.html_remote_content,
            signature_expanded: false,
            attachment_panel: AttachmentPanelState::default(),
            pending_attachment_action: None,
            selected_set: HashSet::new(),
            visual_mode: false,
            visual_anchor: None,
            pending_export_thread: None,
            sidebar_accounts_expanded: true,
            sidebar_system_expanded: true,
            sidebar_user_expanded: true,
            sidebar_saved_searches_expanded: true,
            url_modal: None,
        }
    }
}
