use crate::app::ActivePane;
use crate::theme::Theme;
use mxr_core::types::{Label, LabelKind, SavedSearch};
use ratatui::prelude::*;
use ratatui::widgets::*;
use throbber_widgets_tui::{Throbber, ThrobberState, BRAILLE_SIX};

/// Per-account sync state shown on the account's sidebar line. Derived
/// from the daemon's `AccountSyncStatus` snapshots (GetSyncStatus) which
/// refresh on sync daemon events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountSyncIndicator {
    /// A sync is currently running — renders an animated spinner, plus the
    /// live message count when the daemon is reporting one. Pre-formatted
    /// here so the renderer stays a layout concern: `"3000/16500"` when the
    /// provider could say how much is left, `"3000"` when it could not.
    Syncing { progress: Option<String> },
    /// Last sync succeeded; carries the relative age, e.g. "2m ago".
    Synced(String),
    /// The account is unhealthy or the last sync errored.
    Offline,
    /// No status known yet (daemon snapshot not loaded).
    Unknown,
}

pub struct AccountInfo {
    pub email: String,
    pub is_default: bool,
    pub sync: AccountSyncIndicator,
}

/// Map a daemon sync-status row to the sidebar indicator. `None` means
/// the snapshot hasn't loaded (or the account is missing from it).
pub fn sync_indicator_for(
    status: Option<&mxr_protocol::AccountSyncStatus>,
    synced_age: Option<String>,
) -> AccountSyncIndicator {
    let Some(status) = status else {
        return AccountSyncIndicator::Unknown;
    };
    if status.sync_in_progress {
        return AccountSyncIndicator::Syncing {
            // Plain digits, not thousands-separated: this shares one sidebar
            // line with the account address and a default marker, and the
            // separators cost width the column does not have.
            progress: status
                .progress
                .as_ref()
                .map(|progress| match progress.total {
                    Some(total) => format!("{}/{total}", progress.current),
                    None => progress.current.to_string(),
                }),
        };
    }
    if !status.healthy || status.last_error.is_some() {
        return AccountSyncIndicator::Offline;
    }
    match synced_age {
        Some(age) => AccountSyncIndicator::Synced(age),
        None => AccountSyncIndicator::Unknown,
    }
}

pub struct SidebarView<'a> {
    pub labels: &'a [Label],
    pub active_pane: &'a ActivePane,
    pub saved_searches: &'a [SavedSearch],
    pub sidebar_selected: usize,
    pub all_mail_active: bool,
    pub subscriptions_active: bool,
    pub subscription_count: usize,
    pub now_active: bool,
    /// Now's badge from the rail: work only, the one badge on the rail.
    pub now_badge: Option<u32>,
    /// Messages, an early version on the desk lens.
    pub messages_active: bool,
    pub todo_active: bool,
    /// Updates, the briefing.
    pub updates_active: bool,
    pub reading_active: bool,
    pub archive_active: bool,
    /// Rail ids (`messages`, `updates`, …) the daemon marks early.
    pub early_modes: Vec<String>,
    pub owed_active: bool,
    pub owed_count: usize,
    pub calendar_invites_active: bool,
    pub calendar_invites_count: usize,
    pub accounts: Vec<AccountInfo>,
    pub accounts_expanded: bool,
    /// Spinner state for accounts currently syncing; ticked by the app
    /// while any account reports `sync_in_progress`.
    pub sync_throbber: Option<&'a ThrobberState>,
    /// The More section (pages off the rail, then the system folders).
    pub system_expanded: bool,
    pub user_expanded: bool,
    pub saved_searches_expanded: bool,
    pub active_label: Option<&'a mxr_core::LabelId>,
}

struct SidebarBuildState<'a> {
    subscription_count: usize,
    owed_count: usize,
    calendar_invites_count: usize,
    accounts: &'a [AccountInfo],
    accounts_expanded: bool,
    system_expanded: bool,
    user_expanded: bool,
    saved_searches_expanded: bool,
}

/// One of the rail's entries: Now, the five modes, then Inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RailItem {
    Now,
    Messages,
    Todo,
    Updates,
    Reading,
    Archive,
}

