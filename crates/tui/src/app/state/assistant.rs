
use mxr_protocol::{
    AssistantAvailabilityData, AssistantConversationData, AssistantConversationSummaryData,
    AssistantDeleteData, AssistantItemData, AssistantLinkData, AssistantModelsData,
    AssistantPendingData, AssistantProgressData, AssistantStatusData, Request,
};
use std::collections::HashSet;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssistantFocus {
    #[default]
    Away,
    Input,
    Pane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantPrompt {
    RejectReason { call_id: String, text: String },
    Other { text: String },
    ConfirmDelete(AssistantDeleteData, super::ConfirmChoice),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantCall {
    Availability,
    Conversation,
    Refetch {
        conversation_id: String,
    },
    List,
    Models,
    SetModel,
    DeletePreview,
    Delete,
}

#[derive(Debug, Clone)]
pub struct AssistantOutbound {
    pub call: AssistantCall,
    pub request: Request,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnswerDraft {
    pub selected: Vec<String>,
    pub other: String,
}

impl AnswerDraft {
    pub fn is_complete(&self) -> bool {
        !self.selected.is_empty() || !self.other.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct AssistantListState {
    pub loading: bool,
    pub conversations: Vec<AssistantConversationSummaryData>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AssistantModelsState {
    pub loading: bool,
    pub models: Option<AssistantModelsData>,
    pub selected: usize,
}

#[derive(Debug, Default)]
pub struct AssistantState {
    pub open: bool,
    pub focus: AssistantFocus,
    pub availability: Option<AssistantAvailabilityData>,
    pub conversation_id: Option<String>,
    pub conversation: Option<AssistantConversationData>,
    pub progress: Option<AssistantProgressData>,
    pub started_at: Option<Instant>,
    pub phrase_since: Option<Instant>,
    pub sending: bool,
    pub busy: bool,
    pub input: String,
    pub input_sent: Option<String>,
    pub prompt: Option<AssistantPrompt>,
    pub answers_for: Option<String>,
    pub answers: Vec<AnswerDraft>,
    pub active_question: usize,
    pub exact_call_for: Option<String>,
    pub full_list_calls: HashSet<String>,
    pub steps_expanded: bool,
    pub scroll_from_bottom: u16,
    pub max_scroll: u16,
    pub list: Option<AssistantListState>,
    pub models: Option<AssistantModelsState>,
    pub error: Option<String>,
    pub outbound: Vec<AssistantOutbound>,
    refetch_in_flight: bool,
    refetch_again: bool,
}

impl AssistantState {
    pub fn queue(&mut self, call: AssistantCall, request: Request) {
        self.outbound.push(AssistantOutbound { call, request });
    }

    pub fn take_outbound(&mut self) -> Vec<AssistantOutbound> {
        std::mem::take(&mut self.outbound)
    }

    pub fn overlay_open(&self) -> bool {
        self.list.is_some() || self.models.is_some()
    }

    pub fn running(&self) -> bool {
        self.sending
            || self
                .conversation
                .as_ref()
                .is_some_and(|conversation| conversation.status == AssistantStatusData::Running)
    }

    pub fn available(&self) -> bool {
        self.availability
            .as_ref()
            .is_none_or(|availability| availability.available)
    }

    pub fn pending(&self) -> &[AssistantPendingData] {
        match &self.conversation {
            Some(conversation) if conversation.status == AssistantStatusData::AwaitingUser => {
                &conversation.pending
            }
            _ => &[],
        }
    }

    pub fn linked_reply(&self) -> Option<(usize, Vec<AssistantLinkData>)> {
        let items = &self.conversation.as_ref()?.items;
        items
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, item)| match item {
                AssistantItemData::Assistant { text, .. } => {
                    let (_, links) = crate::ui::assistant_describe::plain_text_with_links(text);
                    (!links.is_empty()).then_some((index, links))
                }
                _ => None,
            })
    }

    pub fn reply_links(&self) -> Vec<AssistantLinkData> {
        self.linked_reply()
            .map(|(_, links)| links)
            .unwrap_or_default()
    }

    pub fn set_progress(&mut self, progress: Option<AssistantProgressData>, now: Instant) {
        if self.phrase_since.is_none() || progress != self.progress {
            self.phrase_since = Some(now);
        }
        self.progress = progress;
    }

    pub fn auto_approve(&self) -> bool {
        self.conversation
            .as_ref()
            .is_some_and(|conversation| conversation.auto_approve)
    }

    pub fn refetch(&mut self) {
        let Some(conversation_id) = self.conversation_id.clone() else {
            return;
        };
        if self.refetch_in_flight {
            self.refetch_again = true;
            return;
        }
        self.refetch_in_flight = true;
        self.queue(
            AssistantCall::Refetch {
                conversation_id: conversation_id.clone(),
            },
            Request::GetAssistantConversation { conversation_id },
        );
    }

    pub fn refetch_settled(&mut self) -> bool {
        self.refetch_in_flight = false;
        std::mem::take(&mut self.refetch_again)
    }

    pub fn reset_conversation(&mut self) {
        self.conversation_id = None;
        self.conversation = None;
        self.progress = None;
        self.started_at = None;
        self.phrase_since = None;
        self.sending = false;
        self.input_sent = None;
        self.busy = false;
        self.prompt = None;
        self.answers_for = None;
        self.answers.clear();
        self.active_question = 0;
        self.exact_call_for = None;
        self.full_list_calls.clear();
        self.steps_expanded = false;
        self.scroll_from_bottom = 0;
        self.error = None;
    }
}
