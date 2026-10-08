//! Now in the TUI: the front page the daemon serves (`GetNow`), Now's
//! teaching copy (`GetModeGuide`) and the rail (`GetRail`). The daemon
//! owns every cap, count and line; this state only holds what came back
//! and what the user is doing with it.

use mxr_core::id::ThreadId;
use mxr_protocol::{
    ArrivalsData, ModeGuideData, NotSureData, NowData, NowPersonData, NowReadingPickData,
    NowTodoData, NowUpdatesCardData, UpdatesLetGoData,
};

/// One selectable row on Now, in the fixed section order.
#[derive(Debug, Clone, Copy)]
pub enum NowRow<'a> {
    /// The arrivals line under the headline (D119): Enter lists the
    /// emails behind it.
    Arrivals(&'a ArrivalsData),
    /// An email two rules disagreed about; one key answers where it goes.
    NotSure(&'a NotSureData),
    Person(&'a NowPersonData),
    Todo(&'a NowTodoData),
    Updates(&'a NowUpdatesCardData),
    Reading(&'a NowReadingPickData),
}

impl NowRow<'_> {
    /// The row's why line, for the footer.
    pub fn why(&self) -> &str {
        match self {
            Self::Arrivals(_) => ARRIVALS_WHY,
            Self::NotSure(question) => &question.line,
            Self::Person(person) => &person.why,
            Self::Todo(todo) => &todo.why,
            Self::Updates(card) => &card.headline,
            Self::Reading(pick) => &pick.why,
        }
    }
}

/// The footer line on the arrivals line.
pub const ARRIVALS_WHY: &str =
    "Every email since then, counted once by where it went. Enter lists them.";

#[derive(Debug, Clone, Default)]
pub struct NowPageState {
    pub now: Option<NowData>,
    pub guide: Option<ModeGuideData>,
    /// The arrivals line, Not-sure questions and the track record
    /// (`GetArrivals`), fetched beside Now.
    pub arrivals: Option<ArrivalsData>,
    /// Opening Now starts a visit: the next `GetArrivals` marks it, so the
    /// line counts from the visit before. A refresh doesn't.
    pub pending_mark_seen: bool,
    /// Ask the runtime for Now and its guide.
    pub pending_refresh: bool,
    /// Ask the daemon what letting go of the card's digest would do.
    pub pending_digest_preview: bool,
    /// Its dry run: Enter lets go of exactly this selection.
    pub digest_preview: Option<UpdatesLetGoData>,
}

impl NowPageState {
    /// Selectable rows in display order: the arrivals line and its Not-sure
    /// questions, then People, Due soon, the Updates card and the evening
    /// Reading pick.
    pub fn rows(&self) -> Vec<NowRow<'_>> {
        let mut rows: Vec<NowRow<'_>> = self.arrivals_rows();
        let Some(now) = &self.now else {
            return rows;
        };
        rows.extend(now.people.rows.iter().map(NowRow::Person));
        rows.extend(now.due_soon.todos.iter().map(NowRow::Todo));
        rows.extend(now.updates.iter().map(NowRow::Updates));
        rows.extend(now.reading.iter().map(NowRow::Reading));
        rows
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    /// Take a to-do with no email off Now before the daemon answers.
    pub fn remove_todo(&mut self, todo_id: &str) {
        if let Some(now) = self.now.as_mut() {
            now.due_soon.todos.retain(|todo| todo.todo.id != todo_id);
        }
    }

    /// The arrivals line (once Now has loaded) and its questions.
    pub fn arrivals_rows(&self) -> Vec<NowRow<'_>> {
        let Some(arrivals) = self.arrivals.as_ref().filter(|_| self.now.is_some()) else {
            return Vec::new();
        };
        std::iter::once(NowRow::Arrivals(arrivals))
            .chain(arrivals.not_sure.iter().map(NowRow::NotSure))
            .collect()
    }

    /// Take an answered question off Now before the daemon answers.
    pub fn remove_not_sure(&mut self, message_id: &mxr_core::id::MessageId) {
        if let Some(arrivals) = self.arrivals.as_mut() {
            arrivals
                .not_sure
                .retain(|question| &question.message_id != message_id);
            if arrivals.not_sure.is_empty() {
                arrivals.not_sure_line = None;
            }
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
