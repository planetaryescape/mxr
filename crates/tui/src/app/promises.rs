//! Promises on send in the TUI: after a send, the daemon checks the sent
//! message for promises ("I'll send the deck by Friday"). Each dated one is
//! offered as a one-line prompt: `y` keeps it as a reminder due at that
//! time, `n` or Esc lets it go. Prompts expire on their own; nothing is
//! stored unless the user says yes.

use super::{App, MutationEffect};
use crate::app::{Toast, ToastSeverity};
use mxr_core::MessageId;
use mxr_protocol::{PromiseDetectionData, PromiseDetectionStatusData, Request};
use std::time::{Duration, Instant};

/// Long enough to read and answer, short enough not to linger.
pub(crate) const PROMISE_PROMPT_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromisePrompt {
    pub message_id: MessageId,
    pub what: String,
    pub due_at: chrono::DateTime<chrono::Utc>,
    /// "Friday 2 October, 09:00", from the daemon's resolution.
    pub due_label: String,
    /// When the prompt became the one on screen.
    pub shown_at: Instant,
}

impl App {
    /// Queue a prompt for each dated promise the sent message makes.
    pub(crate) fn offer_promises(
        &mut self,
        message_id: MessageId,
        detection: PromiseDetectionData,
    ) {
        if detection.status != PromiseDetectionStatusData::Ready {
            return;
        }
        let now = Instant::now();
        for promise in detection.promises {
            let Some(due) = promise.due else {
                continue;
            };
            self.promise_prompts.push_back(PromisePrompt {
                message_id: message_id.clone(),
                what: promise.what,
                due_at: due.at,
                due_label: due.description,
                shown_at: now,
            });
        }
    }

    /// `y` (keep) or `n` (let go) for the prompt on screen.
    pub(crate) fn answer_promise(&mut self, keep: bool) {
        let Some(prompt) = self.promise_prompts.pop_front() else {
            return;
        };
        if keep {
            self.queue_mutation(
                Request::RecordPromise {
                    message_id: prompt.message_id,
                    what: prompt.what,
                    due_at: prompt.due_at,
                    dry_run: false,
                },
                MutationEffect::StatusOnly(format!("Reminder set for {}", prompt.due_label)),
                "Setting reminder...".into(),
            );
        }
        self.restart_next_promise(Instant::now());
    }

    /// Drop the prompt on screen once it has been up for its whole window.
    pub(crate) fn tick_promise_prompts(&mut self, now: Instant) {
        let expired = self.promise_prompts.front().is_some_and(|prompt| {
            now.saturating_duration_since(prompt.shown_at) >= PROMISE_PROMPT_TTL
        });
        if expired {
            self.promise_prompts.pop_front();
            self.restart_next_promise(now);
        }
    }

    fn restart_next_promise(&mut self, now: Instant) {
        if let Some(next) = self.promise_prompts.front_mut() {
            next.shown_at = now;
        }
    }

    /// The prompt on screen as a toast with its keys and a countdown,
    /// synthesized at draw time so answering removes it at once.
    pub fn pending_promise_toast(&self, now: Instant) -> Option<Toast> {
        let prompt = self.promise_prompts.front()?;
        let toast = Toast {
            text: format!(
                "You promised: {}. Remind me {}?",
                prompt.what, prompt.due_label
            ),
            severity: ToastSeverity::Info,
            created_at: prompt.shown_at,
            ttl: PROMISE_PROMPT_TTL,
            action_hint: Some("y remind · n not now".into()),
        };
        (!toast.expired(now)).then_some(toast)
    }
}
