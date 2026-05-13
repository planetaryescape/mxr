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

    /// Build a Draft from the PendingSend and dispatch SendDraft via
    /// the mutation queue. Shared between the regular `[s] send` path
    /// and the `[Ctrl-O] override + send` path. `override_safety_token`
    /// is `Some(token)` only on the override path.
    pub(crate) fn dispatch_send_pending(
        &mut self,
        pending: PendingSend,
        override_safety_token: Option<String>,
    ) {
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
            id: mxr_core::id::DraftId::new(),
            account_id: pending.account_id.clone(),
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
            },
            "Sending...".into(),
        );
        self.schedule_draft_cleanup(pending.draft_path);
    }
}
