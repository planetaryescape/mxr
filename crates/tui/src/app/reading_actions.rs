//! Reading in the TUI: open the edition, read an item beside it, fetch its
//! article only on `L`, keep it on Later, let go of one or everything
//! shown after a preview, unsubscribe after a preview with the evidence,
//! switch to the source's own text, and highlight a paragraph. The daemon
//! owns every band, rank and preview; this turns keys into requests.

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{ReadingConfirm, ReadingRow, ReadingUnsubscribeTarget, ReadingView};
use mxr_core::id::ThreadId;
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, ReadingFetchData, ReadingItemDetailData};

/// The mode id Reading's guide and its card are kept under.
pub(crate) const READING_MODE: &str = mxr_protocol::READING_GUIDE.mode;

/// What a row refers to, copied out so the page can change.
struct Target {
    key: String,
    thread_id: ThreadId,
    message_id: MessageId,
    account_id: mxr_core::AccountId,
    sender_email: String,
    source: String,
    domain: Option<String>,
    on_later: bool,
    link: bool,
}

impl From<ReadingRow<'_>> for Target {
    fn from(row: ReadingRow<'_>) -> Self {
        let issue = row.issue();
        let on_later = match row {
            ReadingRow::Item(item) => item.on_later,
            ReadingRow::Link(_, link) => link.on_later,
        };
        Self {
            key: row.key().to_string(),
            thread_id: issue.thread_id.clone(),
            message_id: issue.message_id.clone(),
            account_id: issue.account_id.clone(),
            sender_email: issue.sender_email.clone(),
            source: issue.source.clone(),
            domain: row.domain().map(str::to_string),
            on_later,
            link: row.is_link(),
        }
    }
}

impl App {
    pub(super) fn apply_reading_action(&mut self, action: Action) {
        match action {
            Action::OpenReading => self.open_reading(),
            Action::ReadingRead => self.reading_read(),
            Action::ReadingArticle => self.reading_article(),
            Action::ReadingLater => self.reading_later(),
            Action::ReadingLetGo => self.reading_let_go(),
            Action::ReadingLetGoAll => self.reading_let_go_all(),
            Action::ReadingUnsubscribe => self.reading_unsubscribe(),
            Action::ReadingOriginal => self.reading_original(),
            Action::ReadingHighlight => self.reading_highlight(),
            Action::ReadingOpenEmail => self.reading_open_email(),
            Action::ReadingBack => self.reading_back(),
            Action::ReadingLaterShelf => self.toggle_later_shelf(),
            _ => {}
        }
    }

    fn open_reading(&mut self) {
        self.enter_mode_view(MailboxView::Reading);
        let page = &mut self.mailbox.reading_page;
        page.confirm = None;
        page.later_shelf = false;
        page.reader_focused = false;
        page.pending_refresh = true;
        page.pending_mark_visit = true;
        self.mailbox.pending_rail_refresh = true;
    }

