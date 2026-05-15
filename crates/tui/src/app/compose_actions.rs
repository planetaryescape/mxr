use super::*;

impl App {
    pub(super) fn apply_compose_action(&mut self, action: Action) {
        match action {
            Action::Compose => {
                // Build contacts from known envelopes (senders we've seen)
                let mut seen = std::collections::HashMap::new();
                for env in &self.mailbox.all_envelopes {
                    seen.entry(env.from.email.clone()).or_insert_with(|| {
                        crate::ui::compose_picker::Contact {
                            name: env.from.name.clone().unwrap_or_default(),
                            email: env.from.email.clone(),
                        }
                    });
                }
                let mut contacts: Vec<_> = seen.into_values().collect();
                contacts.sort_by(|a, b| a.email.to_lowercase().cmp(&b.email.to_lowercase()));
                self.compose.compose_picker.open_to(contacts);
            }
            Action::Reply => {
                if let Some(env) = self.context_envelope() {
                    let message_id = env.id.clone();
                    let account_id = env.account_id.clone();
                    self.dispatch_or_defer_reply(message_id, account_id, false);
                }
            }
            Action::ReplyAll => {
                if let Some(env) = self.context_envelope() {
                    let message_id = env.id.clone();
                    let account_id = env.account_id.clone();
                    self.dispatch_or_defer_reply(message_id, account_id, true);
                }
            }
            Action::Forward => {
                if let Some(env) = self.context_envelope() {
                    self.compose.pending_compose = Some(ComposeAction::Forward {
                        message_id: env.id.clone(),
                        account_id: env.account_id.clone(),
                    });
                }
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }

    /// Pick the fastest available path for `r`/`a`:
    /// - **Cache hit**: skip the IPC entirely (instant editor).
    /// - **Prewarm in flight**: park the action in `deferred_compose` so
    ///   the prewarm result handler dispatches it. Firing a duplicate
    ///   `PrepareReply` here would queue *behind* the in-flight prewarm
    ///   on the serial IPC worker, doubling the wait.
    /// - **Otherwise** (cold path): fire the action with no preloaded
    ///   context — `handle_compose_action` does the IPC.
    pub(super) fn dispatch_or_defer_reply(
        &mut self,
        message_id: mxr_core::MessageId,
        account_id: mxr_core::AccountId,
        reply_all: bool,
    ) {
        let cached = self
            .compose
            .reply_context_cache
            .get(&message_id)
            .and_then(|pair| {
                if reply_all {
                    pair.reply_all.clone()
                } else {
                    pair.reply.clone()
                }
            });
        if cached.is_some() {
            self.compose.pending_compose =
                Some(reply_action(reply_all, message_id, account_id, cached));
            return;
        }
        let prewarm_running = self
            .compose
            .last_prewarmed_message_id
            .as_ref()
            .is_some_and(|id| id == &message_id);
        if prewarm_running {
            self.compose.deferred_compose = Some(DeferredCompose {
                message_id,
                account_id,
                reply_all,
            });
            self.status_message = Some("Preparing reply…".into());
            return;
        }
        self.compose.pending_compose = Some(reply_action(reply_all, message_id, account_id, None));
    }
}

fn reply_action(
    reply_all: bool,
    message_id: mxr_core::MessageId,
    account_id: mxr_core::AccountId,
    preloaded: Option<mxr_protocol::ReplyContext>,
) -> ComposeAction {
    if reply_all {
        ComposeAction::ReplyAll {
            message_id,
            account_id,
            preloaded,
        }
    } else {
        ComposeAction::Reply {
            message_id,
            account_id,
            preloaded,
        }
    }
}
