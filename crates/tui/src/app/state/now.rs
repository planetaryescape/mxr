//! Now in the TUI: the front page the daemon serves (`GetNow`), Now's
//! teaching copy (`GetModeGuide`) and the rail (`GetRail`). The daemon
//! owns every cap, count and line; this state only holds what came back
//! and what the user is doing with it.

use mxr_core::id::ThreadId;
use mxr_protocol::{
    ModeDoneOutcomeData, ModeGuideData, NowData, NowPersonData, NowReadingPickData, NowTodoData,
    NowUpdatesCardData,
};

/// One selectable row on Now, in the fixed section order.
#[derive(Debug, Clone, Copy)]
pub enum NowRow<'a> {
    Person(&'a NowPersonData),
    Todo(&'a NowTodoData),
    Updates(&'a NowUpdatesCardData),
    Reading(&'a NowReadingPickData),
}

impl NowRow<'_> {
    /// The row's why line, for the footer.
    pub fn why(&self) -> &str {
        match self {
            Self::Person(person) => &person.why,
            Self::Todo(todo) => &todo.why,
            Self::Updates(card) => &card.line,
            Self::Reading(pick) => &pick.why,
        }
    }
}

/// "Let go of the digest" as the daemon previewed it: Enter lets go of
/// exactly these threads in Updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowDigestPreview {
    pub thread_ids: Vec<ThreadId>,
    pub items: Vec<ModeDoneOutcomeData>,
}

#[derive(Debug, Clone, Default)]
pub struct NowPageState {
    pub now: Option<NowData>,
    pub guide: Option<ModeGuideData>,
    /// Closed here before the daemon answered, so it never flickers back.
    pub card_closed: bool,
    /// The `SetModeGuideSeen` sent when the card closed here, so a failed
    /// write can show the card again.
    pub card_close_mutation: Option<crate::app::MutationId>,
    /// Ask the runtime for Now and its guide.
    pub pending_refresh: bool,
    /// Ask the daemon what letting go of these threads in Updates would do.
    pub pending_digest_preview: Option<Vec<ThreadId>>,
    pub digest_preview: Option<NowDigestPreview>,
}

impl NowPageState {
    /// Selectable rows in display order: People, Due soon, the Updates
    /// card, then the evening Reading pick.
    pub fn rows(&self) -> Vec<NowRow<'_>> {
        let Some(now) = &self.now else {
            return Vec::new();
        };
        let mut rows: Vec<NowRow<'_>> = now.people.rows.iter().map(NowRow::Person).collect();
        rows.extend(now.due_soon.todos.iter().map(NowRow::Todo));
        rows.extend(now.updates.iter().map(NowRow::Updates));
        rows.extend(now.reading.iter().map(NowRow::Reading));
        rows
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    /// The first-encounter card shows at the top once Now has items,
    /// until it is closed here or retired anywhere.
    pub fn card_visible(&self) -> bool {
        !self.card_closed
            && self.guide.as_ref().is_some_and(|guide| !guide.card_seen)
            && self.now.as_ref().is_some_and(|now| now.item_count > 0)
    }

    /// Take a to-do with no email off Now before the daemon answers.
    pub fn remove_todo(&mut self, todo_id: &str) {
        if let Some(now) = self.now.as_mut() {
            now.due_soon.todos.retain(|todo| todo.todo.id != todo_id);
        }
    }

    /// Take a thread off Now before the daemon answers.
    pub fn remove_thread(&mut self, thread_id: &ThreadId) {
        let Some(now) = self.now.as_mut() else {
            return;
        };
        now.people
            .rows
            .retain(|person| &person.row.thread_id != thread_id);
        now.due_soon
            .todos
            .retain(|todo| todo.todo.thread_id.as_ref() != Some(thread_id));
        if now
            .reading
            .as_ref()
            .is_some_and(|pick| &pick.thread_id == thread_id)
        {
            now.reading = None;
        }
        let shown = now.people.rows.len()
            + now.due_soon.todos.len()
            + usize::from(now.updates.is_some())
            + usize::from(now.reading.is_some());
        now.item_count = u32::try_from(shown).unwrap_or(u32::MAX);
    }
}
