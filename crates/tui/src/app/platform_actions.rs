use super::*;

impl App {
    pub(super) fn apply_platform_action(&mut self, action: Action) {
        match action {
            Action::DraftAssistCurrentThread => {
                let Some(env) = self.context_envelope().cloned() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                self.compose.assist_after_editor = true;
                self.compose.pending_compose = Some(ComposeAction::Reply {
                    message_id: env.id.clone(),
                    account_id: env.account_id.clone(),
                    preloaded: None,
                });
            }
            Action::DraftWithOptions => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                self.modals.draft_options.open(ThreadKey::of_envelope(env));
            }
            Action::DraftNewForSender => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No sender selected".into());
                    return;
                };
                self.queue_platform_request(
                    Request::DraftCompose {
                        account_id: Some(env.account_id.clone()),
                        to: Some(Address {
                            name: env.from.name.clone(),
                            email: env.from.email.clone(),
                        }),
                        instruction: format!(
                            "Follow up on our conversation \"{}\".",
                            env.subject.trim()
                        ),
                        // A new email to this person, not a reply in the
                        // thread (a source message would make it a reply).
                        source_message_id: None,
                        thread_id: None,
                        register: None,
                        length_hint: None,
                    },
                    "Draft for sender",
                    "Generating new draft from relationship profile...",
                );
            }
            Action::RefinePendingDraft => {
                let Some(pending) = self.compose.pending_send_confirm.as_ref() else {
                    self.status_message = Some("No draft to refine".into());
                    return;
                };
                if pending.mode == PendingSendMode::DraftOnlyNoRecipients {
                    self.status_message = Some("Add a recipient before refining".into());
                    return;
                }
                let pending = pending.clone();
                self.queue_platform_request(
                    Request::GetComposeModels,
                    "Choose a writing model",
                    "Loading compose models...",
                );
                if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut() {
                    dispatch.compose_model_picker =
                        Some((Box::new(pending), ComposeModelPickerPurpose::InitialDraft));
                }
            }
            Action::OpenVoiceProfile => {
                let Some(account_id) = self.platform_account_id() else {
                    self.status_message = Some("No account available".into());
                    return;
                };
                self.queue_platform_request(
                    Request::GetUserVoice { account_id },
                    "Voice profile",
                    "Loading user voice profile...",
                );
            }
            Action::RebuildUserVoice => {
                let Some(account_id) = self.platform_account_id() else {
                    self.status_message = Some("No account available".into());
                    return;
                };
                self.queue_platform_request(
                    Request::RebuildUserVoice { account_id },
                    "Voice profile",
                    "Rebuilding user voice profile...",
                );
            }
            Action::OpenCommitments => {
                let Some(account_id) = self.platform_account_id() else {
                    self.status_message = Some("No account available".into());
                    return;
                };
                let email = self.context_envelope().map(|env| env.from.email.clone());
                self.queue_platform_request(
                    Request::ListCommitments {
                        account_id,
                        email,
                        status: Some(mxr_protocol::CommitmentStatusData::Open),
                    },
                    "Open commitments",
                    "Loading open commitments...",
                );
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }

    /// Confirm the Draft Options modal: draft a reply to the stored thread
    /// using the chosen tone/length overrides (or auto when left unset).
    pub(crate) fn submit_draft_options_modal(&mut self) {
        let Some(thread) = self.modals.draft_options.thread.clone() else {
            self.modals.draft_options.close();
            return;
        };
        let register = self.modals.draft_options.register();
        let length = self.modals.draft_options.length();
        let instruction = self.modals.draft_options.instruction.trim().to_string();
        self.modals.draft_options.close();
        self.queue_platform_request(
            Request::DraftCompose {
                account_id: thread.account_id.clone(),
                to: None,
                instruction,
                source_message_id: None,
                thread_id: Some(thread.thread_id.clone()),
                register,
                length_hint: length,
            },
            "Draft assist",
            "Generating reply draft with your tone...",
        );
    }

    pub(crate) fn queue_platform_request(
        &mut self,
        request: Request,
        title: impl Into<String>,
        loading: impl Into<String>,
    ) {
        let title = title.into();
        let loading = loading.into();
        self.modals
            .platform
            .open_loading(title.clone(), loading.clone());
        self.modals
            .pending_platform_dispatch
            .push(PendingPlatformDispatch {
                compose: None,
                compose_assist: None,
                compose_assist_request_id: None,
                compose_model_picker: None,
                compose_assist_accept: None,
                compose_assist_reopen_editor: false,
                prelude: Vec::new(),
                request,
                title,
                loading,
            });
    }

    pub(crate) fn queue_platform_request_after(
        &mut self,
        prelude: Vec<Request>,
        request: Request,
        title: impl Into<String>,
        loading: impl Into<String>,
    ) {
        let title = title.into();
        let loading = loading.into();
        self.modals
            .platform
            .open_loading(title.clone(), loading.clone());
        self.modals
            .pending_platform_dispatch
            .push(PendingPlatformDispatch {
                compose: None,
                compose_assist: None,
                compose_assist_request_id: None,
                compose_model_picker: None,
                compose_assist_accept: None,
                compose_assist_reopen_editor: false,
                prelude,
                request,
                title,
                loading,
            });
    }

    pub(crate) fn take_pending_platform_dispatch(&mut self) -> Vec<PendingPlatformDispatch> {
        std::mem::take(&mut self.modals.pending_platform_dispatch)
    }

    pub(crate) fn request_compose_assist(&mut self, pending: PendingSend, model: Option<String>) {
        let draft_id = pending.draft_id();
        let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
        let reply_headers =
            pending
                .fm
                .in_reply_to
                .as_ref()
                .map(|in_reply_to| mxr_core::types::ReplyHeaders {
                    in_reply_to: in_reply_to.clone(),
                    references: pending.fm.references.clone(),
                    thread_id: pending.fm.thread_id.clone(),
                });
        let now = chrono::Utc::now();
        let mut draft = mxr_core::Draft {
            revision: None,
            id: draft_id.clone(),
            account_id: pending.account_id.clone(),
            from: mxr_compose::draft_codec::parse_from_field(&pending.fm.from)
                .ok()
                .flatten(),
            reply_headers,
            intent: pending.intent,
            to: parse_addrs(&pending.fm.to),
            cc: parse_addrs(&pending.fm.cc),
            bcc: parse_addrs(&pending.fm.bcc),
            subject: pending.fm.subject.clone(),
            content: mxr_core::DraftContent::markdown(pending.body.clone()),
            managed_attachments: None,
            attachments: pending
                .fm
                .attach
                .iter()
                .map(std::path::PathBuf::from)
                .collect(),
            inline_assets: Vec::new(),
            inline_calendar_reply: pending.invite_reply.clone(),
            created_at: now,
            updated_at: now,
        };
        if let Err(error) = mxr_compose::forward::apply(&mut draft, &pending.fm) {
            self.push_toast(Toast::error(error.to_string()));
            return;
        }
        let text = mxr_compose::body_regions::split_body(&pending.body)
            .text
            .to_string();
        self.queue_platform_request_after(
            vec![Request::SaveDraft { draft }],
            Request::ComposeAssist {
                draft: mxr_protocol::DraftTargetData::Stored { draft_id },
                text,
                selection: None,
                action: None,
                instruction: None,
                adjust: mxr_protocol::AssistAdjustData::default(),
                model,
                revise: None,
            },
            "Writing proof",
            "Preparing a proof against your current draft...",
        );
        if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut() {
            dispatch.compose_assist = Some(Box::new(pending));
            self.compose.assist_request_id = self.compose.assist_request_id.wrapping_add(1);
            dispatch.compose_assist_request_id = Some(self.compose.assist_request_id);
        }
    }

    fn platform_account_id(&self) -> Option<mxr_core::AccountId> {
        self.context_envelope()
            .map(|env| env.account_id.clone())
            .or_else(|| {
                self.accounts
                    .page
                    .accounts
                    .iter()
                    .find(|account| account.is_default && account.enabled)
                    .or_else(|| {
                        self.accounts
                            .page
                            .accounts
                            .iter()
                            .find(|account| account.enabled)
                    })
                    .map(|account| account.account_id.clone())
            })
    }
}
