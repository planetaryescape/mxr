//! Updates in the TUI: open the briefing, let go of the whole digest after
//! the daemon's preview (Enter commits exactly that selection), let go of
//! one source, hand a line to To do, tune a source, and open a line's link
//! or email. The daemon owns the cut, the lines and every selection.

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{UpdatesRow, UpdatesTuneMenu};
use mxr_protocol::{UpdateLineData, UpdateSignalData, UpdateSourceSettingData, UpdatesDigestData};

/// The mode id Updates' guide and its card are kept under.
pub(crate) const UPDATES_MODE: &str = mxr_protocol::UPDATES_GUIDE.mode;

/// The `K` menu's choices, in the order their digits pick them.
pub(crate) const TUNE_CHOICES: [(UpdateSourceSettingData, &str); 4] = [
    (UpdateSourceSettingData::EveryDigest, "every digest"),
    (UpdateSourceSettingData::ChangesOnly, "changes only"),
    (UpdateSourceSettingData::Muted, "mute"),
    (
        UpdateSourceSettingData::Breakthrough,
        "breakthrough to To do",
    ),
];

/// A parcel line is a tracker: it leaves Updates on its own when it ends.
pub(crate) fn is_tracker_line(line: &UpdateLineData) -> bool {
    line.tracker.as_ref().is_some_and(|t| t.kind == "parcel")
}

impl App {
    pub(super) fn apply_updates_action(&mut self, action: Action) {
        match action {
            Action::OpenUpdates => self.open_updates(),
            Action::UpdatesOpen => self.updates_open(false),
            Action::UpdatesOpenEmail => self.updates_open(true),
            Action::UpdatesLetGoAll => self.updates_let_go_all(),
            Action::UpdatesLetGoSource => self.updates_let_go_source(),
            Action::UpdatesNeedsMe => self.updates_needs_me(),
            Action::UpdatesTune => self.updates_tune(),
            Action::UpdatesTuneChoice(choice) => self.updates_tune_choice(choice),
            Action::UpdatesOpenLink => self.updates_open_link(),
            Action::UpdatesClose => self.updates_close(),
            _ => {}
        }
    }

    fn open_updates(&mut self) {
        self.enter_mode_view(MailboxView::Updates);
        let page = &mut self.mailbox.updates_page;
        page.let_go_preview = None;
        page.tune = None;
        page.pending_refresh = true;
        self.mailbox.pending_rail_refresh = true;
    }

