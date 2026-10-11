use super::*;
use crate::ui::personal_category_picker::{CategoryNameAction, CategorySurface};

const CATEGORY_MEMBERSHIP_CHUNK: usize = 40;

impl App {
    pub(super) fn apply_personal_category_action(&mut self, action: Action) {
        let (surface, message_ids) = match action {
            Action::OpenPersonalCategories => (CategorySurface::Catalog, Vec::new()),
            Action::AssignPersonalCategory => {
                let ids = self.mutation_target_ids();
                if ids.is_empty() {
                    self.status_message = Some("Select mail to assign a category".into());
                    return;
                }
                (CategorySurface::Assign, ids)
            }
            _ => return,
        };
        self.modals.personal_categories.open(surface, message_ids);
        let generation = self.modals.personal_categories.generation;
        self.pending_category_requests
            .push((generation, Request::ListCategories { account_id: None }));
    }

    pub(crate) fn handle_personal_category_result(
        &mut self,
        generation: u64,
        report_if_stale: bool,
        view_category_id: Option<mxr_core::CategoryId>,
        result: Result<Box<ResponseData>, MxrError>,
    ) {
        let state = &mut self.modals.personal_categories;
        if !state.visible || state.generation != generation {
            if report_if_stale {
                match result {
                    Ok(data) => match *data {
                        ResponseData::CategoryMembershipUndone { restored, skipped } => {
                            self.status_message = Some(if skipped == 0 {
                                format!("Restored {restored} category assignment(s)")
                            } else {
                                format!(
                                    "Restored {restored}; skipped {skipped} changed assignment(s)"
                                )
                            });
                        }
                        ResponseData::CategoryMembershipApplied {
                            mutation_id,
                            added,
                            removed,
                        } if added + removed > 0 => {
                            let count = added + removed;
                            self.pending_undo = Some(PendingUndo {
                                action: UndoAction::CategoryMembership(mutation_id),
                                verb_past: "Updated category assignments".into(),
                                count,
                                applied_at: std::time::Instant::now(),
                            });
                            self.status_message = Some(format!(
                                "Added {added}, removed {removed} category assignment(s) · u to undo"
                            ));
                        }
                        ResponseData::CategoryMembershipApplied { .. } => {
                            self.status_message =
                                Some("Category assignments were unchanged".into());
                        }
                        _ => {}
                    },
                    Err(error) => {
                        self.status_message = Some(format!("Category operation failed: {error}"));
                    }
                }
            }
            return;
        }
        match result {
            Err(error) => {
                state.loading = false;
                state.view_loading = false;
                state.name_preview_pending = false;
                state.membership_failed = state.surface == Some(CategorySurface::Assign);
                state.membership_chunks_pending = 0;
                state.error = Some(error.to_string());
                self.status_message = Some(format!("Category request failed: {error}"));
            }
            Ok(data) => match *data {
                ResponseData::Categories { categories } => {
                    let replacing_catalog = state.name_action.is_none();
                    state.set_categories(categories);
                    if replacing_catalog && state.surface == Some(CategorySurface::Assign) {
                        let category_ids = state
                            .categories
                            .iter()
                            .map(|category| category.id.clone())
                            .collect::<Vec<_>>();
                        let chunks = category_ids.chunks(CATEGORY_MEMBERSHIP_CHUNK);
                        let requests = chunks
                            .map(|chunk| Request::GetCategoryMemberships {
                                account_id: None,
                                message_ids: state.message_ids.clone(),
                                category_ids: chunk.to_vec(),
                            })
                            .collect::<Vec<_>>();
                        state.membership_chunks_pending = requests.len();
                        state.membership_failed = false;
                        if requests.is_empty() {
                            state.loading = false;
                        } else {
                            state.loading = true;
                            self.pending_category_requests
                                .extend(requests.into_iter().map(|request| (generation, request)));
                        }
                    } else if state.name_action.is_some() {
                        state.name_action = None;
                        state.name_preview_pending = false;
                        state.input.clear();
                        self.pending_category_requests
                            .push((generation, Request::ListCategories { account_id: None }));
                        self.status_message = Some("Category saved".into());
                    }
                }
                ResponseData::CategoryMembershipStates { states } => {
                    state.set_memberships(states);
                }
                ResponseData::CategoryNamePreview {
                    normalized_name,
                    available,
                } => {
                    if !available {
                        state.name_preview_pending = false;
                        state.error = Some(format!("‘{normalized_name}’ is already in use"));
                        return;
                    }
                    let Some(action) = state.name_action.clone() else {
                        return;
                    };
                    let name = normalized_name;
                    let request = match action {
                        CategoryNameAction::Create => Request::CreateCategory { name },
                        CategoryNameAction::Rename(category_id) => Request::RenameCategory {
                            category_id,
                            new_name: name,
                        },
                    };
                    self.pending_category_requests.push((generation, request));
                }
                ResponseData::CategoryMessages {
                    envelopes,
                    total,
                    has_more,
                } => {
                    if state.surface != Some(CategorySurface::View)
                        || state.viewing.as_ref() != view_category_id.as_ref()
                    {
                        return;
                    }
                    state.view_loading = false;
                    state.error = None;
                    state.view_total = total;
                    state.view_has_more = has_more;
                    state.view_messages.extend(envelopes);
                    state.selected = state
                        .selected
                        .min(state.view_messages.len().saturating_sub(1));
                }
                ResponseData::CategoryMembershipPreview { preview } => {
                    state.membership_preview = Some(preview);
                    state.error = None;
                }
                ResponseData::CategoryMembershipApplied {
                    mutation_id,
                    added,
                    removed,
                } => {
                    let count = added + removed;
                    state.membership_preview = None;
                    state.staged.clear();
                    if count > 0 {
                        self.pending_undo = Some(PendingUndo {
                            action: UndoAction::CategoryMembership(mutation_id),
                            verb_past: "Updated category assignments".into(),
                            count,
                            applied_at: std::time::Instant::now(),
                        });
                        self.status_message = Some(format!(
                            "Added {added}, removed {removed} category assignment(s) · u to undo"
                        ));
                    } else {
                        self.status_message = Some("Category assignments were unchanged".into());
                    }
                    let categories = state
                        .categories
                        .iter()
                        .map(|category| category.id.clone())
                        .collect::<Vec<_>>();
                    let ids = state.message_ids.clone();
                    state.memberships.clear();
                    if state.surface == Some(CategorySurface::Assign) && !categories.is_empty() {
                        let requests = categories
                            .chunks(CATEGORY_MEMBERSHIP_CHUNK)
                            .map(|chunk| Request::GetCategoryMemberships {
                                account_id: None,
                                message_ids: ids.clone(),
                                category_ids: chunk.to_vec(),
                            })
                            .collect::<Vec<_>>();
                        state.membership_chunks_pending = requests.len();
                        state.membership_failed = false;
                        state.loading = true;
                        self.pending_category_requests
                            .extend(requests.into_iter().map(|request| (generation, request)));
                    }
                }
                ResponseData::CategoryDeletePreview { preview } => {
                    state.delete_preview = Some(preview);
                    state.error = None;
                }
                ResponseData::CategoryDeleted { name, .. } => {
                    state.loading = false;
                    state.error = None;
                    state.delete_preview = None;
                    state.categories.retain(|category| category.name != name);
                    state.selected = state.selected.min(state.categories.len().saturating_sub(1));
                    self.status_message = Some(format!("Deleted category {name}"));
                }
                ResponseData::CategoryMembershipUndone { restored, skipped } => {
                    self.status_message = Some(if skipped == 0 {
                        format!("Restored {restored} category assignment(s)")
                    } else {
                        format!("Restored {restored}; skipped {skipped} changed assignment(s)")
                    });
                    let categories = state
                        .categories
                        .iter()
                        .map(|category| category.id.clone())
                        .collect::<Vec<_>>();
                    let ids = state.message_ids.clone();
                    if state.visible
                        && state.surface == Some(CategorySurface::Assign)
                        && !categories.is_empty()
                    {
                        state.memberships.clear();
                        let requests = categories
                            .chunks(CATEGORY_MEMBERSHIP_CHUNK)
                            .map(|chunk| Request::GetCategoryMemberships {
                                account_id: None,
                                message_ids: ids.clone(),
                                category_ids: chunk.to_vec(),
                            })
                            .collect::<Vec<_>>();
                        state.membership_chunks_pending = requests.len();
                        state.membership_failed = false;
                        state.loading = true;
                        self.pending_category_requests
                            .extend(requests.into_iter().map(|request| (generation, request)));
                    }
                }
                _ => {
                    state.loading = false;
                    state.view_loading = false;
                    state.error = Some("Unexpected response to category request".into());
                }
            },
        }
    }

