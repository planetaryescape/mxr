//! The account chip (blueprint 33): a tag in its account's colour. Email rows
//! carry it in views that mix accounts; the reader header and compose From
//! always show it.

use crate::theme::Theme;
use mxr_core::{AccountColor, AccountId};
use mxr_protocol::AccountSummaryData;
use ratatui::prelude::*;
use std::collections::HashMap;

/// Cells a row's chip column takes: a four-cell fill and a gap, or a plain
/// `[W]` padded to the same width.
pub const CHIP_WIDTH: usize = 5;

/// How the terminal can show the chip. `Plain` covers `NO_COLOR` and any
/// terminal that advertises neither 24-bit nor 256 colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChipDepth {
    #[default]
    Truecolor,
    Indexed,
    Plain,
}

impl ChipDepth {
    pub fn from_env() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::Plain;
        }
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        let colorterm = colorterm.to_ascii_lowercase();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return Self::Truecolor;
        }
        let term = std::env::var("TERM").unwrap_or_default();
        if term.contains("256color") {
            Self::Indexed
        } else {
            Self::Plain
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    name: String,
    email: String,
    identity: Option<(AccountColor, String)>,
}

/// Every account's chip and name, and whether a list shows chips at all.
///
/// `shown` depends only on the accounts and whether the view is scoped to
/// one of them, so a list never gains or loses a chip while it pages. It is
/// false for an older daemon, whose accounts carry no identity.
#[derive(Debug, Clone, Default)]
pub struct AccountMarks {
    entries: HashMap<AccountId, Entry>,
    shown: bool,
    depth: ChipDepth,
}

impl AccountMarks {
    pub fn from_accounts(accounts: &[AccountSummaryData], scoped: bool, depth: ChipDepth) -> Self {
        let enabled = accounts.iter().filter(|account| account.enabled).count();
        let has_identity = accounts.iter().any(|account| account.identity.is_some());
        let entries = accounts
            .iter()
            .map(|account| {
                let identity = account
                    .identity
                    .as_ref()
                    .map(|identity| (identity.color, identity.tag.clone()));
                (
                    account.account_id.clone(),
                    Entry {
                        name: account.name.clone(),
                        email: account.email.clone(),
                        identity,
                    },
                )
            })
            .collect();
        Self {
            entries,
            shown: enabled > 1 && !scoped && has_identity,
            depth,
        }
    }

    /// Whether email rows carry a chip column.
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Width the row's chip column takes, or zero when rows carry no chip.
    pub fn column_width(&self) -> usize {
        if self.shown {
            CHIP_WIDTH
        } else {
            0
        }
    }

    /// The chip cell for an email row: the account's chip when rows are
    /// marked, blank padding of the same width when the account is unknown,
    /// and `None` when the view shows no chips.
    pub fn row_cell(&self, account_id: &AccountId, theme: &Theme) -> Option<Vec<Span<'static>>> {
        if !self.shown {
            return None;
        }
        Some(
            self.chip(account_id, theme)
                .unwrap_or_else(|| vec![Span::raw(" ".repeat(CHIP_WIDTH))]),
        )
    }

    /// The chip for an account, shown whether or not rows are marked. `None`
    /// when the account has no identity (an older daemon).
    pub fn chip(&self, account_id: &AccountId, theme: &Theme) -> Option<Vec<Span<'static>>> {
        let (color, tag) = self.entries.get(account_id)?.identity.as_ref()?;
        Some(chip_spans(tag, *color, self.depth, &theme.account))
    }

    /// The account's name and address, for the reader header and compose.
    pub fn name_and_email(&self, account_id: &AccountId) -> Option<(&str, &str)> {
        self.entries
            .get(account_id)
            .map(|entry| (entry.name.as_str(), entry.email.as_str()))
    }
}