impl RailItem {
    /// Rail order.
    pub(crate) const MODES: [Self; 5] = [
        Self::Messages,
        Self::Todo,
        Self::Updates,
        Self::Reading,
        Self::Archive,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Now => "Now",
            Self::Messages => "Messages",
            Self::Todo => "To do",
            Self::Updates => "Updates",
            Self::Reading => "Reading",
            Self::Archive => "Archive",
        }
    }

    /// The id `GetRail` uses.
    const fn id(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::Messages => "messages",
            Self::Todo => "todo",
            Self::Updates => "updates",
            Self::Reading => "reading",
            Self::Archive => "archive",
        }
    }
}

/// Pages that moved off the rail, under More (D117).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MorePage {
    Screener,
    ReplyQueue,
    Waiting,
}

impl MorePage {
    pub(crate) const ALL: [Self; 3] = [Self::Screener, Self::ReplyQueue, Self::Waiting];

    const fn name(self) -> &'static str {
        match self {
            Self::Screener => "Screener",
            Self::ReplyQueue => "Reply queue",
            Self::Waiting => "Waiting on",
        }
    }
}

#[derive(Debug, Clone)]
enum SidebarEntry<'a> {
    Separator,
    Header {
        title: &'static str,
        expanded: bool,
    },
    Account {
        email: String,
        is_default: bool,
        sync: AccountSyncIndicator,
    },
    Rail(RailItem),
    Inbox,
    More(MorePage),
    AllMail,
    Subscriptions {
        count: usize,
    },
    Owed {
        count: usize,
    },
    CalendarInvites {
        count: usize,
    },
    Label(&'a Label),
    SavedSearch(&'a SavedSearch),
}

/// The inbox is on the rail as a lens of its own, so it isn't repeated
/// among the folders under More.
pub fn is_inbox_label(name: &str) -> bool {
    name == "INBOX"
}

pub fn draw(frame: &mut Frame, area: Rect, view: &SidebarView<'_>, theme: &Theme) {
    let is_focused = *view.active_pane == ActivePane::Sidebar;
    let border_style = theme.border_style(is_focused);

    let inner_width = area.width.saturating_sub(2) as usize;
    let build_state = SidebarBuildState {
        subscription_count: view.subscription_count,
        owed_count: view.owed_count,
        calendar_invites_count: view.calendar_invites_count,
        accounts: &view.accounts,
        accounts_expanded: view.accounts_expanded,
        system_expanded: view.system_expanded,
        user_expanded: view.user_expanded,
        saved_searches_expanded: view.saved_searches_expanded,
    };
    let entries = build_sidebar_entries(view.labels, view.saved_searches, &build_state);
    let selected_visual_index = visual_index_for_selection(&entries, view.sidebar_selected);
    let inbox_active = view.labels.iter().any(|label| {
        is_inbox_label(&label.name) && view.active_label.is_some_and(|id| *id == label.id)
    });

    let items = entries
        .iter()
        .map(|entry| match entry {
            SidebarEntry::Separator => {
                // Visual separator line instead of empty spacer
                ListItem::new(Line::from(Span::styled(
                    "─".repeat(inner_width),
                    Style::default().fg(theme.text_muted),
                )))
            }
            SidebarEntry::Header { title, expanded } => ListItem::new(Line::from(vec![
                Span::styled(
                    if *expanded { "▾ " } else { "▸ " },
                    Style::default().fg(theme.text_muted),
                ),
                Span::styled(*title, Style::default().fg(theme.accent).bold()),
            ])),
            SidebarEntry::Account {
                email,
                is_default,
                sync,
            } => render_account_item(
                inner_width,
                email,
                *is_default,
                sync,
                view.sync_throbber,
                theme,
            ),
            SidebarEntry::Rail(item) => render_rail_item(inner_width, *item, view, theme),
            SidebarEntry::Inbox => {
                render_sidebar_link(inner_width, "Inbox", None, inbox_active, theme)
            }
            SidebarEntry::More(page) => {
                render_sidebar_link(inner_width, page.name(), None, false, theme)
            }
            SidebarEntry::AllMail => render_all_mail_item(inner_width, view.all_mail_active, theme),
            SidebarEntry::Subscriptions { count } => {
                render_subscriptions_item(inner_width, *count, view.subscriptions_active, theme)
            }
            SidebarEntry::Owed { count } => {
                render_owed_item(inner_width, *count, view.owed_active, theme)
            }
            SidebarEntry::CalendarInvites { count } => render_calendar_invites_item(
                inner_width,
                *count,
                view.calendar_invites_active,
                theme,
            ),
            SidebarEntry::Label(label) => {
                render_label_item(label, inner_width, view.active_label, theme)
            }
            SidebarEntry::SavedSearch(search) => ListItem::new(format!("  {}", search.name)),
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Sidebar ")
                .border_type(BorderType::Rounded)
                .border_style(border_style),
        )
        .highlight_style(theme.highlight_style());

    if is_focused {
        let mut state = ListState::default().with_selected(selected_visual_index);
        frame.render_stateful_widget(list, area, &mut state);
    } else {
        frame.render_widget(list, area);
    }
}

/// A rail entry: its name, Now's badge, and a dim "early" on modes that
/// are early versions on an existing view.
fn render_rail_item<'a>(
    inner_width: usize,
    item: RailItem,
    view: &SidebarView<'_>,
    theme: &Theme,
) -> ListItem<'a> {
    let active = match item {
        RailItem::Now => view.now_active,
        RailItem::Messages => view.messages_active,
        RailItem::Todo => view.todo_active,
        RailItem::Updates => view.updates_active,
        RailItem::Reading => view.reading_active,
        RailItem::Archive => view.archive_active,
    };
    if item == RailItem::Now {
        let badge = view
            .now_badge
            .filter(|badge| *badge > 0)
            .map(|badge| badge.to_string());
        return render_sidebar_link(inner_width, item.name(), badge.as_deref(), active, theme);
    }
    if !view.early_modes.iter().any(|id| id == item.id()) {
        return render_sidebar_link(inner_width, item.name(), None, active, theme);
    }
    let name_part = format!("  {}", item.name());
    let early = "early";
    let padding = inner_width.saturating_sub(name_part.chars().count() + early.len());
    let style = if active {
        Style::default()
            .bg(theme.selection_bg)
            .fg(theme.accent)
            .bold()
    } else {
        Style::default()
    };
    ListItem::new(Line::from(vec![
        Span::raw(name_part),
        Span::raw(" ".repeat(padding)),
        Span::styled(early, Style::default().fg(theme.text_muted)),
    ]))
    .style(style)
}

