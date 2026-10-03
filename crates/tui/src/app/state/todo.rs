//! To do in the TUI: the runway the daemon serves (`GetTodoRunway`), the
//! mode's teaching copy (`GetModeGuide`), the Expired list and the
//! one-time catch-up. The daemon owns every band, label and why line; this
//! state only holds what came back and what the user is doing with it.

use mxr_core::id::MessageId;
use mxr_protocol::{ModeGuideData, TodoCatchupData, TodoChangeData, TodoData, TodoRunwayData};

/// Which list the lens shows under its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoPanel {
    /// Now, Coming up, Later and Whenever.
    #[default]
    Runway,
    /// Rows past their window or let go, one key from restore.
    Expired,
    /// The first run's one-time batch: keep or let go of each row.
    Catchup,
}

/// The email a row is about, to open with its link marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoOpen {
    pub message_id: MessageId,
    /// The link the row is about; marked in the message, never opened.
    pub link: Option<String>,
}

/// A list the runtime should fetch for the lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoListFetch {
    Expired,
    Catchup,
}

/// What the prompt under the lens is collecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoPromptKind {
    /// `Z`: when the row should show up, previewed as it is typed.
    Schedule { todo_id: String },
    /// `,`: `field=value`, such as `due=fri 9 oct` or `title=Pay rent`.
    Edit { todo_id: String },
    /// `t` on a conversation: what to do, starting with the verb.
    Create { message_id: MessageId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoPromptState {
    pub kind: TodoPromptKind,
    pub input: String,
    pub time: crate::ui::time_preview::TimePreview,
    pub error: Option<String>,
}

impl TodoPromptState {
    pub fn new(kind: TodoPromptKind, input: String) -> Self {
        Self {
            kind,
            input,
            time: crate::ui::time_preview::TimePreview::default(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TodoPageState {
    pub runway: Option<TodoRunwayData>,
    pub guide: Option<ModeGuideData>,
    pub panel: TodoPanel,
    pub expired: Vec<TodoData>,
    pub catchup: Option<TodoCatchupData>,
    /// "Let go of all" previewed by the daemon, waiting for Enter or Esc.
    /// Enter lets go of exactly these ids.
    pub catchup_preview: Option<TodoChangeData>,
    /// Closed here before the daemon answered, so it never flickers back.
    pub card_closed: bool,
    pub prompt: Option<TodoPromptState>,
    /// Ask the runtime for the runway and the guide.
    pub pending_refresh: bool,
    /// Ask the runtime for the source message of the row under the cursor.
    pub pending_open: Option<TodoOpen>,
    /// The `SetModeGuideSeen` sent when the card closed here, so a failed
    /// write can show the card again.
    pub card_close_mutation: Option<crate::app::MutationId>,
    /// The next refresh records that To do was opened. Only opening the
    /// lens does: a refresh after a change must not reset the count.
    pub pending_mark_seen: bool,
    /// "N expired since you last looked", as counted when the lens opened.
    pub expired_on_open: u32,
    pub pending_list: Option<TodoListFetch>,
    /// Ask the daemon what "let go of all" would let go of.
    pub pending_catchup_preview: bool,
}

impl TodoPageState {
    /// The runway's selectable rows in display order: Now, Coming up by
    /// week, Later, then Whenever.
    pub fn runway_rows(&self) -> Vec<&TodoData> {
        let Some(runway) = &self.runway else {
            return Vec::new();
        };
        runway
            .now
            .iter()
            .chain(runway.coming_up.iter().flat_map(|week| &week.todos))
            .chain(&runway.later)
            .chain(&runway.whenever)
            .collect()
    }

    /// The rows the cursor moves over in the panel on screen.
    pub fn rows(&self) -> Vec<&TodoData> {
        match self.panel {
            TodoPanel::Runway => self.runway_rows(),
            TodoPanel::Expired => self.expired.iter().collect(),
            TodoPanel::Catchup => self
                .catchup
                .as_ref()
                .map(|catchup| catchup.todos.iter().collect())
                .unwrap_or_default(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    /// The first-encounter card shows at the top once the mode has rows,
    /// until it is closed here or retired anywhere.
    pub fn card_visible(&self) -> bool {
        self.panel == TodoPanel::Runway
            && !self.card_closed
            && self.guide.as_ref().is_some_and(|guide| !guide.card_seen)
            && !self.runway_rows().is_empty()
    }
}