fn chip_spans(
    tag: &str,
    color: AccountColor,
    depth: ChipDepth,
    palette: &crate::theme::AccountPalette,
) -> Vec<Span<'static>> {
    match depth {
        ChipDepth::Plain => vec![Span::styled(
            format!("{:<width$}", format!("[{tag}]"), width = CHIP_WIDTH),
            Style::default().bold(),
        )],
        ChipDepth::Truecolor => vec![
            Span::styled(
                format!("{tag:^4}"),
                Style::default()
                    .fg(palette.ink)
                    .bg(palette.fill(color))
                    .bold(),
            ),
            Span::raw(" "),
        ],
        ChipDepth::Indexed => vec![
            Span::styled(
                format!("{tag:^4}"),
                Style::default()
                    .fg(palette.indexed_ink())
                    .bg(palette.indexed_fill(color))
                    .bold(),
            ),
            Span::raw(" "),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::AccountIdentityData;

    fn account(key: &str, enabled: bool, identity: Option<AccountColor>) -> AccountSummaryData {
        let mut summary = mxr_protocol::AccountSummaryData {
            account_id: AccountId::new(),
            key: Some(key.into()),
            name: key.into(),
            email: format!("{key}@example.com"),
            provider_kind: "imap".into(),
            sync_kind: Some("imap".into()),
            send_kind: None,
            enabled,
            is_default: false,
            source: mxr_protocol::AccountSourceData::Config,
            editable: mxr_protocol::AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
            identity: None,
        };
        summary.identity = identity.map(|color| AccountIdentityData {
            color,
            tag: key.chars().next().unwrap().to_ascii_uppercase().to_string(),
        });
        summary
    }

    #[test]
    fn marks_show_only_for_several_enabled_accounts_in_an_unscoped_view() {
        let work = account("work", true, Some(AccountColor::Sky));
        let home = account("home", true, Some(AccountColor::Amber));
        let cold = account("cold", false, Some(AccountColor::Green));
        let depth = ChipDepth::Truecolor;

        assert!(AccountMarks::from_accounts(&[work.clone(), home.clone()], false, depth).shown());
        assert!(!AccountMarks::from_accounts(std::slice::from_ref(&work), false, depth).shown());
        assert!(
            !AccountMarks::from_accounts(&[work.clone(), cold.clone()], false, depth).shown(),
            "a disabled account does not count toward mixing"
        );
        assert!(!AccountMarks::from_accounts(&[work.clone(), home.clone()], true, depth).shown());
    }

    #[test]
    fn older_daemon_without_identity_draws_no_marks() {
        let work = account("work", true, None);
        let home = account("home", true, None);
        let marks = AccountMarks::from_accounts(&[work.clone(), home], false, ChipDepth::Plain);
        assert!(!marks.shown());
        assert_eq!(marks.column_width(), 0);
        assert!(marks
            .row_cell(&work.account_id, &Theme::default())
            .is_none());
        assert!(marks.name_and_email(&work.account_id).is_some());
    }

    #[test]
    fn plain_chip_is_bold_bracketed_tag_with_no_colour() {
        let work = account("work", true, Some(AccountColor::Sky));
        let home = account("home", true, Some(AccountColor::Amber));
        let marks = AccountMarks::from_accounts(&[work.clone(), home], false, ChipDepth::Plain);
        let spans = marks.row_cell(&work.account_id, &Theme::default()).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "[W]  ");
        assert_eq!(spans[0].style.fg, None);
        assert_eq!(spans[0].style.bg, None);
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn truecolor_chip_fills_four_cells_in_the_account_colour() {
        let work = account("work", true, Some(AccountColor::Sky));
        let home = account("home", true, Some(AccountColor::Amber));
        let marks = AccountMarks::from_accounts(&[work.clone(), home], false, ChipDepth::Truecolor);
        let theme = Theme::dark();
        let spans = marks.row_cell(&work.account_id, &theme).unwrap();
        assert_eq!(spans[0].content, " W  ");
        assert_eq!(
            spans[0].style.bg,
            Some(theme.account.fill(AccountColor::Sky))
        );
        assert_eq!(spans[0].style.fg, Some(theme.account.ink));
        let width: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        assert_eq!(width, CHIP_WIDTH);
    }

    #[test]
    fn indexed_chip_uses_the_256_colour_pick() {
        let work = account("work", true, Some(AccountColor::Sky));
        let home = account("home", true, Some(AccountColor::Amber));
        let marks = AccountMarks::from_accounts(&[work.clone(), home], false, ChipDepth::Indexed);
        let theme = Theme::dark();
        let spans = marks.row_cell(&work.account_id, &theme).unwrap();
        assert_eq!(
            spans[0].style.bg,
            Some(theme.account.indexed_fill(AccountColor::Sky))
        );
    }
}
