use crate::ui::search_bar::SearchBar;
use mxr_core::id::MessageId;
use mxr_core::types::*;
use std::collections::HashMap;
use std::time::Instant;
use throbber_widgets_tui::ThrobberState;

#[derive(Default)]
pub struct SearchState {
    pub bar: SearchBar,
    pub page: SearchPageState,
    pub pending: Option<PendingSearchRequest>,
    pub pending_count: Option<PendingSearchCountRequest>,
    pub pending_debounce: Option<PendingSearchDebounce>,
    pub mailbox_session_id: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchPane {
    #[default]
    Results,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchUiStatus {
    #[default]
    Idle,
    Debouncing,
    Searching,
    LoadingMore,
    Loaded,
    Error,
}

#[derive(Debug, Clone)]
pub struct SearchPageState {
    pub query: String,
    pub editing: bool,
    pub results: Vec<Envelope>,
    pub scores: HashMap<MessageId, f32>,
    pub mode: SearchMode,
    pub sort: SortOrder,
    pub has_more: bool,
    pub loading_more: bool,
    pub total_count: Option<u32>,
    pub count_pending: bool,
    pub ui_status: SearchUiStatus,
    pub session_active: bool,
    pub load_to_end: bool,
    pub session_id: u64,
    pub active_pane: SearchPane,
    pub preview_fullscreen: bool,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub result_selected: bool,
    pub throbber: ThrobberState,
}

impl SearchPageState {
    pub fn has_session(&self) -> bool {
        self.session_active || !self.results.is_empty()
    }
}

impl Default for SearchPageState {
    fn default() -> Self {
        Self {
            query: String::new(),
            editing: false,
            results: Vec::new(),
            scores: HashMap::new(),
            mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            has_more: false,
            loading_more: false,
            total_count: None,
            count_pending: false,
            ui_status: SearchUiStatus::Idle,
            session_active: false,
            load_to_end: false,
            session_id: 0,
            active_pane: SearchPane::Results,
            preview_fullscreen: false,
            selected_index: 0,
            scroll_offset: 0,
            result_selected: false,
            throbber: ThrobberState::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTarget {
    Mailbox,
    SearchPage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSearchRequest {
    pub query: String,
    pub mode: SearchMode,
    pub sort: SortOrder,
    pub limit: u32,
    pub offset: u32,
    pub target: SearchTarget,
    pub append: bool,
    pub session_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSearchCountRequest {
    pub query: String,
    pub mode: SearchMode,
    pub session_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSearchDebounce {
    pub query: String,
    pub mode: SearchMode,
    pub session_id: u64,
    pub due_at: Instant,
}
