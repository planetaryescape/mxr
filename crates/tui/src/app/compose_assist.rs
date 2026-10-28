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
                        let review = self.compose.assist_review.as_ref().cloned()?;
                        let (proposal_action, proposal_id) = review
                            .proposal()
                            .map(|proposal| (proposal.action, proposal.proposal_id.clone()))?;
                        let instruction = review.note_input.unwrap_or_default();
                        let text = mxr_compose::body_regions::split_body(&review.pending.body)
                            .text
                            .to_string();
                        let request = Request::ComposeAssist {
                            draft: mxr_protocol::DraftTargetData::Stored {
                                draft_id: review.pending.draft_id(),
                            },
                            text,
                            selection: None,
                            action: Some(proposal_action),
                            instruction: Some(instruction),
                            adjust: mxr_protocol::AssistAdjustData::default(),
                            model: review
                                .proposal_models
                                .get(review.current_index)
                                .cloned()
                                .flatten(),
                            revise: Some(proposal_id),
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
                        if let Some(review) = self.compose.assist_review.as_mut() {
                            review.note_input = None;
                            review.request_pending = true;
                        }
                        None
                    }
                    (KeyCode::Esc, _) => {
                        if let Some(review) = self.compose.assist_review.as_mut() {
                            if review.request_pending {
                                self.compose.assist_request_id =
                                    self.compose.assist_request_id.wrapping_add(1);
                                review.request_pending = false;
                            }
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
            let action = proof_action_for_key(key);
            return match action {
                Some("ComposeProofModel") => {
                    let review = self.compose.assist_review.as_ref()?;
                    if review.request_pending {
                        return None;
                    }
                    let pending = review.pending.clone();
                    self.queue_platform_request(
                        Request::GetComposeModels,
                        "Choose a writing model",
                        "Loading compose models...",
                    );
                    if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut() {
                        dispatch.compose_model_picker =
                            Some((Box::new(pending), ComposeModelPickerPurpose::RetryProof));
                    }
                    None
                }
                // Keep mine: the canonical send review and original editor
                // file remain untouched until a proposal is accepted.
                Some("ComposeProofKeep") => {
                    if self
                        .compose
                        .assist_review
                        .as_ref()
                        .is_some_and(|review| review.request_pending)
                    {
                        self.compose.assist_request_id =
                            self.compose.assist_request_id.wrapping_add(1);
                        self.modals.platform.close();
                    }
                    self.compose.assist_review = None;
                    None
                }
                Some("ComposeProofAccept" | "ComposeProofAcceptEdit") => {
                    let reopen_editor = action == Some("ComposeProofAcceptEdit");
                    if let Some(review) = self.compose.assist_review.take() {
                        let Some(proposal) = review.proposal().cloned() else {
                            self.compose.assist_review = Some(review);
                            return None;
                        };
                        let pending = review.pending;
                        let draft_id = pending.draft_id();
                        self.queue_platform_request_after(
                            vec![Request::ApplyComposeProposal {
                                proposal_id: proposal.proposal_id,
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
                Some("ComposeProofRevise") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.note_input = Some(String::new());
                    }
                    None
                }
                Some("ComposeProofOpenSource") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        if let Some(proposal) = review.proposal() {
                            let sources = &proposal.provenance.voice_examples;
                            if sources.is_empty() {
                                self.status_message = Some("No voice examples were used".into());
                            } else {
                                let index = review.voice_source_index % sources.len();
                                let source = &sources[index];
                                self.mailbox.pending_invite_open = Some(source.message_id.clone());
                                self.compose.viewing_voice_source = true;
                                self.compose.voice_source_request_id =
                                    self.compose.voice_source_request_id.wrapping_add(1);
                                self.status_message = Some(format!(
                                    "Opening voice example {} of {}…",
                                    index + 1,
                                    sources.len()
                                ));
                                review.voice_source_index = (index + 1) % sources.len();
                            }
                        }
                    }
                    None
                }
                Some("ComposeProofPrevious" | "ComposeProofNext") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        let last = review.proposals.len().saturating_sub(1);
                        let index = if action == Some("ComposeProofPrevious") {
                            review.current_index.saturating_sub(1)
                        } else {
                            review.current_index.saturating_add(1).min(last)
                        };
                        review.select_version(index);
                    }
                    None
                }
                Some("ComposeProofScrollUp") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.proof_scroll_offset = review.proof_scroll_offset.saturating_sub(1);
                    }
                    None
                }
                Some("ComposeProofScrollDown") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.proof_scroll_offset = review.proof_scroll_offset.saturating_add(1);
                    }
                    None
                }
                Some("ComposeProofPageUp") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.proof_scroll_offset = review.proof_scroll_offset.saturating_sub(8);
                    }
                    None
                }
                Some("ComposeProofPageDown") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.proof_scroll_offset = review.proof_scroll_offset.saturating_add(8);
                    }
                    None
                }
                Some("ComposeProofScrollTop") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.proof_scroll_offset = 0;
                    }
                    None
                }
                Some("ComposeProofScrollBottom") => {
                    if let Some(review) = self.compose.assist_review.as_mut() {
                        review.proof_scroll_offset = u16::MAX;
                    }
                    None
                }
                _ => None,
            };
        }
        None
    }

    pub(super) fn handle_compose_model_picker_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => self.compose.model_picker = None,
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if let Some(picker) = self.compose.model_picker.as_mut() {
                    picker.selected_index = picker.selected_index.saturating_sub(1);
                }
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if let Some(picker) = self.compose.model_picker.as_mut() {
                    picker.selected_index = picker
                        .selected_index
                        .saturating_add(1)
                        .min(picker.choices.choices.len());
                }
            }
            (KeyCode::Enter, _) => {
                let picker = self.compose.model_picker.as_ref()?;
                let model = if picker.selected_index == 0 {
                    None
                } else {
                    let Some(choice) = picker.choices.choices.get(picker.selected_index - 1) else {
                        self.status_message = Some("No compose models are configured".into());
                        return None;
                    };
                    if !choice.available {
                        self.status_message = Some(
                            choice
                                .unavailable_reason
                                .clone()
                                .unwrap_or_else(|| "That compose model is unavailable".into()),
                        );
                        return None;
                    }
                    Some(choice.id.clone())
                };
                let picker = self.compose.model_picker.take()?;
                match picker.purpose {
                    ComposeModelPickerPurpose::InitialDraft => {
                        self.request_compose_assist(*picker.pending, model);
                    }
                    ComposeModelPickerPurpose::RetryProof => {
                        let review = self.compose.assist_review.as_ref().cloned()?;
                        if review.request_pending
                            || review.pending.draft_path != picker.pending.draft_path
                        {
                            return None;
                        }
                        let proposal = review.proposal()?;
                        let text = mxr_compose::body_regions::split_body(&review.pending.body)
                            .text
                            .to_string();
                        let request = Request::ComposeAssist {
                            draft: mxr_protocol::DraftTargetData::Stored {
                                draft_id: review.pending.draft_id(),
                            },
                            text,
                            selection: None,
                            action: Some(proposal.action),
                            instruction: None,
                            adjust: mxr_protocol::AssistAdjustData::default(),
                            model: model.clone(),
                            revise: None,
                        };
                        self.queue_platform_request(
                            request,
                            "Writing proof",
                            format!(
                                "Trying {} with the same draft text...",
                                model.as_deref().unwrap_or("the compose default")
                            ),
                        );
                        if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut() {
                            self.compose.assist_request_id =
                                self.compose.assist_request_id.wrapping_add(1);
                            dispatch.compose_assist = Some(Box::new(review.pending.clone()));
                            dispatch.compose_assist_request_id =
                                Some(self.compose.assist_request_id);
                        }
                        if let Some(review) = self.compose.assist_review.as_mut() {
                            review.request_pending = true;
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}

fn proof_action_for_key(key: crossterm::event::KeyEvent) -> Option<&'static str> {
    let spelling = crate::app::key_dispatch::spelling(key)?;
    let (_, binding) = mxr_keymap::defaults().tui_binding("compose-assist-proof", &spelling)?;
    let action = binding.name.as_str();
    match action {
        "ComposeProofAccept" => Some("ComposeProofAccept"),
        "ComposeProofAcceptEdit" => Some("ComposeProofAcceptEdit"),
        "ComposeProofRevise" => Some("ComposeProofRevise"),
        "ComposeProofModel" => Some("ComposeProofModel"),
        "ComposeProofKeep" => Some("ComposeProofKeep"),
        "ComposeProofOpenSource" => Some("ComposeProofOpenSource"),
        "ComposeProofPrevious" => Some("ComposeProofPrevious"),
        "ComposeProofNext" => Some("ComposeProofNext"),
        "ComposeProofScrollUp" => Some("ComposeProofScrollUp"),
        "ComposeProofScrollDown" => Some("ComposeProofScrollDown"),
        "ComposeProofPageUp" => Some("ComposeProofPageUp"),
        "ComposeProofPageDown" => Some("ComposeProofPageDown"),
        "ComposeProofScrollTop" => Some("ComposeProofScrollTop"),
        "ComposeProofScrollBottom" => Some("ComposeProofScrollBottom"),
        _ => None,
    }
}
