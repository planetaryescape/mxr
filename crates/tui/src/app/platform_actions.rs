use super::*;

impl App {
    pub(super) fn apply_platform_action(&mut self, action: Action) {
        match action {
            Action::DraftAssistCurrentThread => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                self.queue_platform_request(
                    Request::DraftCompose {
                        account_id: None,
                        to: None,
                        instruction: "Draft a concise reply.".into(),
                        source_message_id: None,
                        thread_id: Some(env.thread_id.clone()),
                        register: None,
                        length_hint: None,
                    },
                    "Draft assist",
                    "Generating relationship-aware reply draft...",
                );
            }
            Action::DraftWithOptions => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                self.modals.draft_options.open(env.thread_id.clone());
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
                            "Follow up on the selected thread: {}",
                            env.subject.trim()
                        ),
                        source_message_id: Some(env.id.clone()),
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
                if pending.mode != PendingSendMode::SendOrSave {
                    self.status_message = Some("Add a recipient before refining".into());
                    return;
                }
                let draft_id = mxr_core::DraftId::new();
                let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
                let reply_headers = pending.fm.in_reply_to.as_ref().map(|in_reply_to| {
                    mxr_core::types::ReplyHeaders {
                        in_reply_to: in_reply_to.clone(),
                        references: pending.fm.references.clone(),
                        thread_id: pending.fm.thread_id.clone(),
                    }
                });
                let now = chrono::Utc::now();
                let draft = mxr_core::Draft {
                    id: draft_id.clone(),
                    account_id: pending.account_id.clone(),
                    from: mxr_compose::draft_codec::parse_from_field(&pending.fm.from),
                    reply_headers,
                    intent: pending.intent,
                    to: parse_addrs(&pending.fm.to),
                    cc: parse_addrs(&pending.fm.cc),
                    bcc: parse_addrs(&pending.fm.bcc),
                    subject: pending.fm.subject.clone(),
                    body_markdown: pending.body.clone(),
                    attachments: pending
                        .fm
                        .attach
                        .iter()
                        .map(std::path::PathBuf::from)
                        .collect(),
                    inline_calendar_reply: pending.invite_reply.clone(),
                    created_at: now,
                    updated_at: now,
                };
                self.queue_platform_request_after(
                    vec![Request::SaveDraft { draft }],
                    Request::DraftRefine {
                        draft_id,
                        knobs: mxr_protocol::DraftRefineKnobsData::default(),
                    },
                    "Refined draft",
                    "Saving and refining current draft...",
                );
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
        let Some(thread_id) = self.modals.draft_options.thread_id.clone() else {
            self.modals.draft_options.close();
            return;
        };
        let register = self.modals.draft_options.register();
        let length = self.modals.draft_options.length();
        self.modals.draft_options.close();
        self.queue_platform_request(
            Request::DraftCompose {
                account_id: None,
                to: None,
                instruction: "Draft a concise reply.".into(),
                source_message_id: None,
                thread_id: Some(thread_id),
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
                prelude,
                request,
                title,
                loading,
            });
    }

    pub(crate) fn take_pending_platform_dispatch(&mut self) -> Vec<PendingPlatformDispatch> {
        std::mem::take(&mut self.modals.pending_platform_dispatch)
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
