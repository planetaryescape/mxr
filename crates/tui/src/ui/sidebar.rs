use crate::app::{ActivePane, SidebarSection};
use mxr_core::types::{Label, LabelKind, SavedSearch};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    labels: &[Label],
    active_pane: &ActivePane,
    saved_searches: &[SavedSearch],
    sidebar_section: &SidebarSection,
    sidebar_selected: usize,
    active_label: Option<&mxr_core::LabelId>,
) {
    let is_focused = *active_pane == ActivePane::Sidebar;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Labels section
    let inner_width = chunks[0].width.saturating_sub(2) as usize;

    let visible_labels: Vec<&Label> = labels
        .iter()
        .filter(|l| !should_hide_label(&l.name))
        .collect();

    let mut system_labels: Vec<&Label> = visible_labels
        .iter()
        .filter(|l| l.kind == LabelKind::System)
        .filter(|l| {
            is_primary_system_label(&l.name) || l.total_count > 0 || l.unread_count > 0
        })
        .copied()
        .collect();
    system_labels.sort_by_key(|l| system_label_order(&l.name));

    let mut user_labels: Vec<&Label> = visible_labels
        .iter()
        .filter(|l| l.kind != LabelKind::System)
        .copied()
        .collect();
    user_labels.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Build visual items list (system + separator + user)
    let mut items: Vec<ListItem> = Vec::new();
    let system_count = system_labels.len();
    let has_user_labels = !user_labels.is_empty();

    for label in &system_labels {
        items.push(render_label_item(label, inner_width, active_label));
    }

    if has_user_labels {
        items.push(ListItem::new(
            Line::from(Span::styled(
                "  ─────────",
                Style::default().fg(Color::Rgb(60, 60, 70)),
            )),
        ));
    }

    for label in &user_labels {
        items.push(render_label_item(label, inner_width, active_label));
    }

    let labels_focused = is_focused && matches!(sidebar_section, SidebarSection::Labels);

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Labels ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(50, 50, 60))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    if labels_focused {
        // Map sidebar_selected (index into logical label list) to visual index
        // (accounting for separator)
        let visual_index = if has_user_labels && sidebar_selected >= system_count {
            sidebar_selected + 1 // +1 for separator
        } else {
            sidebar_selected
        };
        let mut state = ListState::default().with_selected(Some(visual_index));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    } else {
        frame.render_widget(list, chunks[0]);
    }

    // Saved searches section
    let searches_focused = is_focused && matches!(sidebar_section, SidebarSection::SavedSearches);

    let saved_items: Vec<ListItem> = if saved_searches.is_empty() {
        vec![ListItem::new("  No saved searches").style(Style::default().fg(Color::DarkGray))]
    } else {
        saved_searches
            .iter()
            .map(|s| ListItem::new(format!("  {}", s.name)))
            .collect()
    };

    let saved_list = List::new(saved_items)
        .block(
            Block::default()
                .title(" Saved Searches ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(50, 50, 60))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    if searches_focused && !saved_searches.is_empty() {
        let mut state = ListState::default().with_selected(Some(sidebar_selected));
        frame.render_stateful_widget(saved_list, chunks[1], &mut state);
    } else {
        frame.render_widget(saved_list, chunks[1]);
    }
}

fn render_label_item<'a>(
    label: &Label,
    inner_width: usize,
    active_label: Option<&mxr_core::LabelId>,
) -> ListItem<'a> {
    let is_active = active_label.map(|al| al == &label.id).unwrap_or(false);
    let display_name = humanize_label(&label.name);
    let prefix = if is_active { "▸ " } else { "  " };

    let count_str = if label.unread_count > 0 {
        format!("{}/{}", label.unread_count, label.total_count)
    } else if label.total_count > 0 {
        format!("{}", label.total_count)
    } else {
        String::new()
    };

    let name_width = inner_width.saturating_sub(2 + count_str.len() + 1);
    let line = if count_str.is_empty() {
        format!("{}{}", prefix, display_name)
    } else {
        format!("{}{:<nw$} {}", prefix, display_name, count_str, nw = name_width)
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
    matches!(name, "INBOX" | "STARRED" | "SENT" | "DRAFT" | "SPAM" | "TRASH")
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