    /// The Updates lens owns the keyboard (its list, not a reader beside it).
    pub(crate) fn updates_list_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::Updates
            && self.mailbox.active_pane == ActivePane::MailList
    }

    pub fn selected_updates_row(&self) -> Option<UpdatesRow<'_>> {
        self.mailbox
            .updates_page
            .rows()
            .get(self.mailbox.selected_index)
            .copied()
    }

    fn selected_updates_line(&self) -> Option<UpdateLineData> {
        match self.selected_updates_row() {
            Some(UpdatesRow::Line(line)) => Some(line.clone()),
            _ => None,
        }
    }

    /// The runtime fetched the digest and its guide.
    pub(crate) fn set_updates_digest(
        &mut self,
        digest: UpdatesDigestData,
        guide: Option<mxr_protocol::ModeGuideData>,
    ) {
        let page = &mut self.mailbox.updates_page;
        page.digest = Some(digest);
        if guide.is_some() {
            page.guide = guide;
        }
        if self.mailbox.mailbox_view == MailboxView::Updates {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.updates_page.row_count().saturating_sub(1));
        }
    }

    /// Refetch the digest after anything that can move it.
    pub(crate) fn refresh_updates(&mut self) {
        if self.mailbox.mailbox_view == MailboxView::Updates
            || self.mailbox.updates_page.digest.is_some()
        {
            self.mailbox.updates_page.pending_refresh = true;
        }
    }

    fn clamp_updates_selection(&mut self) {
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.updates_page.row_count().saturating_sub(1));
    }

    /// Enter unfolds the quieter sources, or opens the line's email; `o`
    /// always opens the email.
    fn updates_open(&mut self, email_only: bool) {
        match self.selected_updates_row() {
            Some(UpdatesRow::MoreRoutine(_)) if !email_only => {
                self.mailbox.updates_page.routine_open = true;
            }
            Some(UpdatesRow::Line(line)) => match line.latest_message_id.clone() {
                Some(message_id) => {
                    self.mailbox.pending_invite_open = Some(message_id);
                    self.status_message = Some("Opening the email…".into());
                }
                None => {
                    self.status_message =
                        Some("A tracker has no single email; see Deliveries".into());
                }
            },
            _ => {}
        }
    }

    /// `A`: ask the daemon what letting go of the digest would do; with
    /// that preview on screen, Enter lets go of exactly that selection.
    fn updates_let_go_all(&mut self) {
        let page = &mut self.mailbox.updates_page;
        if let Some(preview) = page.let_go_preview.take() {
            let source = page.let_go_source.take();
            if preview.message_ids.is_empty() {
                return;
            }
            match &source {
                Some((_, key)) => page.remove_source(key),
                None => page.clear_digest(),
            }
            self.clamp_updates_selection();
            self.retire_updates_card();
            let (account_id, source_key) = source.unzip();
            self.queue_mutation(
                Request::LetGoDigest {
                    account_id,
                    cut: Some(preview.cut_at),
                    source_key,
                    selection_token: Some(preview.selection_token),
                    dry_run: false,
                },
                MutationEffect::ModeDone(String::new()),
                "Letting go...".into(),
            );
            return;
        }
        let has_lines = page
            .digest
            .as_ref()
            .is_some_and(|digest| digest.let_go_line.is_some());
        if !has_lines {
            self.status_message = Some("Nothing in this digest to let go of".into());
            return;
        }
        page.let_go_source = None;
        page.pending_let_go_preview = true;
        self.status_message = Some("Checking what letting go would change…".into());
    }

    /// The daemon's dry run of letting go of the digest.
    pub(crate) fn show_updates_let_go_preview(&mut self, preview: mxr_protocol::UpdatesLetGoData) {
        self.status_message = None;
        if preview.message_ids.is_empty() {
            self.push_toast(Toast::success("Nothing left to let go of"));
            return;
        }
        self.mailbox.updates_page.let_go_preview = Some(preview);
    }

    /// `e`: let go of this source in the digest, through the same
    /// preview as `A`: the count shows first, Enter commits that selection.
    fn updates_let_go_source(&mut self) {
        let Some(line) = self.selected_updates_line() else {
            return;
        };
        if is_tracker_line(&line) {
            self.status_message = Some("Trackers leave Updates on their own when they end".into());
            return;
        }
        let page = &mut self.mailbox.updates_page;
        page.let_go_source = Some((line.account_id.clone(), line.source_key.clone()));
        page.pending_let_go_preview = true;
        self.status_message = Some(format!(
            "Checking what letting go of {} would change…",
            crate::ui::sanitize::one_line(&line.source_name)
        ));
    }

    /// `t`: this needs me. A to-do titled by the line, on its email.
    fn updates_needs_me(&mut self) {
        let Some(line) = self.selected_updates_line() else {
            return;
        };
        if line.todo_id.is_some() {
            self.status_message = Some("Already in To do".into());
            return;
        }
        // The message the line's fact and title came from, not the newest.
        let Some(message_id) = line
            .fact_message_id
            .clone()
            .or_else(|| line.latest_message_id.clone())
        else {
            self.status_message = Some("A tracker has no email to make a to-do from".into());
            return;
        };
        let title = crate::ui::sanitize::one_line(&line.todo_title);
        self.queue_mutation(
            Request::CreateTodo {
                message_id,
                title: line.todo_title.clone(),
                kind: None,
                due: None,
                time_zone: None,
                dry_run: false,
            },
            MutationEffect::ModeDone(format!("Added to To do: {title}. g x")),
            "Adding to To do...".into(),
        );
    }

    /// `K`: the tune menu for this line's source.
    fn updates_tune(&mut self) {
        let Some(line) = self.selected_updates_line() else {
            return;
        };
        if is_tracker_line(&line) {
            self.status_message = Some("Trackers can't be tuned; they end on their own".into());
            return;
        }
        self.mailbox.updates_page.tune = Some(UpdatesTuneMenu {
            account_id: line.account_id.clone(),
            source_key: line.source_key.clone(),
            source_name: line.source_name.clone(),
            current: line.setting,
        });
    }

    fn updates_tune_choice(&mut self, choice: usize) {
        let Some(menu) = self.mailbox.updates_page.tune.take() else {
            return;
        };
        let Some((setting, _)) = choice.checked_sub(1).and_then(|i| TUNE_CHOICES.get(i)) else {
            return;
        };
        if *setting == menu.current {
            self.status_message = Some("Already set that way".into());
            return;
        }
        self.queue_mutation(
            Request::SetUpdateSource {
                account_id: Some(menu.account_id),
                source: menu.source_key,
                setting: *setting,
                dry_run: false,
            },
            MutationEffect::ModeDone(String::new()),
            "Tuning...".into(),
        );
    }

    /// `L`: open the line's link, never on a line that needs a look (those
    /// open the email, so a pay or verify link is never one key away).
    fn updates_open_link(&mut self) {
        let Some(line) = self.selected_updates_line() else {
            return;
        };
        let needs = matches!(line.signal, UpdateSignalData::NeedsYou);
        match line.link.as_ref().filter(|_| !needs) {
            Some(link) => {
                crate::ui::url_modal::open_url(&link.url);
                self.status_message = Some(format!(
                    "Opening {}",
                    crate::ui::sanitize::one_line(&link.domain)
                ));
            }
            None => self.status_message = Some("No link here; o opens the email".into()),
        }
    }

    fn updates_close(&mut self) {
        let page = &mut self.mailbox.updates_page;
        if page.tune.take().is_some() {
            return;
        }
        if page.let_go_preview.take().is_some() {
            page.let_go_source = None;
            self.status_message = Some("Kept the digest".into());
            return;
        }
        if page.card_visible() {
            self.retire_updates_card();
        }
    }

    /// Retire Updates' first-encounter card here and in every other client.
    fn retire_updates_card(&mut self) {
        if !self.mailbox.updates_page.card_visible() {
            return;
        }
        self.mailbox.updates_page.card_closed = true;
        let id = self.queue_best_effort_mutation(
            Request::SetModeGuideSeen {
                mode: UPDATES_MODE.into(),
                seen: true,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
        self.mailbox.updates_page.card_close_mutation = Some(id);
    }

    /// The daemon didn't store the closed card: show it again.
    pub(crate) fn reopen_updates_card_after_failure(&mut self, failed: crate::app::MutationId) {
        let page = &mut self.mailbox.updates_page;
        if page.card_close_mutation == Some(failed) {
            page.card_close_mutation = None;
            page.card_closed = false;
        }
    }

    /// Key handling for the Updates lens.
    pub(super) fn updates_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let page = &self.mailbox.updates_page;
        if page.let_go_preview.is_some() {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(Action::UpdatesLetGoAll),
                KeyCode::Esc | KeyCode::Char('n') => Some(Action::UpdatesClose),
                _ => None,
            };
        }
        if page.tune.is_some() {
            return match key.code {
                KeyCode::Char(digit @ '1'..='4') => {
                    Some(Action::UpdatesTuneChoice(digit as usize - '0' as usize))
                }
                KeyCode::Esc => Some(Action::UpdatesClose),
                _ => None,
            };
        }
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        let action = match key.code {
            KeyCode::Char('/') if plain => Some(Action::OpenGlobalSearch),
            KeyCode::Char('h') | KeyCode::Left if plain => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                return None;
            }
            KeyCode::Esc => Some(Action::UpdatesClose),
            KeyCode::Enter => Some(Action::UpdatesOpen),
            KeyCode::Char('o') if plain => Some(Action::UpdatesOpenEmail),
            KeyCode::Char('e') if plain => Some(Action::UpdatesLetGoSource),
            KeyCode::Char('t') if plain => Some(Action::UpdatesNeedsMe),
            KeyCode::Char('A') if shifted => Some(Action::UpdatesLetGoAll),
            KeyCode::Char('K') if shifted => Some(Action::UpdatesTune),
            KeyCode::Char('L') if shifted => Some(Action::UpdatesOpenLink),
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
                    | Action::OpenLinks
                    | Action::OpenSenderKindMenu
                    | Action::RespondInvite(_)
                    | Action::RespondInviteWithComment(_)
            )
        })
    }
}
