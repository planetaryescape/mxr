use super::*;
use mxr_protocol::Request;

impl App {
    pub(crate) fn schedule_draft_cleanup(&mut self, path: std::path::PathBuf) {
        if !self.compose.pending_draft_cleanup.contains(&path) {
            self.compose.pending_draft_cleanup.push(path);
        }
    }

    pub(crate) fn take_pending_draft_cleanup(&mut self) -> Vec<std::path::PathBuf> {
        std::mem::take(&mut self.compose.pending_draft_cleanup)
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
        let draft = mxr_core::Draft {
            id: mxr_core::id::DraftId::new(),
            account_id: pending.account_id.clone(),
            from: mxr_compose::draft_codec::parse_from_field(&pending.fm.from),
            reply_headers,
            intent: pending.intent,
            to: parse_addrs(&pending.fm.to),
            cc: parse_addrs(&pending.fm.cc),
            bcc: parse_addrs(&pending.fm.bcc),
            subject: pending.fm.subject,
            body_markdown: pending.body,
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
        self.queue_mutation(
            Request::SendDraft {
                draft,
                override_safety_token,
            },
            MutationEffect::SentSuccess {
                status: "Sent!".into(),
                remind_at,
                sent_message_id: None,
            },
            "Sending...".into(),
        );
        self.schedule_draft_cleanup(pending.draft_path);
    }
}
