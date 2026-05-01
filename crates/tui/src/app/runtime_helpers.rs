use super::*;

impl App {
    pub async fn load(&mut self, client: &mut Client) -> Result<(), MxrError> {
        self.mailbox.labels = client.list_labels().await?;
        self.mailbox.all_envelopes = client.list_envelopes(5000, 0).await?;
        self.load_initial_mailbox(client).await?;
        self.mailbox.saved_searches = client.list_saved_searches().await.unwrap_or_default();
        self.set_subscriptions(client.list_subscriptions(500).await.unwrap_or_default());
        if let Ok(Response::Ok {
            data:
                ResponseData::Status {
                    uptime_secs,
                    daemon_pid,
                    accounts,
                    total_messages,
                    sync_statuses,
                    ..
                },
        }) = client.raw_request(Request::GetStatus).await
        {
            self.apply_status_snapshot(
                uptime_secs,
                daemon_pid,
                accounts,
                total_messages,
                sync_statuses,
            );
        }
        // Queue body prefetch for first visible window
        self.queue_body_window();
        Ok(())
    }

    pub(super) async fn load_initial_mailbox(
        &mut self,
        client: &mut Client,
    ) -> Result<(), MxrError> {
        let Some(inbox_id) = self
            .mailbox
            .labels
            .iter()
            .find(|label| label.name == "INBOX")
            .map(|label| label.id.clone())
        else {
            self.mailbox.envelopes = self.all_mail_envelopes();
            self.mailbox.active_label = None;
            return Ok(());
        };

        match client
            .raw_request(Request::ListEnvelopes {
                label_id: Some(inbox_id.clone()),
                account_id: None,
                limit: 5000,
                offset: 0,
            })
            .await
        {
            Ok(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                self.mailbox.envelopes = envelopes;
                self.mailbox.active_label = Some(inbox_id);
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_label_fetch = None;
                self.mailbox.sidebar_selected = 0;
                Ok(())
            }
            Ok(Response::Error { message }) => {
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.active_label = None;
                self.status_message = Some(format!("Inbox load failed: {message}"));
                Ok(())
            }
            Ok(_) => {
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.active_label = None;
                self.status_message = Some("Inbox load failed: unexpected response".into());
                Ok(())
            }
            Err(error) => {
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.active_label = None;
                self.status_message = Some(format!("Inbox load failed: {error}"));
                Ok(())
            }
        }
    }

    pub fn apply_status_snapshot(
        &mut self,
        uptime_secs: u64,
        daemon_pid: Option<u32>,
        accounts: Vec<String>,
        total_messages: u32,
        sync_statuses: Vec<mxr_protocol::AccountSyncStatus>,
    ) {
        self.diagnostics.page.uptime_secs = Some(uptime_secs);
        self.diagnostics.page.daemon_pid = daemon_pid;
        self.diagnostics.page.accounts = accounts;
        self.diagnostics.page.total_messages = Some(total_messages);
        self.diagnostics.page.sync_statuses = sync_statuses;
        self.last_sync_status = Some(Self::summarize_sync_status(
            &self.diagnostics.page.sync_statuses,
        ));
    }

    pub fn input_pending(&self) -> bool {
        self.input.is_pending()
    }

    pub fn next_background_timeout(&self, fallback: Duration) -> Duration {
        let mut timeout = fallback;
        if let Some(pending) = self.mailbox.pending_preview_read.as_ref() {
            timeout = timeout.min(
                pending
                    .due_at
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        if let Some(pending) = self.search.pending_debounce.as_ref() {
            timeout = timeout.min(
                pending
                    .due_at
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        if self.search_is_pending() {
            timeout = timeout.min(SEARCH_SPINNER_TICK);
        }
        timeout
    }

    pub fn expire_pending_preview_read_for_tests(&mut self) {
        if let Some(pending) = self.mailbox.pending_preview_read.as_mut() {
            pending.due_at = Instant::now();
        }
    }

    pub fn set_terminal_image_support(&mut self, support: TerminalImageSupport) {
        self.html_image_support = Some(support);
    }
}
