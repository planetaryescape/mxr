use crate::app::ActivePane;
use crate::theme::Theme;
use mxr_core::types::{Label, LabelKind, SavedSearch};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct SidebarView<'a> {
    pub labels: &'a [Label],
    pub active_pane: &'a ActivePane,
    pub saved_searches: &'a [SavedSearch],
    pub sidebar_selected: usize,
    pub all_mail_active: bool,
    pub subscriptions_active: bool,
    pub subscription_count: usize,
    pub system_expanded: bool,
    pub user_expanded: bool,
    pub saved_searches_expanded: bool,
    pub active_label: Option<&'a mxr_core::LabelId>,
}

#[derive(Debug, Clone)]
enum SidebarEntry<'a> {
    Separator,
    Header { title: &'static str, expanded: bool },
    AllMail,
    Subscriptions { count: usize },
    Label(&'a Label),
    SavedSearch(&'a SavedSearch),
}

pub fn draw(frame: &mut Frame, area: Rect, view: &SidebarView<'_>, theme: &Theme) {
    let is_focused = *view.active_pane == ActivePane::Sidebar;
    let border_style = theme.border_style(is_focused);

    let inner_width = area.width.saturating_sub(2) as usize;
    let entries = build_sidebar_entries(
        view.labels,
        view.saved_searches,
        view.subscription_count,
        view.system_expanded,
        view.user_expanded,
        view.saved_searches_expanded,
    );
    let selected_visual_index = visual_index_for_selection(&entries, view.sidebar_selected);

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
            SidebarEntry::AllMail => render_all_mail_item(inner_width, view.all_mail_active, theme),
            SidebarEntry::Subscriptions { count } => {
                render_subscriptions_item(inner_width, *count, view.subscriptions_active, theme)
            }
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

fn build_sidebar_entries<'a>(
    labels: &'a [Label],
    saved_searches: &'a [SavedSearch],
    subscription_count: usize,
    system_expanded: bool,
    user_expanded: bool,
    saved_searches_expanded: bool,
) -> Vec<SidebarEntry<'a>> {
    let visible_labels: Vec<&Label> = labels
        .iter()
        .filter(|label| !should_hide_label(&label.name))
        .collect();

    let mut system_labels: Vec<&Label> = visible_labels
        .iter()
        .filter(|label| label.kind == LabelKind::System)
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
    user_labels.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    let mut entries = vec![
        SidebarEntry::Header {
            title: "System",
            expanded: system_expanded,
        },
        SidebarEntry::AllMail,
        SidebarEntry::Subscriptions {
            count: subscription_count,
        },
    ];
    if system_expanded {
        entries.extend(system_labels.into_iter().map(SidebarEntry::Label));
    }

    if !user_labels.is_empty() {
        if !entries.is_empty() {
            entries.push(SidebarEntry::Separator);
        }
        entries.push(SidebarEntry::Header {
            title: "Labels",
            expanded: user_expanded,
        });
        if user_expanded {
            entries.extend(user_labels.into_iter().map(SidebarEntry::Label));
        }
    }

    if !saved_searches.is_empty() {
        if !entries.is_empty() {
            entries.push(SidebarEntry::Separator);
        }
        entries.push(SidebarEntry::Header {
            title: "Saved Searches",
            expanded: saved_searches_expanded,
        });
        if saved_searches_expanded {
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
            SidebarEntry::AllMail
            | SidebarEntry::Subscriptions { .. }
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

fn render_sidebar_link<'a>(
    inner_width: usize,
    name: &str,
    count: Option<&str>,
    is_active: bool,
    theme: &Theme,
) -> ListItem<'a> {
    let line = format!("  {:<width$}", name, width = inner_width.saturating_sub(2));
    let line = if let Some(count) = count {
        let name_part = format!("  {}", name);
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
    let is_active = active_label
        .map(|current| current == &label.id)
        .unwrap_or(false);
    let display_name = humanize_label(&label.name);

    let count_str = if label.unread_count > 0 {
        format!("{}/{}", label.unread_count, label.total_count)
    } else if label.total_count > 0 {
        label.total_count.to_string()
    } else {
        String::new()
    };

    // Right-align count: name on left, count on right
    let name_part = format!("  {}", display_name);
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
    matches!(
        name,
        "INBOX" | "STARRED" | "SENT" | "DRAFT" | "ARCHIVE" | "SPAM" | "TRASH"
    )
}

pub fn system_label_order(name: &str) -> usize {
    match name {
        "INBOX" => 0,
        "STARRED" => 1,
        "SENT" => 2,
        "DRAFT" => 3,
        "ARCHIVE" => 4,
        "SPAM" => 5,
        "TRASH" => 6,
        _ => 100,
    }
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
        }
    }

    #[test]
    fn sidebar_entries_insert_labels_header_before_user_labels() {
        let labels = vec![
            label("INBOX", LabelKind::System),
            label("Work", LabelKind::User),
        ];
        let entries = build_sidebar_entries(&labels, &[], 3, true, true, true);
        assert!(matches!(
            entries[0],
            SidebarEntry::Header {
                title: "System",
                ..
            }
        ));
        assert!(matches!(entries[1], SidebarEntry::AllMail));
        assert!(matches!(
            entries[2],
            SidebarEntry::Subscriptions { count: 3 }
        ));
        assert!(matches!(entries[3], SidebarEntry::Label(label) if label.name == "INBOX"));
        assert!(matches!(entries[4], SidebarEntry::Separator));
        assert!(matches!(
            entries[5],
            SidebarEntry::Header {
                title: "Labels",
                ..
            }
        ));
        assert!(matches!(entries[6], SidebarEntry::Label(label) if label.name == "Work"));
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
        let entries = build_sidebar_entries(&labels, &searches, 2, true, true, true);
        assert_eq!(visual_index_for_selection(&entries, 0), Some(1));
        assert_eq!(visual_index_for_selection(&entries, 1), Some(2));
        assert_eq!(visual_index_for_selection(&entries, 2), Some(3));
        assert_eq!(visual_index_for_selection(&entries, 3), Some(6));
        assert_eq!(visual_index_for_selection(&entries, 4), Some(9));
    }
}
