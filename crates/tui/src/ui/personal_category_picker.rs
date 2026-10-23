use mxr_core::id::{CategoryId, MessageId};
use mxr_core::types::{CategoryMembershipState, Envelope, PersonalCategory};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategorySurface {
    Assign,
    Catalog,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryNameAction {
    Create,
    Rename(CategoryId),
}

#[derive(Debug, Default)]
pub struct PersonalCategoryPicker {
    pub visible: bool,
    pub surface: Option<CategorySurface>,
    pub generation: u64,
    pub loading: bool,
    pub error: Option<String>,
    pub why: Option<Vec<String>>,
    pub input: String,
    pub filtering: bool,
    pub categories: Vec<PersonalCategory>,
    pub selected: usize,
    pub list_state: ListState,
    pub message_ids: Vec<MessageId>,
    pub memberships: HashMap<CategoryId, CategoryMembershipState>,
    pub staged: HashMap<CategoryId, bool>,
    pub membership_chunks_pending: usize,
    pub membership_failed: bool,
    pub membership_preview: Option<mxr_protocol::CategoryMembershipPreview>,
    pub delete_preview: Option<mxr_protocol::CategoryDeletePreview>,
    pub name_action: Option<CategoryNameAction>,
    pub name_preview_pending: bool,
    pub viewing: Option<CategoryId>,
    pub view_loading: bool,
    pub view_messages: Vec<Envelope>,
    pub view_total: u32,
    pub view_has_more: bool,
}

impl PersonalCategoryPicker {
    pub fn open(&mut self, surface: CategorySurface, message_ids: Vec<MessageId>) {
        let generation = self.generation.wrapping_add(1);
        *self = Self {
            visible: true,
            surface: Some(surface),
            generation,
            loading: true,
            message_ids,
            ..Self::default()
        };
    }

    pub fn close(&mut self) {
        let generation = self.generation.wrapping_add(1);
        *self = Self {
            generation,
            ..Self::default()
        };
    }

    pub fn set_categories(&mut self, categories: Vec<PersonalCategory>) {
        self.loading = false;
        self.error = None;
        self.categories = categories;
        self.selected = self
            .selected
            .min(self.visible_indices().len().saturating_sub(1));
        self.list_state.select(Some(self.selected));
    }

    pub fn set_memberships(&mut self, memberships: Vec<CategoryMembershipState>) {
        for state in memberships {
            self.memberships.insert(state.category_id.clone(), state);
        }
        self.membership_chunks_pending = self.membership_chunks_pending.saturating_sub(1);
        if self.membership_chunks_pending == 0 {
            self.loading = false;
            if !self.membership_failed {
                self.error = None;
            }
        }
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let needle = self.input.trim().to_lowercase();
        self.categories
            .iter()
            .enumerate()
            .filter_map(|(index, category)| {
                (needle.is_empty() || category.name.to_lowercase().contains(&needle))
                    .then_some(index)
            })
            .collect()
    }

    pub fn toggle_selected(&mut self) {
        let Some(category) = self.selected_category().cloned() else {
            return;
        };
        let state = self.memberships.get(&category.id);
        let current = self.staged.get(&category.id).copied().unwrap_or_else(|| {
            state.is_some_and(|state| {
                state.selected_messages > 0 && state.assigned_messages == state.selected_messages
            })
        });
        self.staged.insert(category.id, !current);
        self.membership_preview = None;
    }

    pub fn selected_category(&self) -> Option<&PersonalCategory> {
        self.visible_indices()
            .get(self.selected)
            .and_then(|index| self.categories.get(*index))
    }
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    picker: &PersonalCategoryPicker,
    theme: &crate::theme::Theme,
) {
    if !picker.visible || area.width < 2 || area.height < 2 {
        return;
    }
    let width = area.width.min(76);
    let height = area.height.min(24);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let title = match picker.surface {
        Some(CategorySurface::Catalog) => {
            " Categories · c new · r rename · # delete · Enter view · Esc back "
        }
        Some(CategorySurface::View) => " Category mail · Enter open · w Why · Esc back ",
        _ => " Assign Category · Space stage · Enter preview · w Why · c new · Esc ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.success));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(why) = &picker.why {
        let mut lines = vec![Line::from("Why this category · Esc back")];
        lines.extend(why.iter().cloned().map(Line::from));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        return;
    }
    if let Some(action) = &picker.name_action {
        let verb = match action {
            CategoryNameAction::Create => "New category",
            CategoryNameAction::Rename(_) => "Rename category",
        };
        let lines = vec![
            Line::from(verb),
            Line::from(format!("Name: {}", picker.input)),
            Line::from("Enter saves after validation · Esc cancels"),
            picker.error.as_deref().map_or_else(
                || Line::from(" "),
                |error| Line::from(Span::styled(error, Style::default().fg(theme.error))),
            ),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        return;
    }

    if let Some(preview) = &picker.membership_preview {
        let mut lines = vec![
            Line::from(format!(
                "{} message(s): {} added, {} removed",
                preview.message_count, preview.added, preview.removed
            )),
            Line::from("Enter applies this preview · Esc cancels"),
        ];
        lines.extend(preview.effects.iter().map(|e| {
            Line::from(format!(
                "{}: {} → {} · {}",
                e.message_id,
                if e.before { "Assigned" } else { "Unassigned" },
                if e.after { "Assigned" } else { "Unassigned" },
                e.reason
            ))
        }));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        return;
    }
    if let Some(preview) = &picker.delete_preview {
        frame.render_widget(
            Paragraph::new(format!(
                "Delete ‘{}’? This removes it from {} message(s), not the mail.\nEnter confirms this preview · Esc cancels",
                preview.name, preview.affected_messages
            ))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    if picker.surface == Some(CategorySurface::View) {
        if picker.view_loading {
            frame.render_widget(Paragraph::new("Loading category mail…"), inner);
            return;
        }
        if let Some(error) = &picker.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(theme.error)),
                inner,
            );
            return;
        }
        let name = picker
            .viewing
            .as_ref()
            .and_then(|id| picker.categories.iter().find(|category| &category.id == id))
            .map_or("Category", |category| category.name.as_str());
        let header = Line::from(Span::styled(
            format!(
                "{name} · {} of {} message(s){}",
                picker.view_messages.len(),
                picker.view_total,
                if picker.view_has_more {
                    " · PageDown for more"
                } else {
                    ""
                }
            ),
            Style::default().fg(theme.text_primary).bold(),
        ));
        if picker.view_messages.is_empty() {
            frame.render_widget(
                Paragraph::new(vec![header, Line::from("No messages in this category")])
                    .wrap(Wrap { trim: true }),
                inner,
            );
            return;
        }
        let header_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(Paragraph::new(header), header_area);
        let list_area = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
        let items = picker
            .view_messages
            .iter()
            .map(|mail| ListItem::new(format!("{}  {}", mail.from.email, mail.subject)))
            .collect::<Vec<_>>();
        let mut list_state = picker.list_state;
        list_state.select(Some(picker.selected));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_style(theme.highlight_style())
                .highlight_symbol("> "),
            list_area,
            &mut list_state,
        );
        return;
    }

    if inner.height > 1 {
        let filter = if picker.surface == Some(CategorySurface::Assign) {
            format!(
                "{} selected · Filter: {}",
                picker.message_ids.len(),
                picker.input
            )
        } else {
            format!("Filter: {}", picker.input)
        };
        frame.render_widget(
            Paragraph::new(filter),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }
    let list_area = Rect::new(
        inner.x,
        inner.y + u16::from(inner.height > 1),
        inner.width,
        inner.height.saturating_sub(u16::from(inner.height > 1)),
    );
    if picker.loading || picker.view_loading {
        frame.render_widget(
            Paragraph::new("Loading categories and assignments…"),
            list_area,
        );
        return;
    }
    if let Some(error) = &picker.error {
        frame.render_widget(
            Paragraph::new(error.as_str()).style(Style::default().fg(theme.error)),
            list_area,
        );
        return;
    }
    let items = picker
        .visible_indices()
        .iter()
        .map(|index| {
            let category = &picker.categories[*index];
            let state = picker.memberships.get(&category.id);
            let full = state.is_some_and(|state| {
                state.selected_messages > 0 && state.assigned_messages == state.selected_messages
            });
            let staged = picker.staged.get(&category.id).copied();
            let assigned = staged.unwrap_or(full);
            let marker = if picker.surface == Some(CategorySurface::Assign) {
                if staged == Some(true) || (staged.is_none() && assigned) {
                    "[x]"
                } else if staged.is_none() && state.is_some_and(|state| state.assigned_messages > 0)
                {
                    "[-]"
                } else {
                    "[ ]"
                }
            } else {
                "   "
            };
            let count = if picker.surface == Some(CategorySurface::Catalog) {
                format!("{}", category.total_count)
            } else {
                state.map_or(0, |state| state.assigned_messages).to_string()
            };
            let line = format!(
                "{marker} {:<1$}  {count}",
                category.name,
                (list_area.width as usize).saturating_sub(10)
            );
            ListItem::new(line)
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        let message = if picker.surface == Some(CategorySurface::Assign) {
            "No categories yet · press n to create one"
        } else {
            "No categories yet · n creates one"
        };
        frame.render_widget(Paragraph::new(message), list_area);
    } else {
        let mut list_state = picker.list_state;
        list_state.select(Some(picker.selected));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_style(theme.highlight_style())
                .highlight_symbol("> "),
            list_area,
            &mut list_state,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(picker: &PersonalCategoryPicker) -> String {
        let theme = crate::theme::Theme::default();
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))
            .expect("test terminal");
        terminal
            .draw(|frame| draw(frame, frame.area(), picker, &theme))
            .expect("render categories");
        let buffer = terminal.backend().buffer();
        (0..24)
            .map(|row| {
                (0..80)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn assignment_picker_is_readable_at_eighty_columns() {
        let mut picker = PersonalCategoryPicker::default();
        picker.open(CategorySurface::Assign, vec![MessageId::new()]);
        picker.set_categories(vec![PersonalCategory {
            id: CategoryId::new(),
            name: "Travel".into(),
            unread_count: 0,
            total_count: 0,
        }]);
        picker.loading = false;
        let rendered = render(&picker);
        assert!(rendered.contains("Travel"), "{rendered}");
        assert!(rendered.contains("Assign Category"), "{rendered}");
    }

    #[test]
    fn catalog_selection_scrolls_past_the_first_viewport() {
        let mut picker = PersonalCategoryPicker::default();
        picker.open(CategorySurface::Catalog, Vec::new());
        picker.set_categories(
            (0..43)
                .map(|index| PersonalCategory {
                    id: CategoryId::new(),
                    name: format!("Category {index:02}"),
                    unread_count: 0,
                    total_count: 0,
                })
                .collect(),
        );
        picker.selected = 35;
        let rendered = render(&picker);
        assert!(rendered.contains("Category 35"), "{rendered}");
        assert!(!rendered.contains("Category 00"), "{rendered}");
    }

    #[test]
    fn category_messages_scroll_and_highlight_the_selected_row() {
        let mut picker = PersonalCategoryPicker::default();
        let category_id = CategoryId::new();
        picker.open(CategorySurface::View, Vec::new());
        picker.surface = Some(CategorySurface::View);
        picker.categories = vec![PersonalCategory {
            id: category_id.clone(),
            name: "Travel".into(),
            unread_count: 0,
            total_count: 40,
        }];
        picker.viewing = Some(category_id);
        picker.view_messages = (0..40)
            .map(|index| {
                crate::test_fixtures::TestEnvelopeBuilder::new()
                    .subject(format!("Subject {index:02}"))
                    .build()
            })
            .collect();
        picker.view_total = 40;
        picker.selected = 28;
        let rendered = render(&picker);
        assert!(rendered.contains("Subject 28"), "{rendered}");
        assert!(!rendered.contains("Subject 00"), "{rendered}");
        assert!(rendered.contains("> "), "{rendered}");
    }

    #[test]
    fn staged_removal_renders_unchecked_even_for_partial_membership() {
        let mut picker = PersonalCategoryPicker::default();
        let category_id = CategoryId::new();
        picker.open(CategorySurface::Assign, vec![MessageId::new()]);
        picker.set_categories(vec![PersonalCategory {
            id: category_id.clone(),
            name: "Travel".into(),
            unread_count: 0,
            total_count: 0,
        }]);
        picker.memberships.insert(
            category_id.clone(),
            CategoryMembershipState {
                category_id: category_id.clone(),
                selected_messages: 2,
                assigned_messages: 1,
            },
        );
        picker.staged.insert(category_id, false);
        let rendered = render(&picker);
        assert!(rendered.contains("[ ] Travel"), "{rendered}");
        assert!(!rendered.contains("[-] Travel"), "{rendered}");
    }
}
