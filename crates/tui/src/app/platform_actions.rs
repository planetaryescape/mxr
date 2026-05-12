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
                    Request::DraftAssist {
                        thread_id: env.thread_id.clone(),
                        instruction: "Draft a concise reply.".into(),
                    },
                    "Draft assist",
                    "Generating relationship-aware reply draft...",
                );
            }
            Action::DraftNewForSender => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No sender selected".into());
                    return;
                };
                self.queue_platform_request(
                    Request::DraftNew {
                        account_id: env.account_id.clone(),
                        to: Address {
                            name: env.from.name.clone(),
                            email: env.from.email.clone(),
                        },
                        purpose: format!(
                            "Follow up on the selected thread: {}",
                            env.subject.trim()
                        ),
                        register: None,
                        length_hint: None,
                    },
                    "Draft for sender",
                    "Generating new draft from relationship profile...",
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
