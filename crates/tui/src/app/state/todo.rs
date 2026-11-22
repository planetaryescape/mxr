use mxr_core::id::MessageId;
use mxr_protocol::{
    ModeGuideData, TodoCatchupData, TodoChangeData, TodoData, TodoResolutionActionData,
    TodoRunwayData,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoPanel {
    #[default]
    Runway,
    Expired,
    Catchup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoOpen {
    pub message_id: MessageId,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoListFetch {
    Expired,
    Catchup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoPromptKind {
    Edit {
        todo_id: String,
    },
    Correct {
        todo_id: String,
    },
    /// `t` on a conversation: what to do, starting with the verb.
    Create {
        message_id: MessageId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoPromptState {
    pub pending: Option<crate::app::MutationId>,
    pub kind: TodoPromptKind,
    pub input: String,
    pub error: Option<String>,
    pub selected: usize,
}

impl TodoPromptState {
    pub fn new(kind: TodoPromptKind, input: String) -> Self {
        Self {
            pending: None,
            kind,
            input,
            error: None,
            selected: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TodoPageState {
    pub uncertain_open: bool,
    pub uncertain_selected: usize,
    pub runway: Option<TodoRunwayData>,
    pub guide: Option<ModeGuideData>,
    pub panel: TodoPanel,
    pub expired: Vec<TodoData>,
    pub catchup: Option<TodoCatchupData>,
    pub catchup_preview: Option<TodoChangeData>,
    pub prompt: Option<TodoPromptState>,
    pub pending_refresh: bool,
    pub pending_open: Option<TodoOpen>,
    pub pending_mark_seen: bool,
    pub expired_on_open: u32,
    pub pending_list: Option<TodoListFetch>,
    pub pending_catchup_preview: bool,
    pub pending_resolution: Option<(String, String, TodoResolutionActionData)>,
    pub resolution_preview: Option<(String, String, TodoChangeData)>,
    pub microsoft_status: Option<mxr_protocol::MicrosoftTodoStatusData>,
    pub microsoft_preview: Option<mxr_protocol::MicrosoftTodoSendData>,
    pub microsoft_sync_preview: Option<mxr_protocol::MicrosoftTodoSyncData>,
    pub pending_microsoft: Option<mxr_protocol::Request>,
    pub microsoft_busy: bool,
    pub microsoft_review_visible: std::cell::Cell<bool>,
    pub microsoft_sync_review_visible: std::cell::Cell<bool>,
    pub microsoft_scroll_max: std::cell::Cell<u16>,
    pub microsoft_scroll: u16,
    pub peek: Option<String>,
}

impl TodoPageState {
    pub fn peek_body<'a>(
        &self,
        bodies: &'a std::collections::HashMap<MessageId, mxr_core::MessageBody>,
    ) -> Option<&'a str> {
        let peek = self.peek.as_deref()?;
        let todo = self.rows().into_iter().find(|todo| todo.id == peek)?;
        bodies
            .get(todo.source_message_id.as_ref()?)?
            .text_plain
            .as_deref()
    }

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
}
