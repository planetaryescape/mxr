//! State for the Deliveries screen (tab 7).

use mxr_protocol::DeliveryData;

/// Which slice of deliveries the screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryFilter {
    Active,
    Delivered,
    All,
}

impl Default for DeliveryFilter {
    fn default() -> Self {
        Self::Active
    }
}

impl DeliveryFilter {
    /// Wire value for `Request::ListDeliveries { filter }`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Delivered => "delivered",
            Self::All => "all",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Delivered => "Delivered",
            Self::All => "All",
        }
    }

    /// Cycle Active -> Delivered -> All -> Active.
    pub fn next(self) -> Self {
        match self {
            Self::Active => Self::Delivered,
            Self::Delivered => Self::All,
            Self::All => Self::Active,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DeliveriesState {
    pub rows: Vec<DeliveryData>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub filter: DeliveryFilter,
}

impl DeliveriesState {
    pub fn set_rows(&mut self, rows: Vec<DeliveryData>) {
        self.rows = rows;
        self.loading = false;
        self.error = None;
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    pub fn selected_row(&self) -> Option<&DeliveryData> {
        self.rows.get(self.selected)
    }

    pub fn select_next(&mut self) {
        if !self.rows.is_empty() {
            self.selected = (self.selected + 1).min(self.rows.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}
