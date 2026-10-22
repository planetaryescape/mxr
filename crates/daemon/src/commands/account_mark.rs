//! The ACCT column on CLI mail tables (blueprint 33).
//!
//! A table marks each row's account only when its rows can mix accounts: more
//! than one enabled account and no `--account` scope. JSON and JSONL never
//! carry the column; rows keep `account_id` there.

use crate::ipc_client::IpcClient;
use mxr_core::AccountId;
use mxr_protocol::{AccountSummaryData, Request, Response, ResponseData};
use std::collections::HashMap;

pub const HEADER: &str = "ACCT";

/// Tags by account for one table, or nothing when the column is hidden.
pub struct AccountMarks {
    tags: Option<HashMap<AccountId, String>>,
}

impl AccountMarks {
    pub fn hidden() -> Self {
        Self { tags: None }
    }

    /// Asks the daemon for the accounts, but only when a table is being drawn.
    /// `scoped` is true when the command was given `--account`.
    pub async fn load(client: &mut IpcClient, scoped: bool) -> anyhow::Result<Self> {
        if scoped {
            return Ok(Self::hidden());
        }
        let accounts = match client.request(Request::ListAccounts).await? {
            Response::Ok {
                data: ResponseData::Accounts { accounts },
            } => accounts,
            Response::Error { message, .. } => anyhow::bail!(message),
            other => anyhow::bail!("Unexpected accounts response: {other:?}"),
        };
        Ok(Self::from_accounts(&accounts, scoped))
    }

    pub fn from_accounts(accounts: &[AccountSummaryData], scoped: bool) -> Self {
        let enabled = accounts.iter().filter(|account| account.enabled).count();
        // A daemon from before identities sends none. Showing a column of
        // blanks would tell the user nothing, so the column stays hidden.
        let any_identity = accounts.iter().any(|account| account.identity.is_some());
        if scoped || enabled < 2 || !any_identity {
            return Self::hidden();
        }
        let tags = accounts
            .iter()
            .filter_map(|account| {
                Some((
                    account.account_id.clone(),
                    account.identity.as_ref()?.tag.clone(),
                ))
            })
            .collect();
        Self { tags: Some(tags) }
    }

    pub fn shown(&self) -> bool {
        self.tags.is_some()
    }

    /// The mark in front of a row: the tag and a space, or nothing when the
    /// column is hidden. Rows that are not tables use this too.
    pub fn prefix(&self, account_id: &AccountId) -> String {
        if self.shown() {
            format!("{:<4} ", self.cell(account_id))
        } else {
            String::new()
        }
    }

    /// Marks for a command's output. Only table output draws them, so JSON,
    /// JSONL and the other formats never ask the daemon.
    pub async fn marks_for(
        client: &mut IpcClient,
        table: bool,
        scoped: bool,
    ) -> anyhow::Result<Self> {
        if table {
            Self::load(client, scoped).await
        } else {
            Ok(Self::hidden())
        }
    }

    /// The row's cell: the account's tag, or blank when the account has none.
    pub fn cell(&self, account_id: &AccountId) -> String {
        match &self.tags {
            Some(tags) => tags.get(account_id).cloned().unwrap_or_default(),
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::AccountColor;
    use mxr_protocol::{AccountEditModeData, AccountIdentityData, AccountSourceData};

    fn account(n: u128, tag: &str, enabled: bool) -> AccountSummaryData {
        AccountSummaryData {
            account_id: AccountId::from_uuid(uuid::Uuid::from_u128(n)),
            key: Some(format!("a{n}")),
            name: format!("Account {n}"),
            email: format!("a{n}@example.com"),
            provider_kind: "fake".into(),
            sync_kind: None,
            send_kind: None,
            enabled,
            is_default: false,
            source: AccountSourceData::Config,
            editable: AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
            identity: Some(AccountIdentityData {
                color: AccountColor::Sky,
                tag: tag.into(),
            }),
        }
    }

    #[test]
    fn one_enabled_account_hides_the_column() {
        let marks = AccountMarks::from_accounts(&[account(1, "W", true)], false);
        assert!(!marks.shown());
    }

    #[test]
    fn disabled_accounts_do_not_count_towards_mixing() {
        let accounts = [account(1, "W", true), account(2, "WE", false)];
        assert!(!AccountMarks::from_accounts(&accounts, false).shown());
    }

    #[test]
    fn a_scoped_command_hides_the_column_even_with_two_accounts() {
        let accounts = [account(1, "W", true), account(2, "WE", true)];
        assert!(!AccountMarks::from_accounts(&accounts, true).shown());
    }

    #[test]
    fn an_older_daemon_without_identities_hides_the_column() {
        let mut older = account(1, "W", true);
        older.identity = None;
        let mut second = account(2, "WE", true);
        second.identity = None;
        let marks = AccountMarks::from_accounts(&[older, second], false);
        assert!(!marks.shown());
    }

    #[test]
    fn an_unknown_account_renders_blank_not_a_question_mark() {
        let work = account(1, "W", true);
        let wedding = account(2, "WE", true);
        let marks = AccountMarks::from_accounts(&[work, wedding], false);
        let unknown = AccountId::from_uuid(uuid::Uuid::from_u128(9));
        assert_eq!(marks.cell(&unknown), "");
    }

    #[test]
    fn two_enabled_accounts_show_each_tag() {
        let work = account(1, "W", true);
        let wedding = account(2, "WE", true);
        let marks = AccountMarks::from_accounts(&[work.clone(), wedding], false);
        assert!(marks.shown());
        assert_eq!(marks.cell(&work.account_id), "W");
    }
}