fn build_sidebar_entries<'a>(
    labels: &'a [Label],
    saved_searches: &'a [SavedSearch],
    state: &SidebarBuildState<'_>,
) -> Vec<SidebarEntry<'a>> {
    let visible_labels: Vec<&Label> = labels
        .iter()
        .filter(|label| !should_hide_label(&label.name))
        .collect();

    let mut system_labels: Vec<&Label> = visible_labels
        .iter()
        .filter(|label| label.kind == LabelKind::System && !is_inbox_label(&label.name))
        .filter(|label| {
            is_primary_system_label(&label.name) || label.total_count > 0 || label.unread_count > 0
        })
        .copied()
        .collect();
    system_labels.sort_by_key(|label| system_label_order(&label.name));

    let mut user_labels: Vec<&Label> = visible_labels
        .iter()
        .filter(|label| label.kind != LabelKind::System)
        .copied()
        .collect();
    user_labels.sort_by_key(|label| label.name.to_lowercase());

    let mut entries = Vec::new();

    // Accounts section (only shown when multiple accounts exist)
    if state.accounts.len() > 1 {
        entries.push(SidebarEntry::Header {
            title: "Accounts",
            expanded: state.accounts_expanded,
        });
        if state.accounts_expanded {
            entries.extend(state.accounts.iter().map(|a| SidebarEntry::Account {
                email: a.email.clone(),
                is_default: a.is_default,
                sync: a.sync.clone(),
            }));
        }
        entries.push(SidebarEntry::Separator);
    }

    // The rail: Now, the five modes, then Inbox (blueprint 22).
    entries.push(SidebarEntry::Rail(RailItem::Now));
    entries.push(SidebarEntry::Separator);
    entries.extend(RailItem::MODES.into_iter().map(SidebarEntry::Rail));
    entries.push(SidebarEntry::Separator);
    entries.push(SidebarEntry::Inbox);

    entries.push(SidebarEntry::Header {
        title: "More",
        expanded: state.system_expanded,
    });
    if state.system_expanded {
        entries.extend(MorePage::ALL.into_iter().map(SidebarEntry::More));
        entries.push(SidebarEntry::Subscriptions {
            count: state.subscription_count,
        });
        entries.push(SidebarEntry::Owed {
            count: state.owed_count,
        });
        entries.push(SidebarEntry::CalendarInvites {
            count: state.calendar_invites_count,
        });
        entries.push(SidebarEntry::AllMail);
        entries.extend(system_labels.into_iter().map(SidebarEntry::Label));
    }

    if !user_labels.is_empty() {
        entries.push(SidebarEntry::Separator);
        entries.push(SidebarEntry::Header {
            title: "Labels",
            expanded: state.user_expanded,
        });
        if state.user_expanded {
            entries.extend(user_labels.into_iter().map(SidebarEntry::Label));
        }
    }

    if !saved_searches.is_empty() {
        entries.push(SidebarEntry::Separator);
        entries.push(SidebarEntry::Header {
            title: "Saved Searches",
            expanded: state.saved_searches_expanded,
        });
        if state.saved_searches_expanded {
            entries.extend(saved_searches.iter().map(SidebarEntry::SavedSearch));
        }
    }

    entries
}

