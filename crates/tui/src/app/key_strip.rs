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
            MailboxView::Reading => "reading",
            MailboxView::ArchiveMode => "archive",
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
            "reading" => {
                let page = &self.mailbox.reading_page;
                if page.reader_focused && page.reader.is_some() {
                    ("reading-reader", Vec::new())
                } else {
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
                let key = action
                    .tui
                    .iter()
                    .find(|binding| binding.contexts.includes(context))?
                    .keys
                    .first()?;
                let label = action.short_label.as_deref().unwrap_or(&action.label);
                Some((key_display(key), label.to_string()))
            })
            .collect();
        (!hints.is_empty()).then_some(hints)
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
