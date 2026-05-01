#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticsPaneKind {
    #[default]
    Status,
    Data,
    Sync,
    Events,
    Logs,
}

impl DiagnosticsPaneKind {
    pub fn next(self) -> Self {
        match self {
            Self::Status => Self::Data,
            Self::Data => Self::Sync,
            Self::Sync => Self::Events,
            Self::Events => Self::Logs,
            Self::Logs => Self::Status,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Status => Self::Logs,
            Self::Data => Self::Status,
            Self::Sync => Self::Data,
            Self::Events => Self::Sync,
            Self::Logs => Self::Events,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsPageState {
    pub uptime_secs: Option<u64>,
    pub daemon_pid: Option<u32>,
    pub accounts: Vec<String>,
    pub total_messages: Option<u32>,
    pub sync_statuses: Vec<mxr_protocol::AccountSyncStatus>,
    pub doctor: Option<mxr_protocol::DoctorReport>,
    pub events: Vec<mxr_protocol::EventLogEntry>,
    pub logs: Vec<String>,
    pub status: Option<String>,
    pub refresh_pending: bool,
    pub pending_requests: u8,
    pub selected_pane: DiagnosticsPaneKind,
    pub fullscreen_pane: Option<DiagnosticsPaneKind>,
    pub status_scroll_offset: u16,
    pub data_scroll_offset: u16,
    pub sync_scroll_offset: u16,
    pub events_scroll_offset: u16,
    pub logs_scroll_offset: u16,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsState {
    pub page: DiagnosticsPageState,
    pub pending_bug_report: bool,
    pub pending_config_edit: bool,
    pub pending_log_open: bool,
    pub pending_details: Option<DiagnosticsPaneKind>,
    pub pending_status_refresh: bool,
    pub status_request_id: u64,
    pub request_id: u64,
}

impl DiagnosticsPageState {
    pub fn active_pane(&self) -> DiagnosticsPaneKind {
        self.fullscreen_pane.unwrap_or(self.selected_pane)
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen_pane = match self.fullscreen_pane {
            Some(pane) if pane == self.selected_pane => None,
            _ => Some(self.selected_pane),
        };
    }

    pub fn scroll_offset(&self, pane: DiagnosticsPaneKind) -> u16 {
        match pane {
            DiagnosticsPaneKind::Status => self.status_scroll_offset,
            DiagnosticsPaneKind::Data => self.data_scroll_offset,
            DiagnosticsPaneKind::Sync => self.sync_scroll_offset,
            DiagnosticsPaneKind::Events => self.events_scroll_offset,
            DiagnosticsPaneKind::Logs => self.logs_scroll_offset,
        }
    }

    pub fn scroll_offset_mut(&mut self, pane: DiagnosticsPaneKind) -> &mut u16 {
        match pane {
            DiagnosticsPaneKind::Status => &mut self.status_scroll_offset,
            DiagnosticsPaneKind::Data => &mut self.data_scroll_offset,
            DiagnosticsPaneKind::Sync => &mut self.sync_scroll_offset,
            DiagnosticsPaneKind::Events => &mut self.events_scroll_offset,
            DiagnosticsPaneKind::Logs => &mut self.logs_scroll_offset,
        }
    }
}