fn visual_index_for_selection(
    entries: &[SidebarEntry<'_>],
    sidebar_selected: usize,
) -> Option<usize> {
    let mut selectable = 0usize;
    for (visual_index, entry) in entries.iter().enumerate() {
        match entry {
            SidebarEntry::Account { .. }
            | SidebarEntry::Rail(_)
            | SidebarEntry::Inbox
            | SidebarEntry::More(_)
            | SidebarEntry::AllMail
            | SidebarEntry::Subscriptions { .. }
            | SidebarEntry::Owed { .. }
            | SidebarEntry::CalendarInvites { .. }
            | SidebarEntry::Label(_)
            | SidebarEntry::SavedSearch(_) => {
                if selectable == sidebar_selected {
                    return Some(visual_index);
                }
                selectable += 1;
            }
            SidebarEntry::Separator | SidebarEntry::Header { .. } => {}
        }
    }
    None
}

fn render_account_item<'a>(
    inner_width: usize,
    email: &str,
    is_default: bool,
    sync: &AccountSyncIndicator,
    sync_throbber: Option<&ThrobberState>,
    theme: &Theme,
) -> ListItem<'a> {
    let style = if is_default {
        Style::default().fg(theme.accent).bold()
    } else {
        Style::default().fg(theme.text_muted)
    };
    let default_marker = if is_default { " ●" } else { "" };
    let name_part = format!("  {email}{default_marker}");

    // Right-aligned per-account sync state: spinner while syncing,
    // "synced <age>" when healthy, an offline marker on errors.
    let (sync_spans, sync_width): (Vec<Span<'a>>, usize) = match sync {
        AccountSyncIndicator::Syncing { progress } => {
            let spinner = sync_throbber.map_or_else(
                || Span::styled("…", Style::default().fg(theme.accent)),
                |state| {
                    Throbber::default()
                        .throbber_set(BRAILLE_SIX)
                        .throbber_style(Style::default().fg(theme.accent))
                        .to_symbol_span(state)
                },
            );
            let mut width = spinner.width();
            let mut spans = vec![spinner];
            if let Some(progress) = progress {
                let text = format!(" {progress}");
                width += text.chars().count();
                spans.push(Span::styled(text, Style::default().fg(theme.text_muted)));
            }
            (spans, width)
        }
        AccountSyncIndicator::Synced(age) => {
            let text = format!("synced {age}");
            let width = text.len();
            (
                vec![Span::styled(text, Style::default().fg(theme.text_muted))],
                width,
            )
        }
        AccountSyncIndicator::Offline => (
            vec![Span::styled(
                "offline".to_string(),
                Style::default().fg(theme.warning),
            )],
            "offline".len(),
        ),
        AccountSyncIndicator::Unknown => (Vec::new(), 0),
    };

    if sync_spans.is_empty() || name_part.len() + sync_width + 1 > inner_width {
        return ListItem::new(name_part).style(style);
    }

    let padding = inner_width.saturating_sub(name_part.len() + sync_width);
    let mut spans = vec![Span::raw(name_part), Span::raw(" ".repeat(padding))];
    spans.extend(sync_spans);
    ListItem::new(Line::from(spans)).style(style)
}

