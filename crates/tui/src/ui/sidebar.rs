use crate::app::ActivePane;
use mxr_core::types::{Label, LabelKind, SavedSearch};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct SidebarView<'a> {
    pub labels: &'a [Label],
    pub active_pane: &'a ActivePane,
    pub saved_searches: &'a [SavedSearch],
    pub sidebar_selected: usize,
    pub active_label: Option<&'a mxr_core::LabelId>,
}

#[derive(Debug, Clone)]
enum SidebarEntry<'a> {
    Spacer,
    Header(&'static str),
    Label(&'a Label),
    SavedSearch(&'a SavedSearch),
}

pub fn draw(frame: &mut Frame, area: Rect, view: &SidebarView<'_>) {
    let is_focused = *view.active_pane == ActivePane::Sidebar;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let inner_width = area.width.saturating_sub(2) as usize;
    let entries = build_sidebar_entries(view.labels, view.saved_searches);
    let selected_visual_index = visual_index_for_selection(&entries, view.sidebar_selected);

    let items = entries
        .iter()
        .map(|entry| match entry {
            SidebarEntry::Spacer => ListItem::new(Line::from("")),
            SidebarEntry::Header(title) => ListItem::new(Line::from(Span::styled(
                *title,
                Style::default().fg(Color::Cyan).bold(),
            ))),
            SidebarEntry::Label(label) => render_label_item(label, inner_width, view.active_label),
            SidebarEntry::SavedSearch(search) => ListItem::new(format!("  {}", search.name)),
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Sidebar ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(50, 50, 60))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

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

    let mut entries = Vec::new();
    entries.extend(system_labels.into_iter().map(SidebarEntry::Label));

    if !user_labels.is_empty() {
        if !entries.is_empty() {
            entries.push(SidebarEntry::Spacer);
        }
        entries.push(SidebarEntry::Header(" Labels"));
        entries.extend(user_labels.into_iter().map(SidebarEntry::Label));
    }

    if !saved_searches.is_empty() {
        if !entries.is_empty() {
            entries.push(SidebarEntry::Spacer);
        }
        entries.push(SidebarEntry::Header(" Saved Searches"));
        entries.extend(saved_searches.iter().map(SidebarEntry::SavedSearch));
    }

    entries
}

fn visual_index_for_selection(entries: &[SidebarEntry<'_>], sidebar_selected: usize) -> Option<usize> {
    let mut selectable = 0usize;
    for (visual_index, entry) in entries.iter().enumerate() {
        match entry {
            SidebarEntry::Label(_) | SidebarEntry::SavedSearch(_) => {
                if selectable == sidebar_selected {
                    return Some(visual_index);
                }
                selectable += 1;
            }
            SidebarEntry::Spacer | SidebarEntry::Header(_) => {}
        }
    }
    None
}

fn render_label_item<'a>(
    label: &Label,
    inner_width: usize,
    active_label: Option<&mxr_core::LabelId>,
) -> ListItem<'a> {
    let is_active = active_label.map(|current| current == &label.id).unwrap_or(false);
    let display_name = humanize_label(&label.name);
    let prefix = if is_active { "▸ " } else { "  " };

    let count_str = if label.unread_count > 0 {
        format!("{}/{}", label.unread_count, label.total_count)
    } else if label.total_count > 0 {
        label.total_count.to_string()
    } else {
        String::new()
    };

    let name_width = inner_width.saturating_sub(2 + count_str.len() + 1);
    let line = if count_str.is_empty() {
        format!("{prefix}{display_name}")
    } else {
        format!("{prefix}{display_name:<name_width$} {count_str}")
    };

    let style = if is_active {
        Style::default().fg(Color::Cyan).bold()
    } else if label.unread_count > 0 {
        Style::default().fg(Color::White).bold()
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
        "INBOX" | "STARRED" | "SENT" | "DRAFT" | "SPAM" | "TRASH"
    )
}

pub fn system_label_order(name: &str) -> usize {
    match name {
        "INBOX" => 0,
        "STARRED" => 1,
        "SENT" => 2,
        "DRAFT" => 3,
        "SPAM" => 4,
        "TRASH" => 5,
        _ => 100,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, LabelId, SavedSearchId};
    use mxr_core::types::SortOrder;

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
        let labels = vec![label("INBOX", LabelKind::System), label("Work", LabelKind::User)];
        let entries = build_sidebar_entries(&labels, &[]);
        assert!(matches!(entries[0], SidebarEntry::Label(label) if label.name == "INBOX"));
        assert!(matches!(entries[1], SidebarEntry::Spacer));
        assert!(matches!(entries[2], SidebarEntry::Header(" Labels")));
        assert!(matches!(entries[3], SidebarEntry::Label(label) if label.name == "Work"));
    }

    #[test]
    fn sidebar_selection_skips_headers_and_spacers() {
        let labels = vec![label("INBOX", LabelKind::System), label("Work", LabelKind::User)];
        let searches = vec![SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Unread".into(),
            query: "is:unread".into(),
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        }];
        let entries = build_sidebar_entries(&labels, &searches);
        assert_eq!(visual_index_for_selection(&entries, 0), Some(0));
        assert_eq!(visual_index_for_selection(&entries, 1), Some(3));
        assert_eq!(visual_index_for_selection(&entries, 2), Some(6));
    }
}
