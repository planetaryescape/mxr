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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveStep {
    pub message_ids: Vec<MessageId>,
    pub thread_id: mxr_core::ThreadId,
    pub forward: bool,
    pub neighbour: Option<MessageId>,
    pub mutation: Option<crate::app::MutationId>,
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
    Owed,
    Desk,
    CalendarInvites,
    Place(mxr_protocol::MailPlaceData),
    Todo,
    Now,
    Reading,
    Updates,
    ArchiveMode,
    People,
    Focus,
    Snoozed,
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
    pub calendar: Option<CalendarMetadata>,
    pub original_lines: Option<usize>,
    pub cleaned_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyViewState {
    Loading {
        preview: Option<String>,
    },
    Ready {
        raw: Box<String>,
        rendered: Box<String>,
        source: BodySource,
        metadata: Box<BodyViewMetadata>,
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
    pub fn ready(
        raw: String,
        rendered: String,
        source: BodySource,
        metadata: BodyViewMetadata,
    ) -> Self {
        Self::Ready {
            raw: Box::new(raw),
            rendered: Box::new(rendered),
            source,
            metadata: Box::new(metadata),
        }
    }

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
    pub other_participant_count: usize,
    pub open_commitment_count: u32,
    pub triage_verdict: Option<String>,
    pub reply_later: bool,
    pub pending_mutation: bool,
    pub mode_chip: Option<&'static str>,
    pub wake_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionEntry {
    pub summary: SubscriptionSummary,
    pub envelope: Envelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarHeader {
    Accounts,
    More,
    Labels,
    SavedSearches,
}

#[derive(Debug, Clone)]
pub enum SidebarItem {
    Header(SidebarHeader),
    Account(Box<mxr_protocol::AccountSummaryData>),
    AllMail,
    Starred,
    Subscriptions,
    Now,
    Messages,
    Todo,
    Updates,
    Reading,
    ArchiveMode,
    Inbox,
    Screener,
    ReplyQueue,
    Waiting,
    Owed,
    CalendarInvites,
    Label(Box<Label>),
    SavedSearch(Box<mxr_core::SavedSearch>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SidebarSelectionKey {
    Header(SidebarHeader),
    Account(String),
    AllMail,
    Starred,
    Subscriptions,
    Now,
    Messages,
    Todo,
    Updates,
    Reading,
    ArchiveMode,
    Inbox,
    Screener,
    ReplyQueue,
    Waiting,
    Owed,
    CalendarInvites,
    Label(mxr_core::LabelId),
    SavedSearch(String),
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionsPageState {
    pub entries: Vec<SubscriptionEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct SnoozedPageState {
    pub envelopes: Vec<Envelope>,
    pub wake_at: HashMap<MessageId, chrono::DateTime<chrono::Utc>>,
    pub loaded: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OwedRepliesPageState {
    pub entries: Vec<mxr_protocol::OwedReplyRowData>,
    pub people: Option<mxr_protocol::PeopleSplitData>,
}

#[derive(Debug, Clone, Default)]
pub struct DeskPageState {
    pub lanes: Vec<(mxr_protocol::DeskLaneKind, mxr_protocol::DeskLaneData)>,
    pub elsewhere: mxr_protocol::DeskElsewhereData,
    pub loaded: bool,
    pub low_tide: bool,
}

impl DeskPageState {
    pub fn from_response(data: mxr_protocol::ResponseData) -> Option<Self> {
        use mxr_protocol::DeskLaneKind;
        let mxr_protocol::ResponseData::Desk {
            owed,
            due,
            waiting,
            people_new,
            elsewhere,
            ..
        } = data
        else {
            return None;
        };
        Some(Self {
            lanes: vec![
                (DeskLaneKind::Owed, owed),
                (DeskLaneKind::Due, due),
                (DeskLaneKind::Waiting, waiting),
                (DeskLaneKind::PeopleNew, people_new),
            ],
            elsewhere,
            loaded: true,
            low_tide: false,
        })
    }

    pub fn rows(&self) -> impl Iterator<Item = &mxr_protocol::DeskRowData> {
        self.lanes.iter().flat_map(|(_, lane)| lane.rows.iter())
    }

    pub fn row_count(&self) -> usize {
        self.lanes.iter().map(|(_, lane)| lane.rows.len()).sum()
    }

    pub fn remove_thread(&mut self, thread_id: &mxr_core::ThreadId) {
        for (_, lane) in &mut self.lanes {
            let before = lane.rows.len();
            lane.rows.retain(|row| &row.thread_id != thread_id);
            let removed = (before - lane.rows.len()) as u32;
            lane.total = lane.total.saturating_sub(removed);
        }
    }

    pub fn work_count(&self) -> usize {
        use mxr_protocol::DeskLaneKind;
        self.lanes
            .iter()
            .filter(|(kind, _)| matches!(kind, DeskLaneKind::Owed | DeskLaneKind::Due))
            .map(|(_, lane)| lane.total as usize)
            .sum()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlacePageState {
    pub place: Option<mxr_protocol::MailPlaceData>,
    pub bundles: Vec<mxr_protocol::PlaceBundleData>,
    pub total_bundles: u32,
    pub total_messages: u32,
    pub loaded: bool,
}

impl PlacePageState {
    pub fn from_response(data: mxr_protocol::ResponseData) -> Option<Self> {
        let mxr_protocol::ResponseData::Place {
            place,
            bundles,
            total_bundles,
            total_messages,
            ..
        } = data
        else {
            return None;
        };
        Some(Self {
            place: Some(place),
            bundles,
            total_bundles,
            total_messages,
            loaded: true,
        })
    }

    pub fn rows(
        &self,
    ) -> impl Iterator<
        Item = (
            &mxr_protocol::PlaceBundleData,
            &mxr_protocol::PlaceMessageData,
        ),
    > {
        self.bundles
            .iter()
            .flat_map(|bundle| bundle.messages.iter().map(move |message| (bundle, message)))
    }

    pub fn row_count(&self) -> usize {
        self.bundles
            .iter()
            .map(|bundle| bundle.messages.len())
            .sum()
    }

    pub fn remove_thread(&mut self, thread_id: &mxr_core::ThreadId) {
        for bundle in &mut self.bundles {
            bundle
                .messages
                .retain(|message| &message.thread_id != thread_id);
        }
        self.bundles.retain(|bundle| !bundle.messages.is_empty());
    }

    pub fn set_pinned(&mut self, message_id: &MessageId, pinned: bool) {
        for bundle in &mut self.bundles {
            let mut changed = false;
            for message in &mut bundle.messages {
                if &message.message_id == message_id && message.pinned != pinned {
                    message.pinned = pinned;
                    changed = true;
                }
            }
            if changed {
                bundle.pinned_count = if pinned {
                    bundle.pinned_count + 1
                } else {
                    bundle.pinned_count.saturating_sub(1)
                };
            }
        }
    }
}

pub const PLACE_PAGE_SENDERS: u32 = 50;
pub const PLACE_PAGE_MESSAGES: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceFetch {
    Refresh {
        place: mxr_protocol::MailPlaceData,
        senders: u32,
        messages_per_sender: u32,
    },
    MoreSenders {
        place: mxr_protocol::MailPlaceData,
        offset: u32,
    },
    MoreFromSender {
        place: mxr_protocol::MailPlaceData,
        account_id: mxr_core::AccountId,
        sender_email: String,
        message_offset: u32,
    },
}

impl PlacePageState {
    pub fn refresh_fetch(&self, place: mxr_protocol::MailPlaceData) -> PlaceFetch {
        let loaded = if self.place == Some(place) {
            self
        } else {
            &Self::default()
        };
        PlaceFetch::Refresh {
            place,
            senders: (loaded.bundles.len() as u32).max(PLACE_PAGE_SENDERS),
            messages_per_sender: loaded
                .bundles
                .iter()
                .map(|bundle| bundle.messages.len() as u32)
                .max()
                .unwrap_or(0)
                .max(PLACE_PAGE_MESSAGES),
        }
    }

    pub fn apply(&mut self, fetch: &PlaceFetch, page: Self) {
        match fetch {
            PlaceFetch::Refresh { .. } => *self = page,
            PlaceFetch::MoreSenders { .. } => {
                let known: HashSet<(mxr_core::AccountId, String)> = self
                    .bundles
                    .iter()
                    .map(|b| (b.account_id.clone(), b.sender_email.clone()))
                    .collect();
                self.bundles.extend(
                    page.bundles.into_iter().filter(|b| {
                        !known.contains(&(b.account_id.clone(), b.sender_email.clone()))
                    }),
                );
                self.total_bundles = page.total_bundles;
                self.total_messages = page.total_messages;
            }
            PlaceFetch::MoreFromSender {
                account_id,
                sender_email,
                ..
            } => {
                let Some(fresh) = page.bundles.into_iter().next() else {
                    return;
                };
                if let Some(bundle) = self
                    .bundles
                    .iter_mut()
                    .find(|b| &b.account_id == account_id && &b.sender_email == sender_email)
                {
                    let known: HashSet<MessageId> = bundle
                        .messages
                        .iter()
                        .map(|m| m.message_id.clone())
                        .collect();
                    bundle.messages.extend(
                        fresh
                            .messages
                            .into_iter()
                            .filter(|m| !known.contains(&m.message_id)),
                    );
                    bundle.message_count = fresh.message_count;
                }
            }
        }
    }

    pub fn has_more_senders(&self) -> bool {
        (self.bundles.len() as u32) < self.total_bundles
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepTarget {
    pub place: mxr_protocol::MailPlaceData,
    pub account_id: Option<mxr_core::AccountId>,
    pub sender_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSweepConfirm {
    pub target: SweepTarget,
    pub preview: mxr_protocol::SweepPreviewData,
    pub shown: u32,
    pub sweep_focused: bool,
    pub from_updates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderKindMenu {
    pub account_id: mxr_core::AccountId,
    pub sender_email: String,
    pub display: String,
    pub current: mxr_protocol::MailKindData,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct CalendarInvitesPageState {
    pub entries: Vec<mxr_protocol::CalendarInviteData>,
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
    pub filename: String,
    pub operation: AttachmentOperation,
    pub destination: Option<std::path::PathBuf>,
    pub replace_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSummary {
    pub filename: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummaryPreview {
    pub text: String,
    pub model: String,
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
    pub thread_summary: Option<ThreadSummaryPreview>,
    pub thread_summary_in_flight: HashSet<mxr_core::ThreadId>,
    pub thread_summary_error: Option<String>,
    pub thread_context: Option<mxr_protocol::ThreadContextData>,
    pub thread_gist: Option<mxr_protocol::ThreadGistData>,
    pub thread_selected_index: usize,
    pub message_scroll_offset: u16,
    pub body_cache: HashMap<MessageId, MessageBody>,
    pub priority_body_fetches: Vec<MessageId>,
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
    pub saved_search_unread_counts: HashMap<mxr_core::id::SavedSearchId, u32>,
    pub subscriptions_page: SubscriptionsPageState,
    pub owed_page: OwedRepliesPageState,
    pub snoozed_page: SnoozedPageState,
    pub desk_page: DeskPageState,
    pub todo_page: super::TodoPageState,
    pub records_page: super::RecordsPageState,
    pub now_page: super::NowPageState,
    pub trust: super::TrustState,
    pub messages_page: super::MessagesPageState,
    pub reading_page: super::ReadingPageState,
    pub updates_page: super::UpdatesPageState,
    pub focus_page: super::FocusPageState,
    pub rail: Option<mxr_protocol::RailData>,
    pub pending_rail_refresh: bool,
    pub place_page: PlacePageState,
    pub calendar_invites_page: CalendarInvitesPageState,
    pub active_label: Option<mxr_core::LabelId>,
    pub pending_label_fetch: Option<mxr_core::LabelId>,
    pub pending_active_label: Option<mxr_core::LabelId>,
    pub active_role: Option<mxr_core::Role>,
    pub pending_role_fetch: Option<mxr_core::Role>,
    pub pending_labels_refresh: bool,
    pub pending_all_envelopes_refresh: bool,
    pub pending_subscriptions_refresh: bool,
    pub pending_owed_refresh: bool,
    pub pending_snoozed_refresh: bool,
    pub pending_desk_refresh: bool,
    pub pending_place_refresh: Option<mxr_protocol::MailPlaceData>,
    pub pending_place_more: Option<PlaceFetch>,
    pub pending_sweep_preview: Option<SweepTarget>,
    pub pending_sweep: Option<PendingSweepConfirm>,
    pub sweep_confirm: Option<PendingSweepConfirm>,
    pub sender_kind_menu: Option<SenderKindMenu>,
    pub pending_calendar_invites_refresh: bool,
    pub pending_invite_open: Option<MessageId>,
    pub todo_link: Option<(MessageId, String)>,
    pub pending_commitment_counts_refresh: bool,
    pub open_commitment_counts: HashMap<(mxr_core::AccountId, mxr_core::ThreadId), u32>,
    pub reply_later_message_ids: HashSet<MessageId>,
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
    pub quotes_expanded: bool,
    pub collapsed_messages: HashSet<MessageId>,
    pub scroll_to_focused_message: bool,
    pub archive_step: Option<ArchiveStep>,
    pub attachment_panel: AttachmentPanelState,
    pub pending_attachment_action: Option<PendingAttachmentAction>,
    pub selected_set: HashSet<MessageId>,
    pub visual_mode: bool,
    pub visual_anchor: Option<usize>,
    pub visual_count: Option<usize>,
    pub pending_export: Option<super::export_modal::PendingExport>,
    pub sidebar_accounts_expanded: bool,
    pub sidebar_sync_throbber: ThrobberState,
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
            thread_summary: None,
            thread_summary_in_flight: HashSet::new(),
            thread_summary_error: None,
            thread_context: None,
            thread_gist: None,
            thread_selected_index: 0,
            message_scroll_offset: 0,
            body_cache: HashMap::new(),
            priority_body_fetches: Vec::new(),
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
            saved_search_unread_counts: HashMap::new(),
            subscriptions_page: SubscriptionsPageState::default(),
            owed_page: OwedRepliesPageState::default(),
            snoozed_page: SnoozedPageState::default(),
            desk_page: DeskPageState::default(),
            todo_page: super::TodoPageState::default(),
            records_page: super::RecordsPageState::default(),
            now_page: super::NowPageState::default(),
            trust: super::TrustState::default(),
            messages_page: super::MessagesPageState::default(),
            reading_page: super::ReadingPageState::default(),
            updates_page: super::UpdatesPageState::default(),
            focus_page: super::FocusPageState::default(),
            rail: None,
            pending_rail_refresh: false,
            place_page: PlacePageState::default(),
            calendar_invites_page: CalendarInvitesPageState::default(),
            active_label: None,
            pending_label_fetch: None,
            pending_active_label: None,
            active_role: None,
            pending_role_fetch: None,
            pending_labels_refresh: false,
            pending_all_envelopes_refresh: false,
            pending_subscriptions_refresh: false,
            pending_owed_refresh: false,
            pending_snoozed_refresh: false,
            pending_desk_refresh: true,
            pending_place_refresh: None,
            pending_place_more: None,
            pending_sweep_preview: None,
            pending_sweep: None,
            sweep_confirm: None,
            sender_kind_menu: None,
            pending_calendar_invites_refresh: false,
            pending_invite_open: None,
            todo_link: None,
            pending_commitment_counts_refresh: false,
            open_commitment_counts: HashMap::new(),
            reply_later_message_ids: HashSet::new(),
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
            quotes_expanded: false,
            collapsed_messages: HashSet::new(),
            scroll_to_focused_message: false,
            archive_step: None,
            attachment_panel: AttachmentPanelState::default(),
            pending_attachment_action: None,
            selected_set: HashSet::new(),
            visual_mode: false,
            visual_anchor: None,
            visual_count: None,
            pending_export: None,
            sidebar_accounts_expanded: true,
            sidebar_sync_throbber: ThrobberState::default(),
            sidebar_system_expanded: true,
            sidebar_user_expanded: true,
            sidebar_saved_searches_expanded: true,
            url_modal: None,
        }
    }
}