    /// The Reading lens owns the keyboard.
    pub(crate) fn reading_lens_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::Reading
            && self.mailbox.active_pane == ActivePane::MailList
    }

    pub fn selected_reading_row(&self) -> Option<ReadingRow<'_>> {
        self.mailbox
            .reading_page
            .rows()
            .get(self.mailbox.selected_index)
            .copied()
    }

    fn reading_target(&self) -> Option<Target> {
        self.selected_reading_row().map(Target::from)
    }

    /// The runtime fetched the edition and the guide.
    pub(crate) fn set_reading_edition(
        &mut self,
        edition: mxr_protocol::ReadingEditionData,
        guide: Option<mxr_protocol::ModeGuideData>,
    ) {
        let page = &mut self.mailbox.reading_page;
        page.edition = Some(edition);
        if guide.is_some() {
            page.guide = guide;
        }
        if self.mailbox.mailbox_view == MailboxView::Reading {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.reading_page.row_count().saturating_sub(1));
        }
    }

    /// The item under the cursor, when the reader shows another one (or
    /// none) and nothing is on its way: the runtime fetches it.
    pub(crate) fn reading_item_to_load(&mut self) -> Option<String> {
        if self.mailbox.mailbox_view != MailboxView::Reading {
            return None;
        }
        let page = &mut self.mailbox.reading_page;
        if let Some(key) = page.pending_item.take() {
            page.item_in_flight = Some(key.clone());
            return Some(key);
        }
        let key = self.selected_reading_row()?.key().to_string();
        let page = &mut self.mailbox.reading_page;
        let shown = page
            .reader
            .as_ref()
            .map(|reader| reader.item.item_key.as_str());
        if shown == Some(key.as_str()) || page.item_in_flight.as_deref() == Some(key.as_str()) {
            return None;
        }
        page.item_in_flight = Some(key.clone());
        Some(key)
    }

    pub(crate) fn set_reading_item(&mut self, key: &str, detail: ReadingItemDetailData) {
        let page = &mut self.mailbox.reading_page;
        if page.item_in_flight.as_deref() == Some(key) {
            page.item_in_flight = None;
        }
        let same = page
            .reader
            .as_ref()
            .is_some_and(|reader| reader.item.item_key == key);
        if !same {
            page.scroll = 0;
            page.view = ReadingView::Issue;
            page.original_text = None;
        }
        page.reader = Some(detail);
        page.reader_lines = crate::ui::reading_lens::reader_line_count(page);
    }

    pub(crate) fn reading_item_failed(&mut self, key: &str, error: &str) {
        let page = &mut self.mailbox.reading_page;
        if page.item_in_flight.as_deref() == Some(key) {
            page.item_in_flight = None;
        }
        self.status_message = Some(format!("Couldn't open the item: {error}"));
    }

    /// The article came back: show it, or why it couldn't be read.
    pub(crate) fn set_reading_article(&mut self, key: &str, fetch: ReadingFetchData) {
        let page = &mut self.mailbox.reading_page;
        let shown = page
            .reader
            .as_mut()
            .filter(|reader| reader.item.item_key == key);
        let status = match (&fetch.article, &fetch.error) {
            (Some(article), _) => {
                let from = if fetch.cached {
                    "Saved copy".to_string()
                } else {
                    format!("Fetched from {}", article.contacted.join(", "))
                };
                format!("{from}: {} min", article.minutes)
            }
            (None, Some(error)) => format!("Couldn't read the article: {error}"),
            (None, None) => "No article came back".to_string(),
        };
        if let Some(reader) = shown {
            reader.article_error.clone_from(&fetch.error);
            if fetch.article.is_some() {
                reader.article = fetch.article;
            }
            page.view = ReadingView::Article;
            page.scroll = 0;
            page.reader_lines = crate::ui::reading_lens::reader_line_count(page);
        }
        self.status_message = Some(status);
    }

    /// Refetch the edition, and the item in the reader, after a change.
    pub(crate) fn refresh_reading(&mut self) {
        let page = &mut self.mailbox.reading_page;
        page.pending_refresh = true;
        // Refetch the open item too; its place in the text is kept.
        page.pending_item = page.reader.as_ref().map(|r| r.item.item_key.clone());
    }

    /// Enter: read the item beside the list.
    fn reading_read(&mut self) {
        let Some(target) = self.reading_target() else {
            return;
        };
        let page = &mut self.mailbox.reading_page;
        page.reader_focused = true;
        page.opened_at = Some(std::time::Instant::now());
        if page
            .reader
            .as_ref()
            .is_none_or(|reader| reader.item.item_key != target.key)
        {
            page.pending_item = Some(target.key.clone());
        }
        self.queue_best_effort_mutation(
            Request::RecordReadingEngagement {
                item_key: target.key,
                opened: true,
                dwell_ms: 0,
                progress: 0.0,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
    }

    /// Esc in the reader: back to the edition, reporting the time read and
    /// how far.
    fn reading_back(&mut self) {
        let page = &mut self.mailbox.reading_page;
        page.reader_focused = false;
        page.original_text = None;
        let Some(key) = page.reader.as_ref().map(|r| r.item.item_key.clone()) else {
            return;
        };
        let dwell_ms = page.opened_at.take().map_or(0, |at| {
            u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX)
        });
        let progress = page.progress();
        self.queue_best_effort_mutation(
            Request::RecordReadingEngagement {
                item_key: key,
                opened: false,
                dwell_ms,
                progress,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
    }

    /// `L`: the article, fetched only now and only after naming the site.
    fn reading_article(&mut self) {
        let Some(target) = self.reading_target() else {
            return;
        };
        let Some(domain) = target.domain else {
            self.status_message =
                Some("This issue is the whole piece: there is no article to fetch".into());
            return;
        };
        let page = &mut self.mailbox.reading_page;
        let saved = page
            .reader
            .as_ref()
            .filter(|reader| reader.item.item_key == target.key)
            .is_some_and(|reader| reader.article.is_some());
        if saved {
            page.view = match page.view {
                ReadingView::Issue => ReadingView::Article,
                ReadingView::Article => ReadingView::Issue,
            };
            page.scroll = 0;
            page.reader_lines = crate::ui::reading_lens::reader_line_count(page);
            return;
        }
        page.pending_fetch = Some((target.key, false));
        self.status_message = Some(format!("Fetching from {domain}\u{2026}"));
    }

    /// `b`: on Later, or off it. A link's article is saved too, so Later
    /// reads offline.
    fn reading_later(&mut self) {
        let Some(target) = self.reading_target() else {
            return;
        };
        let later = !target.on_later;
        self.queue_mutation(
            Request::SetReadingLater {
                item_keys: vec![target.key.clone()],
                later,
                dry_run: false,
            },
            MutationEffect::ModeDone(String::new()),
            if later {
                "Saving to Later...".into()
            } else {
                "Taking it off Later...".into()
            },
        );
        if later && target.link {
            if let Some(domain) = target.domain {
                self.mailbox.reading_page.pending_fetch = Some((target.key, false));
                self.status_message = Some(format!(
                    "Saving to Later and fetching from {domain}\u{2026}"
                ));
            }
        }
    }

    /// `e`: let go of the item's issue in Reading. Its links on Later stay.
    fn reading_let_go(&mut self) {
        let Some(target) = self.reading_target() else {
            return;
        };
        self.mailbox.reading_page.remove_thread(&target.thread_id);
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.reading_page.row_count().saturating_sub(1));
        self.queue_mutation(
            Request::SetModeDone {
                thread_ids: vec![target.thread_id],
                mode: ModeKindData::Reading,
                dry_run: false,
                todo_ids: Vec::new(),
                sender: None,
            },
            MutationEffect::ModeDone("Let go".into()),
            "Letting go...".into(),
        );
    }

    /// `B`: the Later shelf, or back to the edition.
    fn toggle_later_shelf(&mut self) {
        let page = &mut self.mailbox.reading_page;
        page.later_shelf = !page.later_shelf;
        page.reader_focused = false;
        self.mailbox.selected_index = 0;
    }

    /// `A`: ask the daemon what letting go of everything shown would do;
    /// with that preview on screen, Enter lets go of exactly those.
    fn reading_let_go_all(&mut self) {
        let page = &mut self.mailbox.reading_page;
        if let Some(ReadingConfirm::LetGoAll { thread_ids, .. }) = page.confirm.take() {
            for thread in &thread_ids {
                page.remove_thread(thread);
            }
            self.mailbox.selected_index = 0;
            self.queue_mutation(
                Request::SetModeDone {
                    thread_ids,
                    mode: ModeKindData::Reading,
                    dry_run: false,
                    todo_ids: Vec::new(),
                    sender: None,
                },
                MutationEffect::ModeDone("Let go of everything shown".into()),
                "Letting go...".into(),
            );
            return;
        }
        if page.later_shelf {
            self.status_message = Some("Later never fades: b takes one thing off".into());
            return;
        }
        let thread_ids = page.thread_ids();
        if thread_ids.is_empty() {
            self.status_message = Some("Nothing in the edition to let go of".into());
            return;
        }
        page.pending_let_go_preview = Some(thread_ids);
        self.status_message = Some("Checking what letting go would change\u{2026}".into());
    }

    /// The daemon's dry run of letting go of everything shown.
    pub(crate) fn show_reading_let_go_preview(
        &mut self,
        thread_ids: &[ThreadId],
        items: Vec<ModeDoneOutcomeData>,
    ) {
        self.status_message = None;
        let thread_ids: Vec<ThreadId> = items
            .iter()
            .filter(|item| item.error.is_none())
            .map(|item| item.thread_id.clone())
            .filter(|thread| thread_ids.contains(thread))
            .collect();
        if thread_ids.is_empty() {
            self.push_toast(Toast::success("Nothing left to let go of"));
            return;
        }
        self.mailbox.reading_page.confirm = Some(ReadingConfirm::LetGoAll { thread_ids, items });
    }

    /// `D`: preview unsubscribing from the item's source, with the
    /// evidence; Enter on the preview unsubscribes.
    fn reading_unsubscribe(&mut self) {
        let page = &mut self.mailbox.reading_page;
        if let Some(ReadingConfirm::Unsubscribe {
            target,
            preview_token,
            ..
        }) = page.confirm.take()
        {
            // Only a preview the daemon answered with a token commits.
            let Some(preview_token) = preview_token else {
                self.status_message =
                    Some("Nothing to unsubscribe with: preview again with D".into());
                return;
            };
            self.queue_mutation(
                Request::UnsubscribePurge {
                    address: target.sender_email.clone(),
                    account_id: Some(target.account_id),
                    dry_run: false,
                    archive_on_no_method: false,
                    preview_token: Some(preview_token),
                },
                MutationEffect::ModeDone(format!("Unsubscribed from {}", target.source)),
                "Unsubscribing...".into(),
            );
            return;
        }
        let Some(target) = self.reading_target() else {
            return;
        };
        let source = self
            .mailbox
            .reading_page
            .edition
            .as_ref()
            .and_then(|edition| {
                edition.sources.iter().find(|source| {
                    source.account_id == target.account_id
                        && source.sender_email == target.sender_email
                })
            });
        let (evidence, method) = source.map_or_else(
            || {
                (
                    "mxr hasn't seen you read any of its issues yet".to_string(),
                    mxr_protocol::ReadingUnsubscribeData::None,
                )
            },
            |source| (source.evidence.clone(), source.unsubscribe),
        );
        self.mailbox.reading_page.pending_unsubscribe_preview = Some(ReadingUnsubscribeTarget {
            account_id: target.account_id,
            sender_email: target.sender_email,
            source: target.source,
            evidence,
            method,
        });
        self.status_message = Some("Checking what unsubscribing would do\u{2026}".into());
    }

    /// The daemon's dry run of the unsubscribe.
    pub(crate) fn show_reading_unsubscribe_preview(
        &mut self,
        mut target: ReadingUnsubscribeTarget,
        preview: mxr_protocol::UnsubscribePurgeResultData,
    ) {
        self.status_message = None;
        // The method shown is the one the daemon would use now.
        target.method = mxr_protocol::ReadingUnsubscribeData::from(&preview.method);
        let preview_token = preview
            .preview_token
            .filter(|_| target.method != mxr_protocol::ReadingUnsubscribeData::None);
        self.mailbox.reading_page.confirm = Some(ReadingConfirm::Unsubscribe {
            target,
            message_count: preview.message_count,
            preview_token,
        });
    }

    /// `R`: the source's own text instead of the reader, remembered for
    /// the source.
    fn reading_original(&mut self) {
        let Some(target) = self.reading_target() else {
            return;
        };
        let page = &mut self.mailbox.reading_page;
        let on = page.original_text.is_none();
        if let Some(source) = page.edition.as_mut().and_then(|edition| {
            edition.sources.iter_mut().find(|source| {
                source.account_id == target.account_id && source.sender_email == target.sender_email
            })
        }) {
            source.original_layout = on;
        }
        if on {
            page.pending_original = Some(target.message_id);
            self.status_message = Some("Showing the email's own text".into());
        } else {
            page.original_text = None;
            page.scroll = 0;
            page.reader_lines = crate::ui::reading_lens::reader_line_count(page);
            self.status_message = Some("Back to the reader".into());
        }
        self.queue_best_effort_mutation(
            Request::SetReadingSource {
                account_id: target.account_id,
                sender_email: target.sender_email,
                original_layout: Some(on),
                dismiss_unsubscribe_offer: false,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
    }

    pub(crate) fn set_reading_original(&mut self, text: String) {
        let page = &mut self.mailbox.reading_page;
        page.original_text = Some(text);
        page.scroll = 0;
        page.reader_lines = crate::ui::reading_lens::reader_line_count(page);
    }

    /// `h`: the paragraph at the top of the reader becomes a highlight.
    fn reading_highlight(&mut self) {
        let page = &self.mailbox.reading_page;
        let Some(reader) = &page.reader else {
            self.status_message = Some("Open an item first: Enter reads it".into());
            return;
        };
        let Some(quote) = crate::ui::reading_lens::paragraph_at_top(page) else {
            self.status_message = Some("Scroll to the paragraph to highlight".into());
            return;
        };
        let view = match page.view {
            ReadingView::Article if reader.article.is_some() => "article",
            _ => "issue",
        };
        let key = reader.item.item_key.clone();
        self.queue_mutation(
            Request::SaveHighlight {
                item_key: key,
                quote,
                note: None,
                view: Some(view.into()),
            },
            MutationEffect::ModeDone("Highlight saved".into()),
            "Saving the highlight...".into(),
        );
    }

    fn reading_open_email(&mut self) {
        let Some(target) = self.reading_target() else {
            return;
        };
        self.mailbox.pending_invite_open = Some(target.message_id);
        self.status_message = Some("Opening the email\u{2026}".into());
    }

    fn scroll_reader(&mut self, down: bool, lines: u16) {
        let page = &mut self.mailbox.reading_page;
        let max = page.reader_lines.saturating_sub(1);
        page.scroll = if down {
            page.scroll.saturating_add(lines).min(max)
        } else {
            page.scroll.saturating_sub(lines)
        };
    }

    /// Key handling for the Reading lens: its verbs, the reader's
    /// scrolling, and the two previews.
    pub(super) fn reading_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let page = &self.mailbox.reading_page;
        if let Some(confirm) = &page.confirm {
            let confirm_action = match confirm {
                ReadingConfirm::LetGoAll { .. } => Action::ReadingLetGoAll,
                ReadingConfirm::Unsubscribe { .. } => Action::ReadingUnsubscribe,
            };
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(confirm_action),
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.mailbox.reading_page.confirm = None;
                    self.status_message = Some("Nothing changed".into());
                    None
                }
                _ => None,
            };
        }
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        if page.reader_focused && page.reader.is_some() {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down if plain => {
                    self.scroll_reader(true, 1);
                    return None;
                }
                KeyCode::Char('k') | KeyCode::Up if plain => {
                    self.scroll_reader(false, 1);
                    return None;
                }
                KeyCode::Char(' ') | KeyCode::PageDown => {
                    self.scroll_reader(true, 20);
                    return None;
                }
                KeyCode::PageUp => {
                    self.scroll_reader(false, 20);
                    return None;
                }
                KeyCode::Esc | KeyCode::Left => return Some(Action::ReadingBack),
                KeyCode::Char('h') if plain => return Some(Action::ReadingHighlight),
                _ => {}
            }
        }
        let action = match key.code {
            KeyCode::Char('/') if plain => Some(Action::OpenGlobalSearch),
            KeyCode::Char('h') | KeyCode::Left if plain => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                return None;
            }
            KeyCode::Esc if self.active_hint().is_some() => Some(Action::DismissHint),
            KeyCode::Esc if page.later_shelf => Some(Action::ReadingLaterShelf),
            KeyCode::Char('B') if shifted => Some(Action::ReadingLaterShelf),
            KeyCode::Char('K') if shifted => Some(Action::OpenSenderKindMenu),
            KeyCode::Enter => Some(Action::ReadingRead),
            KeyCode::Char('L') if shifted => Some(Action::ReadingArticle),
            KeyCode::Char('b') if plain => Some(Action::ReadingLater),
            KeyCode::Char('e') if plain => Some(Action::ReadingLetGo),
            KeyCode::Char('A') if shifted => Some(Action::ReadingLetGoAll),
            KeyCode::Char('D') if shifted => Some(Action::ReadingUnsubscribe),
            KeyCode::Char('R') if shifted => Some(Action::ReadingOriginal),
            KeyCode::Char('o') if plain => Some(Action::ReadingOpenEmail),
            KeyCode::Char('t') if plain => Some(Action::CreateTodoFromMessage),
            KeyCode::Char('u') if plain => Some(Action::UndoLastMutation),
            _ => None,
        };
        if action.is_some() {
            return action;
        }
        self.contextual_input_action(key).filter(|action| {
            !matches!(
                action,
                Action::OpenSelected
                    | Action::ToggleSelect
                    | Action::VisualLineMode
                    | Action::AttachmentList
                    | Action::ExportThread
                    | Action::ToggleFullscreen
                    | Action::FlagReplyLater
                    | Action::NextSearchResult
                    | Action::PrevSearchResult
                    | Action::SwitchPane
                    | Action::Back
                    | Action::Archive
                    | Action::MarkReadAndArchive
                    | Action::Trash
                    | Action::Spam
                    | Action::Star
                    | Action::MarkRead
                    | Action::MarkUnread
                    | Action::Reply
                    | Action::ReplyAll
                    | Action::Forward
                    | Action::RespondInvite(_)
                    | Action::RespondInviteWithComment(_)
            )
        })
    }
}
