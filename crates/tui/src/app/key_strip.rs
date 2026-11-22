
use super::state::{NowRow, ReadingRow, TodoPanel, UpdatesRow};
use super::*;

impl App {
    pub(crate) fn mode_key_context(&self) -> Option<&'static str> {
        if self.screen != Screen::Mailbox || self.mailbox.active_pane != ActivePane::MailList {
            return None;
        }
        Some(match self.mailbox.mailbox_view {
            MailboxView::Now => "now",
            MailboxView::People => "messages",
            MailboxView::Todo => match self.mailbox.todo_page.panel {
                TodoPanel::Runway => "todo",
                TodoPanel::Catchup => "catchup",
                TodoPanel::Expired => "expired",
            },
            MailboxView::Updates => "updates",
            MailboxView::Reading
                if self.mailbox.reading_page.reader_focused
                    && self.mailbox.reading_page.reader.is_some() =>
            {
                "reading-reader"
            }
            MailboxView::Reading => "reading",
            MailboxView::ArchiveMode => "archive",
            MailboxView::Focus => "focus",
            _ => return None,
        })
    }

    fn lit_strip(&self, context: &str) -> Option<(&'static str, Vec<&'static str>)> {
        Some(match context {
            "now" => match self.selected_now_row()? {
                NowRow::Person(_) => ("now.person", Vec::new()),
                NowRow::Updates(_) => ("now.updates", Vec::new()),
                NowRow::Todo(_) | NowRow::Reading(_) => ("now.item", Vec::new()),
                NowRow::Arrivals(_) | NowRow::NotSure(_) => return None,
            },
            "messages" if self.selected_messages_row().is_some() => ("messages", Vec::new()),
            "messages" => ("messages.empty", Vec::new()),
            "todo" => {
                let todo = self.selected_todo()?;
                (
                    if todo.done_at.is_some() {
                        "todo.done"
                    } else {
                        "todo"
                    },
                    Vec::new(),
                )
            }
            "focus" => {
                let page = &self.mailbox.focus_page;
                if page.current().is_some() {
                    ("focus", Vec::new())
                } else {
                    let mut omit = vec!["focus.reply", "focus.done", "focus.snooze", "focus.draft"];
                    if page.deferred.is_empty() {
                        omit.push("focus.skip");
                    }
                    ("focus", omit)
                }
            }
            "catchup" => ("catchup", Vec::new()),
            "expired" => ("expired", Vec::new()),
            "updates" => {
                let has_code = matches!(
                    self.selected_updates_row()?,
                    UpdatesRow::Line(line) if line.code.is_some()
                );
                (
                    "updates",
                    if has_code {
                        Vec::new()
                    } else {
                        vec!["updates.copy-code"]
                    },
                )
            }
            "reading-reader" => ("reading-reader", Vec::new()),
            "reading" => {
                let page = &self.mailbox.reading_page;
                let offer = matches!(
                    self.selected_reading_row()?,
                    ReadingRow::Item(item) if item.unsubscribe_offer.is_some()
                );
                let omit = if offer && !page.later_shelf {
                    Vec::new()
                } else {
                    vec!["reading.unsubscribe"]
                };
                ("reading", omit)
            }
            "archive" if self.mailbox.records_page.showing_subscriptions() => {
                ("archive.subscriptions", Vec::new())
            }
            "archive" => {
                let pdf = self.selected_record()?.pdf.is_some();
                (
                    "archive",
                    if pdf {
                        Vec::new()
                    } else {
                        vec!["archive.open-document"]
                    },
                )
            }
            _ => return None,
        })
    }

    pub(crate) fn mode_strip_hints(&self) -> Option<Vec<(String, String)>> {
        let context = self.mode_key_context()?;
        let (strip_id, omit) = self.lit_strip(context)?;
        let keymap = mxr_keymap::defaults();
        let hints: Vec<(String, String)> = keymap
            .strip(strip_id)?
            .actions
            .iter()
            .filter(|id| !omit.contains(&id.as_str()))
            .filter_map(|id| {
                let action = keymap.get(id)?;
                let remaps = &self.keybindings.modes;
                let key = match remaps.key_for(context, id) {
                    Some(key) => key,
                    None => action
                        .tui
                        .iter()
                        .find(|binding| binding.contexts.includes(context))?
                        .keys
                        .iter()
                        .find(|key| remaps.lookup(context, key).is_none())?,
                };
                let label = action.short_label.as_deref().unwrap_or(&action.label);
                Some((key_display(key), label.to_string()))
            })
            .collect();
        (!hints.is_empty()).then_some(hints)
    }
}

impl App {
    pub(crate) fn help_key_places(&self) -> Vec<&'static str> {
        if self.modals.help_context_filter.is_some() {
            return Vec::new();
        }
        if self.screen == Screen::Mailbox
            && self.mailbox.active_pane == ActivePane::MailList
            && self.mailbox.mailbox_view == MailboxView::CalendarInvites
        {
            return vec!["invites"];
        }
        self.key_place().into_iter().collect()
    }
}

fn key_display(key: &str) -> String {
    match key {
        "Escape" => "Esc".into(),
        "ArrowDown" => "↓".into(),
        "ArrowUp" => "↑".into(),
        "ArrowLeft" => "←".into(),
        "ArrowRight" => "→".into(),
        other => other.into(),
    }
}

impl App {
    pub(crate) fn prefix_guide(
        &self,
    ) -> Option<(char, Vec<crate::ui::prefix_guide::Continuation>)> {
        if self.input.prefix_waited(self.input.now())? < crate::input::PREFIX_GUIDE_DELAY {
            return None;
        }
        self.prefix_choices()
            .filter(|(_, choices)| !choices.is_empty())
    }

    fn prefix_choices(&self) -> Option<(char, Vec<crate::ui::prefix_guide::Continuation>)> {
        let prefix = self.input.pending_prefix()?;
        let mut places = self.help_key_places();
        if places.is_empty() {
            places = crate::ui::help_keys::places_for(self.current_ui_context());
        }
        let keys = crate::ui::help_keys::KeyRows {
            config: &self.keybindings,
            view: self.keybinding_context(),
        };
        let mut choices = crate::ui::prefix_guide::continuations(&keys, &places, prefix);
        let saved = &self.mailbox.saved_searches;
        choices.retain_mut(|choice| {
            let Some(slot) = choice.id.strip_prefix("nav.saved-search-") else {
                return true;
            };
            let search = slot
                .parse::<usize>()
                .ok()
                .and_then(|slot| slot.checked_sub(1))
                .and_then(|index| saved.get(index));
            match search {
                Some(search) => {
                    choice.label = search.name.clone();
                    true
                }
                None => false,
            }
        });
        Some((prefix, choices))
    }

    #[cfg(test)]
    pub(crate) fn freeze_input_clock_for_tests(&mut self) {
        self.input.freeze_clock_for_tests();
    }

    #[cfg(test)]
    pub(crate) fn advance_input_clock_for_tests(&mut self, by: std::time::Duration) {
        self.input.advance_clock_for_tests(by);
    }
}
