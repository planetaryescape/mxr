use super::*;
use crate::ui::account_mark::{AccountMarks, ChipDepth};

impl App {
    /// The chips for the views on screen. No view is scoped to one account
    /// yet: every list request passes `account_id: None` (client.rs and
    /// runner.rs), so the scope is always "all accounts".
    pub(crate) fn account_marks(&self) -> AccountMarks {
        AccountMarks::from_accounts(&self.accounts.page.accounts, false, ChipDepth::from_env())
    }
}