fn render_all_mail_item<'a>(inner_width: usize, is_active: bool, theme: &Theme) -> ListItem<'a> {
    render_sidebar_link(inner_width, "All Mail", None, is_active, theme)
}

fn render_subscriptions_item<'a>(
    inner_width: usize,
    count: usize,
    is_active: bool,
    theme: &Theme,
) -> ListItem<'a> {
    let count_str = (count > 0).then(|| count.to_string());
    render_sidebar_link(
        inner_width,
        "Subscriptions",
        count_str.as_deref(),
        is_active,
        theme,
    )
}

fn render_owed_item<'a>(
    inner_width: usize,
    count: usize,
    is_active: bool,
    theme: &Theme,
) -> ListItem<'a> {
    let count_str = (count > 0).then(|| count.to_string());
    render_sidebar_link(inner_width, "Owed", count_str.as_deref(), is_active, theme)
}

fn render_calendar_invites_item<'a>(
    inner_width: usize,
    count: usize,
    is_active: bool,
    theme: &Theme,
) -> ListItem<'a> {
    let count_str = (count > 0).then(|| count.to_string());
    render_sidebar_link(
        inner_width,
        "Calendar invites",
        count_str.as_deref(),
        is_active,
        theme,
    )
}

fn render_sidebar_link<'a>(
    inner_width: usize,
    name: &str,
    count: Option<&str>,
    is_active: bool,
    theme: &Theme,
) -> ListItem<'a> {
    let line = format!("  {:<width$}", name, width = inner_width.saturating_sub(2));
    let line = if let Some(count) = count {
        let name_part = format!("  {name}");
        let padding = inner_width.saturating_sub(name_part.len() + count.len());
        format!("{}{}{}", name_part, " ".repeat(padding), count)
    } else {
        line
    };
    let style = if is_active {
        Style::default()
            .bg(theme.selection_bg)
            .fg(theme.accent)
            .bold()
    } else {
        Style::default()
    };
    ListItem::new(line).style(style)
}

fn render_label_item<'a>(
    label: &Label,
    inner_width: usize,
    active_label: Option<&mxr_core::LabelId>,
    theme: &Theme,
) -> ListItem<'a> {
    let is_active = active_label.is_some_and(|current| current == &label.id);
    let display_name = humanize_label(&label.name);

    let count_str = if label.unread_count > 0 {
        format!("{}/{}", label.unread_count, label.total_count)
    } else if label.total_count > 0 {
        label.total_count.to_string()
    } else {
        String::new()
    };

    // Right-align count: name on left, count on right
    let name_part = format!("  {display_name}");
    let line = if count_str.is_empty() {
        name_part
    } else {
        let padding = inner_width.saturating_sub(name_part.len() + count_str.len());
        format!("{}{}{}", name_part, " ".repeat(padding), count_str)
    };

    let style = if is_active {
        // Full-width highlight bar for active label
        Style::default()
            .bg(theme.selection_bg)
            .fg(theme.accent)
            .bold()
    } else if label.unread_count > 0 {
        theme.unread_style()
    } else {
        Style::default()
    };

    ListItem::new(line).style(style)
}

pub fn humanize_label(name: &str) -> &str {
    match name {
        "INBOX" => "Inbox",
        "SENT" => "Sent",
        "DRAFT" => "Drafts",
        "ARCHIVE" => "Archive",
        "ALL" => "All Mail",
        "TRASH" => "Trash",
        "SPAM" => "Spam",
        "STARRED" => "Starred",
        "IMPORTANT" => "Important",
        "UNREAD" => "Unread",
        "CHAT" => "Chat",
        _ => name,
    }
}

