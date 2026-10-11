use super::*;
use mxr_protocol::Request;

impl App {
    pub(crate) fn apply_retained_send_identity(
        &mut self,
        account: mxr_core::AccountId,
        identity: mxr_core::SendIdentity,
    ) {
        if self
            .compose
            .last_send_identity
            .as_ref()
            .is_none_or(|(current_account, current)| {
                current_account == &account
                    && current.operation_id.split(':').next()
                        == identity.operation_id.split(':').next()
                    && current.expected_revision <= identity.expected_revision
            })
        {
            self.compose.last_send_identity = Some((account, identity));
            self.compose.last_send_query = None;
        }
    }
    pub(crate) fn compose_file_owned(&self, path: &std::path::Path) -> bool {
        self.compose.active_draft_cleanup.contains(path)
            || self.compose.pending_schedule_files.contains(path)
            || self
                .compose
                .pending_send_mutations
                .values()
                .any(|(pending, _)| pending.draft_path == path)
            || self
                .compose
                .pending_draft_cleanup
                .iter()
                .any(|pending| pending == path)
    }
    pub(crate) fn restore_failed_compose(&mut self, pending: PendingSend) {
        if self.compose.pending_send_confirm.is_none() && self.compose.pending_compose.is_none() {
            self.compose.pending_send_confirm = Some(pending);
        } else {
            self.compose.failed_send = Some(pending);
        }
    }

    pub(crate) fn finish_compose_schedule(&mut self, pending: PendingSend, accepted: bool) {
        self.compose
            .pending_schedule_files
            .remove(&pending.draft_path);
        if accepted {
            self.schedule_draft_cleanup(pending.draft_path);
        } else {
            self.restore_failed_compose(pending);
        }
    }

    pub(crate) fn schedule_draft_cleanup(&mut self, path: std::path::PathBuf) {
        if !self.compose.pending_draft_cleanup.contains(&path) {
            self.compose.pending_draft_cleanup.push(path);
        }
    }

    pub(crate) fn take_pending_draft_cleanup(&mut self) -> Vec<std::path::PathBuf> {
        let paths = std::mem::take(&mut self.compose.pending_draft_cleanup);
        self.compose
            .active_draft_cleanup
            .extend(paths.iter().cloned());
        paths
    }

    /// First default-and-enabled account id, if any. Used by IPC
    /// dispatchers that need an account id but don't have one in
    /// scope (e.g. sidebar lens refresh).
    pub fn default_account_id(&self) -> Option<&mxr_core::AccountId> {
        self.accounts
            .page
            .accounts
            .iter()
            .find(|a| a.is_default && a.enabled)
            .map(|a| &a.account_id)
            .or_else(|| {
                self.accounts
                    .page
                    .accounts
                    .iter()
                    .find(|a| a.enabled)
                    .map(|a| &a.account_id)
            })
    }

    /// Build a Draft from the PendingSend and dispatch SendDraft via
    /// the mutation queue. Shared between the regular `[s] send` path
    /// and the `[Ctrl-O] override + send` path. `override_safety_token`
    /// is `Some(token)` only on the override path.
    pub(crate) fn dispatch_send_pending(
        &mut self,
        pending: PendingSend,
        override_safety_token: Option<String>,
    ) {
        self.dispatch_send_pending_with_reminder(pending, override_safety_token, None);
    }

    pub(crate) fn dispatch_send_pending_with_reminder(
        &mut self,
        pending: PendingSend,
        override_safety_token: Option<String>,
        remind_at: Option<chrono::DateTime<chrono::Utc>>,
    ) {
        // Keep the reviewed file reachable even when local preparation fails.
        self.compose.pending_send_confirm = Some(pending.clone());
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
        let retained = match crate::send_recovery_state::for_draft(&pending.draft_id()) {
            Ok(retained) => retained,
            Err(error) => {
                self.push_toast(Toast::error(format!("Cannot read send recovery: {error}")));
                return;
            }
        };
        if retained
            .as_ref()
            .is_some_and(|(account, _)| account != &pending.account_id)
        {
            self.push_toast(Toast::error("Reviewed send belongs to another account"));
            return;
        }
        let revision = retained
            .as_ref()
            .map_or(1, |(_, identity)| identity.expected_revision);
        let draft = mxr_core::Draft {
            revision: Some(revision),
            id: pending.draft_id(),
            account_id: pending.account_id.clone(),
            // `fm.from` was validated when the PendingSend was built; a parse
            // error is unreachable here, so fall back to "no override".
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
        let identity = retained.map_or_else(
            || mxr_core::SendIdentity::for_draft(&draft),
            |(_, identity)| identity,
        );
        self.compose.last_send_identity = Some((draft.account_id.clone(), identity.clone()));
        self.compose.last_send_query = None;
        #[cfg(not(test))]
        {
            if let Err(error) = crate::send_recovery_state::retain_compose(
                &draft.account_id,
                &draft.id,
                &identity,
                &pending.draft_path,
            ) {
                self.push_toast(Toast::error(format!(
                    "Cannot retain send identity: {error}"
                )));
                return;
            }
        }
        let id = self.queue_mutation(
            Request::SendDraft {
                send_identity: Some(identity),
                draft,
                override_safety_token,
            },
            MutationEffect::SentSuccess {
                status: "Sent!".into(),
                remind_at,
                sent_message_id: None,
                draft_path: Some(pending.draft_path.clone()),
            },
            "Sending...".into(),
        );
        self.compose.pending_send_confirm = None;
        self.compose
            .pending_send_mutations
            .insert(id, (pending, true));
    }
}
