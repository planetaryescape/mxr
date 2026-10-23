use super::*;

impl App {
    pub(super) fn handle_compose_assist_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        if self.compose.assist_review.is_some() {
            if self
                .compose
                .assist_review
                .as_ref()
                .is_some_and(|review| review.note_input.is_some())
            {
                return match (key.code, key.modifiers) {
                    (KeyCode::Enter, _) => {
                        let Some(review) = self.compose.assist_review.as_ref().cloned() else {
                            return None;
                        };
                        let instruction = review.note_input.unwrap_or_default();
                        let text = mxr_composer::check::split_body(&review.pending.body)
                            .text
                            .to_string();
                        let request = Request::ComposeAssist {
                            draft: mxr_protocol::DraftTargetData::Stored {
                                draft_id: review.pending.draft_id(),
                            },
                            text,
                            selection: None,
                            action: Some(review.proposal.action),
                            instruction: Some(instruction),
                            adjust: mxr_protocol::AssistAdjustData::default(),
                            revise: Some(review.proposal.proposal_id),
                        };
                        self.queue_platform_request(
                            request,
                            "Writing proof",
                            "Revising the displayed proposal...",
                        );
                        if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut() {
                            self.compose.assist_request_id =
                                self.compose.assist_request_id.wrapping_add(1);
                            dispatch.compose_assist = Some(Box::new(review.pending));
                            dispatch.compose_assist_request_id =
                                Some(self.compose.assist_request_id);
                        }
                        None
                    }
                    (KeyCode::Esc, _) => {
                        if let Some(review) = self.compose.assist_review.as_mut() {
                            review.note_input = None;
                        }
                        None
                    }
                    (KeyCode::Backspace, _) => {
                        if let Some(review) = self.compose.assist_review.as_mut() {
                            if let Some(input) = review.note_input.as_mut() {
                                input.pop();
                            }
                        }
                        None
                    }
                    (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        if let Some(review) = self.compose.assist_review.as_mut() {
                            if let Some(input) = review.note_input.as_mut() {
                                input.push(c);
                            }
                        }
                        None
                    }
                    _ => None,
                };
            }
            return match (key.code, key.modifiers) {
                // Keep mine: the canonical send review and original editor
                // file remain untouched until a proposal is accepted.
                (KeyCode::Esc, _) => {
                    self.compose.assist_review = None;
                    None
                }
                (KeyCode::Char('y'), KeyModifiers::NONE)
                | (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    let reopen_editor = matches!(key.code, KeyCode::Char('e'));
                    if let Some(review) = self.compose.assist_review.take() {
                        if !review.proposal.gaps.is_empty() {
                            let labels = review
                                .proposal
                                .gaps
                                .iter()
                                .map(|gap| gap.label.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            self.status_message =
                                Some(format!("Fill these details before accepting: {labels}"));
                            self.compose.assist_review = Some(review);
                            return None;
                        }
                        let pending = review.pending;
                        let draft_id = pending.draft_id();
                        self.queue_platform_request_after(
                            vec![Request::ApplyComposeProposal {
                                proposal_id: review.proposal.proposal_id,
                                draft_id: draft_id.clone(),
                            }],
                            Request::GetDraft { draft_id },
                            "Accepting writing proof",
                            "Applying the proposal and rechecking send safety...",
                        );
                        if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut() {
                            dispatch.compose_assist_accept = Some(Box::new(pending));
                            dispatch.compose_assist_reopen_editor = reopen_editor;
                            self.compose.assist_request_id =
                                self.compose.assist_request_id.wrapping_add(1);
                            dispatch.compose_assist_request_id =
                                Some(self.compose.assist_request_id);
                        }
                    }
                    None
                }
                (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.note_input = Some(String::new());
                    }
                    None
                }
                _ => None,
            };
        }
        None
    }
}