pub fn should_hide_label(name: &str) -> bool {
    matches!(
        name,
        "CATEGORY_FORUMS"
            | "CATEGORY_UPDATES"
            | "CATEGORY_PERSONAL"
            | "CATEGORY_PROMOTIONS"
            | "CATEGORY_SOCIAL"
            | "ALL"
            | "RED_STAR"
            | "YELLOW_STAR"
            | "ORANGE_STAR"
            | "GREEN_STAR"
            | "BLUE_STAR"
            | "PURPLE_STAR"
            | "RED_BANG"
            | "YELLOW_BANG"
            | "BLUE_INFO"
            | "ORANGE_GUILLEMET"
            | "GREEN_CHECK"
            | "PURPLE_QUESTION"
    )
}

pub fn is_primary_system_label(name: &str) -> bool {
    mxr_core::types::system_labels::is_primary(name)
}

pub fn system_label_order(name: &str) -> usize {
    mxr_core::types::system_labels::display_order(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, LabelId, SavedSearchId};
    use mxr_core::types::{SearchMode, SortOrder};

    fn label(name: &str, kind: LabelKind) -> Label {
        Label {
            id: LabelId::new(),
            account_id: AccountId::new(),
            name: name.into(),
            kind,
            color: None,
            provider_id: name.into(),
            unread_count: 0,
            total_count: 1,
            role: None,
        }
    }

    fn sync_status(
        sync_in_progress: bool,
        healthy: bool,
        error: Option<&str>,
    ) -> mxr_protocol::AccountSyncStatus {
        mxr_protocol::AccountSyncStatus {
            account_id: AccountId::new(),
            account_name: "test".into(),
            last_attempt_at: None,
            last_success_at: None,
            last_error: error.map(String::from),
            failure_class: None,
            consecutive_failures: 0,
            backoff_until: None,
            sync_in_progress,
            current_cursor_summary: None,
            last_synced_count: 0,
            healthy,
            progress: None,
        }
    }

    fn syncing_with_progress(current: u32, total: Option<u32>) -> mxr_protocol::AccountSyncStatus {
        let mut status = sync_status(true, true, None);
        status.progress = Some(mxr_protocol::SyncProgressData {
            current,
            total,
            message: "Stored messages".into(),
        });
        status
    }

    #[test]
    fn sync_indicator_maps_daemon_status_to_sidebar_state() {
        assert_eq!(
            sync_indicator_for(None, None),
            AccountSyncIndicator::Unknown,
            "missing snapshot must read as unknown, not offline"
        );
        assert_eq!(
            sync_indicator_for(Some(&sync_status(true, true, None)), None),
            AccountSyncIndicator::Syncing { progress: None }
        );
        assert_eq!(
            sync_indicator_for(Some(&sync_status(false, false, None)), None),
            AccountSyncIndicator::Offline
        );
        assert_eq!(
            sync_indicator_for(Some(&sync_status(false, true, Some("boom"))), None),
            AccountSyncIndicator::Offline,
            "a recorded sync error must surface even when marked healthy"
        );
        assert_eq!(
            sync_indicator_for(Some(&sync_status(false, true, None)), Some("2m ago".into())),
            AccountSyncIndicator::Synced("2m ago".into())
        );
        assert_eq!(
            sync_indicator_for(Some(&sync_status(false, true, None)), None),
            AccountSyncIndicator::Unknown,
            "healthy but never-synced accounts show no stale age"
        );
    }

    /// A backfill runs for minutes; a bare spinner says only that something
    /// is happening. The daemon reports how far the pass has got, so the
    /// sidebar shows it, with a denominator when the provider could count
    /// what was left and without one when it could not.
    #[test]
    fn sync_indicator_carries_the_live_message_count_while_syncing() {
        assert_eq!(
            sync_indicator_for(Some(&syncing_with_progress(3000, Some(16500))), None),
            AccountSyncIndicator::Syncing {
                progress: Some("3000/16500".into())
            }
        );
        assert_eq!(
            sync_indicator_for(Some(&syncing_with_progress(3000, None)), None),
            AccountSyncIndicator::Syncing {
                progress: Some("3000".into())
            },
            "a provider that cannot count what is left still shows movement"
        );
    }

    fn build_state() -> SidebarBuildState<'static> {
        SidebarBuildState {
            subscription_count: 3,
            owed_count: 0,
            calendar_invites_count: 0,
            accounts: &[],
            accounts_expanded: true,
            system_expanded: true,
            user_expanded: true,
            saved_searches_expanded: true,
        }
    }

    #[test]
    fn the_rail_is_now_the_modes_then_inbox_with_the_rest_under_more() {
        let labels = vec![
            label("INBOX", LabelKind::System),
            label("SENT", LabelKind::System),
            label("Work", LabelKind::User),
        ];
        let entries = build_sidebar_entries(&labels, &[], &build_state());
        assert!(matches!(entries[0], SidebarEntry::Rail(RailItem::Now)));
        assert!(matches!(entries[1], SidebarEntry::Separator));
        assert!(matches!(entries[2], SidebarEntry::Rail(RailItem::Messages)));
        assert!(matches!(entries[3], SidebarEntry::Rail(RailItem::Todo)));
        assert!(matches!(entries[4], SidebarEntry::Rail(RailItem::Updates)));
        assert!(matches!(entries[5], SidebarEntry::Rail(RailItem::Reading)));
        assert!(matches!(entries[6], SidebarEntry::Rail(RailItem::Archive)));
        assert!(matches!(entries[7], SidebarEntry::Separator));
        assert!(matches!(entries[8], SidebarEntry::Inbox));
        assert!(matches!(
            entries[9],
            SidebarEntry::Header { title: "More", .. }
        ));
        assert!(matches!(
            entries[10],
            SidebarEntry::More(MorePage::Screener)
        ));
        assert!(matches!(
            entries[11],
            SidebarEntry::More(MorePage::ReplyQueue)
        ));
        assert!(matches!(entries[12], SidebarEntry::More(MorePage::Waiting)));
        assert!(matches!(
            entries[13],
            SidebarEntry::Subscriptions { count: 3 }
        ));
        assert!(matches!(entries[14], SidebarEntry::Owed { .. }));
        assert!(matches!(entries[15], SidebarEntry::CalendarInvites { .. }));
        assert!(matches!(entries[16], SidebarEntry::AllMail));
        // Inbox is on the rail, so it isn't repeated among the folders.
        assert!(matches!(entries[17], SidebarEntry::Label(label) if label.name == "SENT"));
        assert!(matches!(entries[18], SidebarEntry::Separator));
        assert!(matches!(
            entries[19],
            SidebarEntry::Header {
                title: "Labels",
                ..
            }
        ));
        assert!(matches!(entries[20], SidebarEntry::Label(label) if label.name == "Work"));
    }

    #[test]
    fn sidebar_selection_skips_headers_and_spacers() {
        let labels = vec![
            label("INBOX", LabelKind::System),
            label("Work", LabelKind::User),
        ];
        let searches = vec![SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Unread".into(),
            query: "is:unread".into(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        }];
        let entries = build_sidebar_entries(&labels, &searches, &build_state());
        assert_eq!(visual_index_for_selection(&entries, 0), Some(0)); // Now
        assert_eq!(visual_index_for_selection(&entries, 1), Some(2)); // Messages
        assert_eq!(visual_index_for_selection(&entries, 5), Some(6)); // Archive
        assert_eq!(visual_index_for_selection(&entries, 6), Some(8)); // Inbox
        assert_eq!(visual_index_for_selection(&entries, 7), Some(10)); // Screener
        assert_eq!(visual_index_for_selection(&entries, 13), Some(16)); // All Mail
        assert_eq!(visual_index_for_selection(&entries, 14), Some(19)); // Work
        assert_eq!(visual_index_for_selection(&entries, 15), Some(22)); // Unread
    }

    #[test]
    fn folding_more_hides_its_pages_and_folders() {
        let labels = vec![
            label("INBOX", LabelKind::System),
            label("SENT", LabelKind::System),
        ];
        let state = SidebarBuildState {
            system_expanded: false,
            ..build_state()
        };
        let entries = build_sidebar_entries(&labels, &[], &state);
        assert!(matches!(
            entries.last(),
            Some(SidebarEntry::Header {
                title: "More",
                expanded: false
            })
        ));
        assert!(!entries
            .iter()
            .any(|entry| matches!(entry, SidebarEntry::More(_))));
    }
}
