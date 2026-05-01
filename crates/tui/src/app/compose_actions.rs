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
                    self.compose.pending_compose = Some(ComposeAction::Reply {
                        message_id: env.id.clone(),
                        account_id: env.account_id.clone(),
                    });
                }
            }
            Action::ReplyAll => {
                if let Some(env) = self.context_envelope() {
                    self.compose.pending_compose = Some(ComposeAction::ReplyAll {
                        message_id: env.id.clone(),
                        account_id: env.account_id.clone(),
                    });
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
}
