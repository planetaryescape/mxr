//! The assistant pane: a conversation with the daemon's assistant, shown
//! beside whatever screen is open. Turns run in the daemon, so hiding the
//! pane never stops one; the pane only mirrors the daemon's conversation.

use mxr_protocol::{
    AssistantAvailabilityData, AssistantConversationData, AssistantConversationSummaryData,
    AssistantDeleteData, AssistantItemData, AssistantLinkData, AssistantModelsData,
    AssistantPendingData, AssistantProgressData, AssistantStatusData, Request,
};
use std::collections::HashSet;
use std::time::Instant;

/// Where keys go while the pane is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssistantFocus {
    /// The screen behind the pane has the keys.
    #[default]
    Away,
    /// Typing into the message box.
    Input,
    /// The pane's own keys: decisions, scrolling, conversation keys.
    Pane,
}

/// A one-line text prompt the pane is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantPrompt {
    /// Why an approval is rejected (optional).
    RejectReason { call_id: String, text: String },
    /// The user's own answer to the active question.
    Other { text: String },
    /// The daemon's dry run of deleting the open conversation, or the one
    /// highlighted in the history list, with which option is lit.
    ConfirmDelete(AssistantDeleteData, super::ConfirmChoice),
}

/// What a request was for, so its answer lands in the right place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantCall {
    Availability,
    /// Send, decide, cancel, auto-approve and get all answer with the
    /// conversation.
    Conversation,
    /// A refetch after an `AssistantUpdated` event.
    Refetch {
        conversation_id: String,
    },
    List,
    /// The model choices, or the choice just set.
    Models,
    SetModel,
    DeletePreview,
    Delete,
}

/// One request for the runtime to send.
#[derive(Debug, Clone)]
pub struct AssistantOutbound {
    pub call: AssistantCall,
    pub request: Request,
}

/// An answer being written to one question.
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

/// The past-conversations list.
#[derive(Debug, Clone, Default)]
pub struct AssistantListState {
    pub loading: bool,
    pub conversations: Vec<AssistantConversationSummaryData>,
    pub selected: usize,
}

/// The model picker: the daemon's choices and the highlighted row.
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
    /// What the running turn is doing now, from the latest event.
    pub progress: Option<AssistantProgressData>,
    /// When the running turn started, for the elapsed seconds.
    pub started_at: Option<Instant>,
    /// When the live phrase last changed, for the slow-model note.
    pub phrase_since: Option<Instant>,
    /// A send is on its way and the daemon has not answered yet.
    pub sending: bool,
    /// A decision, cancel or setting change is on its way.
    pub busy: bool,
    pub input: String,
    /// The message on its way, shown until the conversation comes back.
    pub input_sent: Option<String>,
    pub prompt: Option<AssistantPrompt>,
    /// The waiting question's call and the answers being written to it.
    pub answers_for: Option<String>,
    pub answers: Vec<AnswerDraft>,
    /// Which question number keys answer when there are several.
    pub active_question: usize,
    /// The approval whose exact call (tool and arguments) is unfolded.
    /// Kept by call id so the next approval starts folded, as on the web.
    pub exact_call_for: Option<String>,
    /// Waiting approvals whose full message list is unfolded. Kept by call
    /// id so a new approval starts with its list folded.
    pub full_list_calls: HashSet<String>,
    /// Settled tool steps are folded to "N steps" unless this is set.
    pub steps_expanded: bool,
    /// Lines scrolled up from the newest; 0 keeps the newest in view.
    pub scroll_from_bottom: u16,
    /// The furthest the transcript can scroll, from the last draw.
    pub max_scroll: u16,
    pub list: Option<AssistantListState>,
    pub models: Option<AssistantModelsState>,
    /// The last request that failed, in the daemon's words.
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

    /// A list the pane shows in place of the transcript.
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

    /// What waits for the user now.
    pub fn pending(&self) -> &[AssistantPendingData] {
        match &self.conversation {
            Some(conversation) if conversation.status == AssistantStatusData::AwaitingUser => {
                &conversation.pending
            }
            _ => &[],
        }
    }

    /// The latest reply that links an email or a person, and its links
    /// numbered `1`-`9` in the order it shows them. A later reply without
    /// links ("anything else?") leaves the earlier one's open.
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

    /// The links `1`-`9` open: [`Self::linked_reply`]'s.
    pub fn reply_links(&self) -> Vec<AssistantLinkData> {
        self.linked_reply()
            .map(|(_, links)| links)
            .unwrap_or_default()
    }

    /// Records the running turn's progress. The phrase's clock restarts only
    /// when the progress changes, so the slow-model note counts from there.
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

    /// Refetch the open conversation. Events can come faster than answers,
    /// so at most one refetch is in flight and one more follows it.
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

    /// A refetch answered; true when another was asked for meanwhile.
    pub fn refetch_settled(&mut self) -> bool {
        self.refetch_in_flight = false;
        std::mem::take(&mut self.refetch_again)
    }

    /// Start over with no conversation open.
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
