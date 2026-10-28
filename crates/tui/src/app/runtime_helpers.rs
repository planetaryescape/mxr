use super::*;
use crate::async_result::StatusSnapshot;

impl App {
    pub async fn load(&mut self, client: &mut Client) -> Result<(), MxrError> {
        self.mailbox.labels = client.list_labels().await?;
        self.mailbox.all_envelopes = client.list_envelopes(5000, 0).await?;
        self.load_initial_mailbox(client).await?;
        if self.is_paired_remote_read_only() {
            self.queue_body_window();
            return Ok(());
        }
        self.mailbox.pending_commitment_counts_refresh = true;
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
                    degraded,
                    ..
                },
        }) = client.raw_request(Request::GetStatus).await
        {
            let freshness = match client.raw_request(FRESHNESS_REQUEST).await {
                Ok(Response::Ok {
                    data: ResponseData::Freshness { freshness },
                }) => Some(freshness),
                _ => None,
            };
            self.apply_status_snapshot(StatusSnapshot {
                uptime_secs,
                daemon_pid,
                accounts,
                total_messages,
                sync_statuses,
                degraded,
                freshness,
            });
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
                label_role: crate::client::unified_role_for(&self.mailbox.labels, &inbox_id),
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
            Ok(Response::Error { message, .. }) => {
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

    /// Take a `GetStatus` reading into the diagnostics page.
    ///
    /// A degraded snapshot is not a reading: the daemon answered without
    /// having read its database, so the counts and sync statuses in it are
    /// blank rather than zero. Keep whatever was last known and record that
    /// the daemon is too busy to say, instead of overwriting a real reading
    /// with emptiness.
    pub(crate) fn apply_status_snapshot(&mut self, snapshot: StatusSnapshot) {
        if let Some(freshness) = snapshot.freshness {
            self.freshness = Some(freshness);
        }
        self.diagnostics.page.uptime_secs = Some(snapshot.uptime_secs);
        self.diagnostics.page.daemon_pid = snapshot.daemon_pid;
        self.diagnostics.page.status_degraded = snapshot.degraded;
        if snapshot.degraded {
            return;
        }
        self.diagnostics.page.accounts = snapshot.accounts;
        self.diagnostics.page.total_messages = Some(snapshot.total_messages);
        self.diagnostics.page.sync_statuses = snapshot.sync_statuses;
        self.last_sync_status = Some(Self::summarize_sync_status(
            &self.diagnostics.page.sync_statuses,
        ));
    }

    /// Keep the last recovery snapshot when status omits this optional field,
    /// but invalidate previews whose owner can no longer be verified.
    pub(crate) fn apply_recovery_status_snapshot(
        &mut self,
        recovery: Option<mxr_core::RecoveryStatusData>,
    ) {
        let Some(recovery) = recovery else {
            self.diagnostics.page.recovery_preview = None;
            return;
        };

        let same_cached_owner = self
            .diagnostics
            .page
            .recovery
            .as_ref()
            .is_some_and(|cached| {
                cached.workspace_id == recovery.workspace_id
                    && cached.recovery_generation == recovery.recovery_generation
            });
        let same_preview_owner =
            self.diagnostics
                .page
                .recovery_preview
                .as_ref()
                .is_some_and(|preview| {
                    let owner = match preview {
                        crate::app::RecoveryPreview::Activation { owner }
                        | crate::app::RecoveryPreview::Draft { owner, .. } => owner,
                    };
                    owner.workspace_id == recovery.workspace_id
                        && owner.recovery_generation == recovery.recovery_generation
                });
        if !same_cached_owner || !same_preview_owner {
            self.diagnostics.page.recovery_preview = None;
        }
        self.diagnostics.page.recovery = Some(recovery);
    }

    pub fn input_pending(&self) -> bool {
        self.input.is_pending()
    }

    pub fn next_background_timeout(&self, fallback: Duration) -> Duration {
        self.next_background_timeout_at(fallback, Instant::now())
    }

    fn next_background_timeout_at(&self, fallback: Duration, now: Instant) -> Duration {
        let mut timeout = fallback;
        if let Some(pending) = self.mailbox.pending_preview_read.as_ref() {
            timeout = timeout.min(pending.due_at.saturating_duration_since(now));
        }
        if let Some(pending) = self.search.pending_debounce.as_ref() {
            timeout = timeout.min(pending.due_at.saturating_duration_since(now));
        }
        if self.search_is_pending() {
            timeout = timeout.min(SEARCH_SPINNER_TICK);
        }
        // The assistant's running line counts the seconds.
        if self.assistant.open && self.assistant.running() {
            timeout = timeout.min(Duration::from_secs(1));
        }

        if let Some(expires_in) = self.toasts.next_expiry(now) {
            timeout = timeout.min(expires_in);
        }
        for toast in self.toasts.visible(now) {
            if toast.action_hint.is_some() {
                timeout = timeout.min(next_countdown_tick(toast.created_at, now));
            }
        }
        if let Some(undo) = &self.pending_undo {
            let expires_at = undo.applied_at + super::UNDO_HINT_TTL;
            timeout = timeout.min(expires_at.saturating_duration_since(now));
            if undo.applied_at <= now {
                timeout = timeout.min(next_countdown_tick(undo.applied_at, now));
            }
        }
        if let Some(prompt) = self.promise_prompts.front() {
            let expires_at = prompt.shown_at + super::promises::PROMISE_PROMPT_TTL;
            timeout = timeout.min(expires_at.saturating_duration_since(now));
            if prompt.shown_at <= now {
                timeout = timeout.min(next_countdown_tick(prompt.shown_at, now));
            }
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

fn next_countdown_tick(started_at: Instant, now: Instant) -> Duration {
    let elapsed = now.saturating_duration_since(started_at);
    Duration::from_secs(1).saturating_sub(Duration::from_nanos(u64::from(elapsed.subsec_nanos())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Toast, ToastSeverity};

    #[test]
    fn background_timeout_wakes_for_toast_expiry_then_stops_scheduling_it() {
        let mut app = App::new();
        let started_at = Instant::now();
        app.toasts.push(Toast {
            text: "Completed".into(),
            severity: ToastSeverity::Success,
            created_at: started_at,
            ttl: Duration::from_secs(5),
            action_hint: None,
        });

        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), started_at),
            Duration::from_secs(5)
        );
        let expiry = started_at + Duration::from_secs(5);
        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), expiry),
            Duration::ZERO
        );
        app.toasts.sweep_expired(expiry);
        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), expiry),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn background_timeout_refreshes_undo_countdown_and_clears_expired_undo() {
        let mut app = App::new();
        let started_at = Instant::now();
        app.pending_undo = Some(crate::app::PendingUndo {
            action: crate::app::UndoAction::Mutations(Vec::new()),
            verb_past: "Archived".into(),
            count: 1,
            applied_at: started_at,
        });

        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), started_at),
            Duration::from_secs(1)
        );
        assert_eq!(
            app.next_background_timeout_at(
                Duration::from_secs(60),
                started_at + Duration::from_millis(1_400)
            ),
            Duration::from_millis(600)
        );
        let expiry = started_at + super::UNDO_HINT_TTL;
        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), expiry),
            Duration::ZERO
        );
        app.tick_pending_undo(expiry);
        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), expiry),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn existing_background_deadlines_still_win() {
        let mut app = App::new();
        let now = Instant::now();
        app.toasts.push(Toast {
            text: "Completed".into(),
            severity: ToastSeverity::Success,
            created_at: now,
            ttl: Duration::from_secs(5),
            action_hint: None,
        });
        app.mailbox.pending_preview_read = Some(crate::app::PendingPreviewRead {
            message_id: mxr_core::MessageId::new(),
            due_at: now + Duration::from_millis(100),
        });
        app.search.pending_debounce = Some(crate::app::state::PendingSearchDebounce {
            query: "test".into(),
            mode: mxr_core::SearchMode::Lexical,
            session_id: 1,
            due_at: now + Duration::from_millis(250),
        });

        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), now),
            Duration::from_millis(100)
        );
        app.mailbox.pending_preview_read = None;
        assert_eq!(
            app.next_background_timeout_at(Duration::from_secs(60), now),
            Duration::from_millis(250)
        );
    }
}
