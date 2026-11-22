use mxr_core::id::ThreadId;
use mxr_protocol::{
    ArrivalsData, ModeGuideData, NotSureData, NowData, NowPersonData, NowReadingPickData,
    NowTodoData, NowUpdatesCardData, UpdatesLetGoData,
};

#[derive(Debug, Clone, Copy)]
pub enum NowRow<'a> {
    Arrivals(&'a ArrivalsData),
    NotSure(&'a NotSureData),
    Person(&'a NowPersonData),
    Todo(&'a NowTodoData),
    Updates(&'a NowUpdatesCardData),
    Reading(&'a NowReadingPickData),
}

impl NowRow<'_> {
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

pub const ARRIVALS_WHY: &str =
    "Every email since then, counted once by where it went. Enter lists them.";

#[derive(Debug, Clone, Default)]
pub struct NowPageState {
    pub now: Option<NowData>,
    pub guide: Option<ModeGuideData>,
    pub arrivals: Option<ArrivalsData>,
    pub pending_mark_seen: bool,
    pub pending_refresh: bool,
    pub pending_digest_preview: bool,
    pub digest_preview: Option<UpdatesLetGoData>,
}

impl NowPageState {
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

    pub fn remove_todo(&mut self, todo_id: &str) {
        if let Some(now) = self.now.as_mut() {
            now.due_soon.todos.retain(|todo| todo.todo.id != todo_id);
        }
    }

    pub fn arrivals_rows(&self) -> Vec<NowRow<'_>> {
        let Some(arrivals) = self.arrivals.as_ref().filter(|_| self.now.is_some()) else {
            return Vec::new();
        };
        std::iter::once(NowRow::Arrivals(arrivals))
            .chain(arrivals.not_sure.iter().map(NowRow::NotSure))
            .collect()
    }

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
