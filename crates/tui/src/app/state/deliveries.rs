use mxr_protocol::DeliveryData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeliveryFilter {
    #[default]
    Active,
    Delivered,
    All,
}

impl DeliveryFilter {
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
    pub preview_active: bool,
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
