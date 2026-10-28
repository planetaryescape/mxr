//! The hint bar's verbs for what is lit in a mode, from the key
//! definition's `[[strip]]` tables (blueprint chapter 35, D130). The web
//! app's status strip reads the same tables, so one state offers the same
//! verbs in both clients.

use super::state::{NowRow, ReadingRow, TodoPanel, UpdatesRow};
use super::*;

impl App {
    /// The key definition's name for the mode the list is showing, when it
    /// is one with its own keys.
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

    /// The strip for the lit item, with the members that don't apply to it.
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
                    // The end: leave, or `j` for the ones set aside.
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

    /// Key and label for each verb of the lit item's strip that has a TUI
    /// key here. `None` outside a mode, or when nothing is lit.
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
    /// The key definition's places help lists keys for: where keys are
    /// going while help tracks focus. Empty while help is filtered to
    /// another context, so help lists that context's places.
    pub(crate) fn help_key_places(&self) -> Vec<&'static str> {
        if self.modals.help_context_filter.is_some() {
            return Vec::new();
        }
        // The invites lens reads its own keys; the list's don't apply there.
        if self.screen == Screen::Mailbox
            && self.mailbox.active_pane == ActivePane::MailList
            && self.mailbox.mailbox_view == MailboxView::CalendarInvites
        {
            return vec!["invites"];
        }
        self.key_place().into_iter().collect()
    }
}

/// The hint bar spells a few keys shorter than the definition does.
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
    /// The prefix guide, once a prefix has waited [`PREFIX_GUIDE_DELAY`]
    /// (blueprint 35): the prefix and the keys that can follow it where keys
    /// are going, from the same rows help lists.
    ///
    /// [`PREFIX_GUIDE_DELAY`]: crate::input::PREFIX_GUIDE_DELAY
    pub(crate) fn prefix_guide(
        &self,
        now: std::time::Instant,
    ) -> Option<(char, Vec<crate::ui::prefix_guide::Continuation>)> {
        if self.input.prefix_waited(now)? < crate::input::PREFIX_GUIDE_DELAY {
            return None;
        }
        self.prefix_choices()
            .filter(|(_, choices)| !choices.is_empty())
    }

    /// The waiting prefix and every key that can follow it here.
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
        Some((
            prefix,
            crate::ui::prefix_guide::continuations(&keys, &places, prefix),
        ))
    }

    /// A prefix that just started and has nothing to list here waits
    /// [`BARE_PREFIX_TIMEOUT`] instead of the full wait.
    ///
    /// [`BARE_PREFIX_TIMEOUT`]: crate::input::BARE_PREFIX_TIMEOUT
    pub(crate) fn shorten_bare_prefix(&mut self) {
        if self
            .prefix_choices()
            .is_some_and(|(_, choices)| choices.is_empty())
        {
            self.input.cap_wait(crate::input::BARE_PREFIX_TIMEOUT);
        }
    }

    /// Test-only: as if the waiting prefix was pressed `by` earlier.
    #[cfg(test)]
    pub(crate) fn age_prefix_for_tests(&mut self, by: std::time::Duration) {
        self.input.age_prefix_for_tests(by);
    }
}
