//! `b`, Not now (blueprint 35, D173): one picker everywhere. "Later (no
//! time)" comes first, then the snooze presets on digits, then a typed
//! time. What Later and a time do depends on where `b` was pressed, and
//! each sends the request that place always sent (`NotNowTarget`).

use super::*;
use crate::app::state::NotNowTarget;

impl App {
    /// What `b` acts on where the cursor is, or `None` with nothing to act on.
    pub(crate) fn not_now_target(&self) -> Option<NotNowTarget> {
        if self.mailbox.mailbox_view == MailboxView::Focus {
            return self
                .focus_not_now_message()
                .map(|message_id| NotNowTarget::Mail { message_id });
        }
        if self.screen == Screen::Mailbox && self.mailbox.mailbox_view == MailboxView::Todo {
            return self.selected_todo().map(|todo| NotNowTarget::Todo {
                todo_id: todo.id.clone(),
            });
        }
        if self.screen == Screen::Mailbox && self.mailbox.mailbox_view == MailboxView::Reading {
            let row = self.selected_reading_row()?;
            let issue = row.issue();
            return Some(NotNowTarget::Reading {
                item_key: row.key().to_string(),
                message_id: issue.message_id.clone(),
                on_later: match row {
                    crate::app::ReadingRow::Item(item) => item.on_later,
                    crate::app::ReadingRow::Link(_, link) => link.on_later,
                },
                link: row.is_link(),
                domain: row.domain().map(str::to_string),
            });
        }
        if self.now_list_focused() {
            let message_id = match self.selected_now_row()? {
                NowRow::Person(person) => person.row.message_id.clone(),
                NowRow::NotSure(question) => question.message_id.clone(),
                NowRow::Reading(pick) => pick.message_id.clone(),
                NowRow::Todo(todo) => todo.todo.source_message_id.clone()?,
                NowRow::Arrivals(_) | NowRow::Updates(_) => return None,
            };
            return Some(NotNowTarget::Mail { message_id });
        }
        if self.messages_list_focused() || self.desk_list_focused() {
            let (thread, message_id, waiting) = self.reply_later_target()?;
            return Some(NotNowTarget::Conversation {
                thread,
                message_id,
                waiting,
            });
        }
        self.context_envelope().map(|envelope| NotNowTarget::Mail {
            message_id: envelope.id.clone(),
        })
    }

    /// `b`: open the picker for what the cursor is on.
    pub(super) fn open_not_now(&mut self) {
        let Some(target) = self.not_now_target() else {
            self.status_message = Some(
                if self.now_list_focused() {
                    "This row has no single email to put off"
                } else {
                    "No message selected"
                }
                .into(),
            );
            return;
        };
        let panel = &mut self.modals.snooze_panel;
        panel.visible = true;
        panel.target = Some(target);
        panel.selected_index = 0;
        panel.custom_input = None;
        panel.custom_error = None;
        panel.custom_time.clear();
    }

    pub(super) fn close_not_now(&mut self) {
        let panel = &mut self.modals.snooze_panel;
        panel.visible = false;
        panel.target = None;
        panel.custom_input = None;
        panel.custom_error = None;
        panel.custom_time.clear();
    }

    /// Enter: the highlighted row, or the typed time while typing one.
    pub(super) fn confirm_not_now(&mut self) {
        let Some(target) = self.modals.snooze_panel.target.clone() else {
            self.close_not_now();
            return;
        };
        if self.modals.snooze_panel.custom_input.is_some() {
            let typed = self
                .modals
                .snooze_panel
                .custom_input
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty();
            if typed {
                self.modals.snooze_panel.custom_error =
                    Some("Type a time, e.g. `tomorrow 9am` or `in 2h`".into());
                return;
            }
            // Commit the instant the panel previewed, not a fresh parse.
            match self.modals.snooze_panel.custom_time.chosen() {
                Ok(at) => {
                    self.close_not_now();
                    self.not_now_until(target, at);
                }
                Err(message) => self.modals.snooze_panel.custom_error = Some(message),
            }
            return;
        }
        let rows = crate::ui::snooze_modal::NotNowRows::of(&target);
        match rows.row(self.modals.snooze_panel.selected_index) {
            crate::ui::snooze_modal::NotNowRow::Later => {
                self.close_not_now();
                self.not_now_later(target);
            }
            crate::ui::snooze_modal::NotNowRow::Preset(preset) => {
                let at = resolve_snooze_preset(preset, &self.modals.snooze_config);
                self.close_not_now();
                self.not_now_until(target, at);
            }
            crate::ui::snooze_modal::NotNowRow::Custom => {
                let panel = &mut self.modals.snooze_panel;
                panel.custom_input = Some(String::new());
                panel.custom_error = None;
                panel.custom_time.clear();
            }
        }
    }

    /// A digit picks the Nth preset at once.
    pub(super) fn not_now_preset(&mut self, nth: usize) {
        let Some(target) = self.modals.snooze_panel.target.clone() else {
            return;
        };
        let Some(preset) = nth
            .checked_sub(1)
            .and_then(|index| snooze_presets().get(index).copied())
        else {
            return;
        };
        let at = resolve_snooze_preset(preset, &self.modals.snooze_config);
        self.close_not_now();
        self.not_now_until(target, at);
    }

    /// Later, with no time.
    fn not_now_later(&mut self, target: NotNowTarget) {
        if let NotNowTarget::Mail { message_id } = &target {
            self.focus_put_off(message_id);
        }
        match target {
            NotNowTarget::Mail { message_id }
            | NotNowTarget::Conversation {
                message_id,
                waiting: false,
                ..
            } => self.flag_reply_later_now(message_id),
            NotNowTarget::Reading {
                item_key,
                on_later,
                link,
                domain,
                ..
            } => self.set_reading_later(item_key, !on_later, link, domain),
            // No Later row is shown for these.
            NotNowTarget::Conversation { waiting: true, .. } | NotNowTarget::Todo { .. } => {}
        }
    }

    /// Off until `at`, by the request this place always used.
    fn not_now_until(&mut self, target: NotNowTarget, at: chrono::DateTime<chrono::Utc>) {
        if let NotNowTarget::Mail { message_id } = &target {
            self.focus_put_off(message_id);
        }
        match target {
            NotNowTarget::Mail { message_id } | NotNowTarget::Reading { message_id, .. } => {
                self.queue_mutation(
                    Request::Snooze {
                        message_id,
                        wake_at: at,
                    },
                    MutationEffect::StatusOnly(format!(
                        "Snoozed until {}",
                        at.with_timezone(&chrono::Local).format("%a %b %e %H:%M")
                    )),
                    "Snoozing...".into(),
                );
            }
            NotNowTarget::Conversation { thread, .. } => {
                // Off the desk at once; the answer says what was set, and a
                // failure refetches the desk.
                self.remove_desk_thread(&thread);
                self.queue_mutation(
                    Request::DeferThreads {
                        thread_ids: vec![thread.thread_id.clone()],
                        thread_accounts: thread_accounts(std::slice::from_ref(&thread)),
                        until: at,
                        dry_run: false,
                    },
                    MutationEffect::RefreshPlaces("Time set. Press u to undo".into()),
                    "Setting the time...".into(),
                );
            }
            NotNowTarget::Todo { todo_id } => {
                self.queue_mutation(
                    Request::ScheduleTodo {
                        todo_id,
                        when: Some(at.to_rfc3339()),
                        time_zone: None,
                        dry_run: false,
                    },
                    MutationEffect::Todo("Scheduled. It shows up then".into()),
                    "Saving...".into(),
                );
            }
        }
    }
}