    pub(crate) fn handle_personal_category_key(
        &mut self,
        key: ratatui::crossterm::event::KeyEvent,
    ) -> bool {
        use crate::ui::personal_category_picker::CategoryNameAction;
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};
        let state = &mut self.modals.personal_categories;
        if !state.visible {
            return false;
        }
        let generation = state.generation;
        if state.name_action.is_some() {
            if state.name_preview_pending && key.code != KeyCode::Esc {
                return true;
            }
            match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    state.generation = state.generation.wrapping_add(1);
                    state.name_action = None;
                    state.name_preview_pending = false;
                    state.input.clear();
                    state.error = None;
                }
                (KeyCode::Enter, _) if !state.name_preview_pending => {
                    if state.input.trim().is_empty() {
                        state.error = Some("Category name cannot be blank".into());
                    } else {
                        let category_id = match &state.name_action {
                            Some(CategoryNameAction::Rename(id)) => Some(id.clone()),
                            _ => None,
                        };
                        state.name_preview_pending = true;
                        state.error = None;
                        state.generation = state.generation.wrapping_add(1);
                        let name_generation = state.generation;
                        self.pending_category_requests.push((
                            name_generation,
                            Request::PreviewCategoryName {
                                category_id,
                                name: state.input.clone(),
                            },
                        ));
                    }
                }
                (KeyCode::Backspace, _) => {
                    state.input.pop();
                    state.error = None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) if !c.is_control() => {
                    state.input.push(c);
                    state.error = None;
                }
                _ => {}
            }
            return true;
        }
        if state.membership_preview.is_some() {
            match key.code {
                KeyCode::Esc => state.membership_preview = None,
                KeyCode::Enter => {
                    if let Some(preview) = state.membership_preview.take() {
                        self.pending_category_requests.push((
                            generation,
                            Request::ApplyCategoryMembership {
                                preview_token: preview.preview_token,
                            },
                        ));
                        state.close();
                    }
                }
                _ => {}
            }
            return true;
        }
        if state.delete_preview.is_some() {
            match key.code {
                KeyCode::Esc => state.delete_preview = None,
                KeyCode::Enter => {
                    if let Some(preview) = state.delete_preview.take() {
                        self.pending_category_requests.push((
                            generation,
                            Request::DeleteCategory {
                                preview_token: preview.preview_token,
                            },
                        ));
                        state.loading = true;
                    }
                }
                _ => {}
            }
            return true;
        }
        if state.filtering {
            match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    state.filtering = false;
                    state.input.clear();
                    state.selected = 0;
                    return true;
                }
                (KeyCode::Enter, _) => {
                    state.filtering = false;
                    return true;
                }
                (KeyCode::Backspace, _) => {
                    state.input.pop();
                    state.selected = 0;
                    return true;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) if !c.is_control() => {
                    state.input.push(c);
                    state.selected = 0;
                    return true;
                }
                _ => {}
            }
        }
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) if state.filtering => {
                state.filtering = false;
                state.input.clear();
                state.selected = 0;
            }
            (KeyCode::Esc | KeyCode::Char('q'), _) => {
                if state.surface == Some(CategorySurface::View) {
                    state.surface = Some(CategorySurface::Catalog);
                    state.viewing = None;
                    state.view_messages.clear();
                    state.selected = 0;
                } else {
                    state.close();
                }
            }
            (KeyCode::Down | KeyCode::Char('j'), _) => {
                let count = if state.surface == Some(CategorySurface::View) {
                    state.view_messages.len()
                } else {
                    state.visible_indices().len()
                };
                state.selected = (state.selected + 1).min(count.saturating_sub(1));
            }
            (KeyCode::Up | KeyCode::Char('k'), _) => {
                state.selected = state.selected.saturating_sub(1);
            }
            (KeyCode::PageDown, _)
                if state.surface == Some(CategorySurface::View)
                    && state.view_has_more
                    && !state.view_loading =>
            {
                if let Some(category_id) = state.viewing.clone() {
                    let offset = state.view_messages.len() as u32;
                    state.view_loading = true;
                    self.pending_category_requests.push((
                        generation,
                        Request::ListCategoryMessages {
                            category_id,
                            account_id: None,
                            limit: 200,
                            offset,
                        },
                    ));
                }
            }
            (KeyCode::Backspace, _)
                if state.surface == Some(CategorySurface::Assign) || state.filtering =>
            {
                state.input.pop();
                state.selected = 0;
            }
            (KeyCode::Char('/'), KeyModifiers::NONE)
                if state.surface != Some(CategorySurface::View) =>
            {
                state.input.clear();
                state.filtering = true;
            }
            (KeyCode::Char('n'), KeyModifiers::NONE)
                if matches!(
                    state.surface,
                    Some(CategorySurface::Catalog | CategorySurface::Assign)
                ) =>
            {
                state.input.clear();
                state.error = None;
                state.name_action = Some(CategoryNameAction::Create);
            }
            (KeyCode::Char('r'), KeyModifiers::NONE)
                if state.surface == Some(CategorySurface::Catalog) =>
            {
                if let Some((category_id, name)) = state
                    .selected_category()
                    .map(|category| (category.id.clone(), category.name.clone()))
                {
                    state.input = name;
                    state.name_action = Some(CategoryNameAction::Rename(category_id));
                    state.error = None;
                }
            }
            (KeyCode::Char('d'), KeyModifiers::NONE)
                if state.surface == Some(CategorySurface::Catalog) =>
            {
                if let Some(category) = state.selected_category() {
                    self.pending_category_requests.push((
                        generation,
                        Request::PreviewDeleteCategory {
                            category_id: category.id.clone(),
                        },
                    ));
                    state.loading = true;
                }
            }
            (KeyCode::Char('v'), KeyModifiers::NONE)
                if state.surface == Some(CategorySurface::Assign) =>
            {
                if let Some(category_id) = state
                    .selected_category()
                    .map(|category| category.id.clone())
                {
                    state.viewing = Some(category_id.clone());
                    state.surface = Some(CategorySurface::View);
                    state.view_loading = true;
                    state.selected = 0;
                    self.pending_category_requests.push((
                        generation,
                        Request::ListCategoryMessages {
                            category_id,
                            account_id: None,
                            limit: 200,
                            offset: 0,
                        },
                    ));
                }
            }
            (KeyCode::Enter, _) if state.surface == Some(CategorySurface::Assign) => {
                let mut add = Vec::new();
                let mut remove = Vec::new();
                for (category_id, assigned) in &state.staged {
                    let original = state
                        .memberships
                        .get(category_id)
                        .is_some_and(|membership| {
                            membership.selected_messages > 0
                                && membership.assigned_messages == membership.selected_messages
                        });
                    if *assigned && !original {
                        add.push(category_id.clone());
                    }
                    let originally_assigned = state
                        .memberships
                        .get(category_id)
                        .is_some_and(|membership| membership.assigned_messages > 0);
                    if !*assigned && originally_assigned {
                        remove.push(category_id.clone());
                    }
                }
                if !add.is_empty() || !remove.is_empty() {
                    self.pending_category_requests.push((
                        generation,
                        Request::PreviewCategoryMembership {
                            account_id: None,
                            message_ids: state.message_ids.clone(),
                            add_category_ids: add,
                            remove_category_ids: remove,
                        },
                    ));
                    state.loading = true;
                }
            }
            (KeyCode::Enter, _) if state.surface == Some(CategorySurface::Catalog) => {
                if let Some(category_id) = state
                    .selected_category()
                    .map(|category| category.id.clone())
                {
                    state.viewing = Some(category_id.clone());
                    state.surface = Some(CategorySurface::View);
                    state.view_loading = true;
                    state.selected = 0;
                    self.pending_category_requests.push((
                        generation,
                        Request::ListCategoryMessages {
                            category_id,
                            account_id: None,
                            limit: 200,
                            offset: 0,
                        },
                    ));
                }
            }
            (KeyCode::Enter, _) if state.surface == Some(CategorySurface::View) => {
                if let Some(envelope) = state.view_messages.get(state.selected).cloned() {
                    state.close();
                    self.open_envelope(envelope);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                    self.screen = Screen::Mailbox;
                }
            }
            (KeyCode::Char(' '), KeyModifiers::NONE)
                if state.surface == Some(CategorySurface::Assign) =>
            {
                state.toggle_selected();
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT)
                if !c.is_control() && state.filtering =>
            {
                state.input.push(c);
                state.selected = 0;
            }
            _ => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn category(id: mxr_core::CategoryId, name: &str) -> PersonalCategory {
        PersonalCategory {
            id,
            name: name.into(),
            unread_count: 0,
            total_count: 0,
        }
    }

    #[test]
    fn assignment_uses_the_same_selected_message_ids_for_preview_and_apply() {
        let mut app = App::new();
        let ids = [MessageId::new(), MessageId::new()];
        let category_id = mxr_core::CategoryId::new();
        app.mailbox.selected_set.extend(ids.iter().cloned());
        app.apply(Action::AssignPersonalCategory);
        let generation = app.modals.personal_categories.generation;
        assert!(matches!(
            app.pending_category_requests
                .pop()
                .map(|(_, request)| request),
            Some(Request::ListCategories { .. })
        ));

        app.handle_personal_category_result(
            generation,
            false,
            None,
            Ok(Box::new(ResponseData::Categories {
                categories: vec![category(category_id.clone(), "Travel")],
            })),
        );
        let membership_request = app.pending_category_requests.pop().unwrap().1;
        assert!(matches!(
            membership_request,
            Request::GetCategoryMemberships { .. }
        ));
        app.handle_personal_category_result(
            generation,
            false,
            None,
            Ok(Box::new(ResponseData::CategoryMembershipStates {
                states: vec![],
            })),
        );

        app.modals.personal_categories.toggle_selected();
        assert!(app.handle_personal_category_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        let Request::PreviewCategoryMembership {
            message_ids: preview_ids,
            add_category_ids,
            remove_category_ids,
            ..
        } = app.pending_category_requests.pop().unwrap().1
        else {
            panic!("expected membership preview");
        };
        assert_eq!(
            preview_ids
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
            ids.into_iter().collect()
        );
        assert_eq!(add_category_ids, vec![category_id]);
        assert!(remove_category_ids.is_empty());

        app.handle_personal_category_result(
            generation,
            false,
            None,
            Ok(Box::new(ResponseData::CategoryMembershipPreview {
                preview: mxr_protocol::CategoryMembershipPreview {
                    category_ids: add_category_ids,
                    message_count: 2,
                    added: 2,
                    removed: 0,
                    preview_token: "preview".into(),
                },
            })),
        );
        assert!(app.handle_personal_category_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!app.modals.personal_categories.visible);
        assert!(matches!(
            app.pending_category_requests.pop().map(|(_, request)| request),
            Some(Request::ApplyCategoryMembership { preview_token }) if preview_token == "preview"
        ));
        app.handle_personal_category_result(
            generation,
            true,
            None,
            Ok(Box::new(ResponseData::CategoryMembershipApplied {
                mutation_id: "membership-mutation".into(),
                added: 2,
                removed: 0,
            })),
        );
        assert!(matches!(
            app.pending_undo.as_ref().map(|undo| &undo.action),
            Some(UndoAction::CategoryMembership(id)) if id == "membership-mutation"
        ));
        let previous_undo = app.pending_undo.clone();
        app.handle_personal_category_result(
            generation,
            true,
            None,
            Ok(Box::new(ResponseData::CategoryMembershipApplied {
                mutation_id: "empty-mutation".into(),
                added: 0,
                removed: 0,
            })),
        );
        assert!(matches!(
            (app.pending_undo.as_ref(), previous_undo.as_ref()),
            (Some(current), Some(previous)) if current.action == previous.action
        ));
    }

    #[test]
    fn closing_category_modal_discards_late_list_responses() {
        let mut app = App::new();
        app.apply(Action::OpenPersonalCategories);
        let generation = app.modals.personal_categories.generation;
        app.modals.personal_categories.close();
        app.handle_personal_category_result(
            generation,
            false,
            None,
            Ok(Box::new(ResponseData::Categories {
                categories: vec![category(mxr_core::CategoryId::new(), "Private")],
            })),
        );
        assert!(app.modals.personal_categories.categories.is_empty());
        assert!(!app.modals.personal_categories.visible);
    }

    #[test]
    fn catalog_filter_selects_from_visible_categories() {
        let mut picker = crate::ui::personal_category_picker::PersonalCategoryPicker::default();
        picker.open(CategorySurface::Catalog, Vec::new());
        picker.set_categories(vec![
            category(mxr_core::CategoryId::new(), "Home"),
            category(mxr_core::CategoryId::new(), "Travel"),
        ]);
        picker.input = "trav".into();
        assert_eq!(picker.selected_category().unwrap().name, "Travel");
    }

    #[test]
    fn category_view_rejects_a_late_response_for_another_category() {
        let mut app = App::new();
        app.apply(Action::OpenPersonalCategories);
        let generation = app.modals.personal_categories.generation;
        let current = mxr_core::CategoryId::new();
        let late = mxr_core::CategoryId::new();
        app.modals.personal_categories.surface = Some(CategorySurface::View);
        app.modals.personal_categories.viewing = Some(current.clone());
        app.modals.personal_categories.view_loading = true;
        app.handle_personal_category_result(
            generation,
            false,
            Some(late),
            Ok(Box::new(ResponseData::CategoryMessages {
                envelopes: vec![],
                total: 0,
                has_more: false,
            })),
        );
        assert!(app.modals.personal_categories.view_loading);
        assert!(app.modals.personal_categories.view_messages.is_empty());
        assert_eq!(app.modals.personal_categories.viewing, Some(current));
    }

    fn press(app: &mut App, code: KeyCode) {
        assert!(app.handle_personal_category_key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    #[test]
    fn filter_text_takes_precedence_over_catalog_and_assignment_shortcuts() {
        let mut app = App::new();
        app.apply(Action::OpenPersonalCategories);
        app.modals
            .personal_categories
            .set_categories(vec![category(mxr_core::CategoryId::new(), "Travel")]);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.modals.personal_categories.input, "r");
        assert!(app.modals.personal_categories.name_action.is_none());

        app.modals.personal_categories.close();
        app.modals
            .personal_categories
            .open(CategorySurface::Assign, vec![MessageId::new()]);
        app.modals
            .personal_categories
            .set_categories(vec![category(mxr_core::CategoryId::new(), "Travel")]);
        press(&mut app, KeyCode::Char('/'));
        for letter in "Travel".chars() {
            press(&mut app, KeyCode::Char(letter));
        }
        assert_eq!(app.modals.personal_categories.input, "Travel");
        assert_eq!(
            app.modals.personal_categories.surface,
            Some(CategorySurface::Assign)
        );
    }

    #[test]
    fn assignment_filter_enter_finishes_search_before_staging_and_preview() {
        let mut app = App::new();
        let category_id = mxr_core::CategoryId::new();
        app.modals
            .personal_categories
            .open(CategorySurface::Assign, vec![MessageId::new()]);
        app.pending_category_requests.clear();
        app.modals
            .personal_categories
            .set_categories(vec![category(category_id.clone(), "Travel")]);
        press(&mut app, KeyCode::Char('/'));
        for letter in "Travel".chars() {
            press(&mut app, KeyCode::Char(letter));
        }
        press(&mut app, KeyCode::Enter);
        assert!(!app.modals.personal_categories.filtering);
        assert_eq!(
            app.modals
                .personal_categories
                .selected_category()
                .unwrap()
                .name,
            "Travel"
        );
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(
            app.modals.personal_categories.staged.get(&category_id),
            Some(&true)
        );
        press(&mut app, KeyCode::Enter);
        assert!(matches!(
            app.pending_category_requests.pop().map(|(_, request)| request),
            Some(Request::PreviewCategoryMembership { add_category_ids, .. })
                if add_category_ids == vec![category_id]
        ));
    }

    #[test]
    fn removing_a_staged_partial_membership_previews_its_removal() {
        let mut app = App::new();
        app.modals
            .personal_categories
            .open(CategorySurface::Assign, vec![MessageId::new()]);
        let category_id = mxr_core::CategoryId::new();
        let picker = &mut app.modals.personal_categories;
        picker.set_categories(vec![category(category_id.clone(), "Travel")]);
        picker.memberships.insert(
            category_id.clone(),
            mxr_core::types::CategoryMembershipState {
                category_id: category_id.clone(),
                selected_messages: 2,
                assigned_messages: 1,
            },
        );
        picker.staged.insert(category_id.clone(), false);
        press(&mut app, KeyCode::Enter);
        let Request::PreviewCategoryMembership {
            add_category_ids,
            remove_category_ids,
            ..
        } = app.pending_category_requests.pop().unwrap().1
        else {
            panic!("expected membership preview");
        };
        assert!(add_category_ids.is_empty());
        assert_eq!(remove_category_ids, vec![category_id]);
    }

    #[test]
    fn late_name_preview_cannot_apply_to_a_new_name_action() {
        let mut app = App::new();
        app.apply(Action::OpenPersonalCategories);
        app.pending_category_requests.clear();
        press(&mut app, KeyCode::Char('n'));
        for letter in "Alpha".chars() {
            press(&mut app, KeyCode::Char(letter));
        }
        press(&mut app, KeyCode::Enter);
        let (old_generation, old_request) = app.pending_category_requests.pop().unwrap();
        assert!(matches!(old_request, Request::PreviewCategoryName { .. }));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.modals.personal_categories.input, "Alpha");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('n'));
        for letter in "Beta".chars() {
            press(&mut app, KeyCode::Char(letter));
        }

        app.handle_personal_category_result(
            old_generation,
            false,
            None,
            Ok(Box::new(ResponseData::CategoryNamePreview {
                normalized_name: "Alpha".into(),
                available: true,
            })),
        );
        assert_eq!(app.modals.personal_categories.input, "Beta");
        assert!(matches!(
            app.modals.personal_categories.name_action,
            Some(CategoryNameAction::Create)
        ));
        assert!(app.pending_category_requests.is_empty());
    }
}
