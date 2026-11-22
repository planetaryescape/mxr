use super::*;
use crate::ui::account_mark::{AccountMarks, ChipDepth};

impl App {
    pub(crate) fn account_marks(&self) -> AccountMarks {
        AccountMarks::from_accounts(&self.accounts.page.accounts, false, ChipDepth::from_env())
    }
}
