use crate::state::AppState;
use mxr_config::{ChimeConfig, ChimeEvent, ChimeSound};
use mxr_core::AccountId;
use mxr_protocol::{
    ClientKind, DaemonEvent, IpcMessage, IpcPayload, MutationCommand, Request, Response,
    ResponseData,
};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChimePlayback {
    pub sound: ChimeSound,
    pub played: bool,
}

/// One alert per account failure period. The set contains only accounts with
/// a current sync failure and is cleared by that account's `SyncCompleted`.
#[derive(Default)]
pub(crate) struct SyncErrorChimes(Mutex<HashSet<AccountId>>);

impl SyncErrorChimes {
    pub(crate) fn retain_accounts(&self, accounts: &HashSet<AccountId>) {
        self.0
            .lock()
            .retain(|account_id| accounts.contains(account_id));
    }

    fn should_play(&self, event: &DaemonEvent) -> bool {
        match event {
            DaemonEvent::SyncError { account_id, .. } => self.0.lock().insert(account_id.clone()),
            DaemonEvent::SyncCompleted { account_id, .. } => {
                self.0.lock().remove(account_id);
                false
            }
            DaemonEvent::OperationFailed {
                operation,
                account_id: Some(account_id),
                ..
            } if operation == "sync" => self.0.lock().insert(account_id.clone()),
            _ => true,
        }
    }
}

pub(crate) fn preview(config: &ChimeConfig, event: ChimeEvent) -> ChimePlayback {
    let sound = config.sound_for(event);
    let playback = ChimePlayback {
        sound,
        played: sound != ChimeSound::None,
    };
    if playback.played {
        spawn_playback(sound, config.volume);
    }
    playback
}

pub(crate) fn emit_daemon_event(state: &AppState, event: DaemonEvent) {
    play_for_daemon_event(state, &event);
    let message = IpcMessage {
        id: 0,
        source: ClientKind::default(),
        payload: IpcPayload::Event(event),
    };
    let _ = state.event_tx.send(message);
}

pub(crate) fn play_for_daemon_event(
    state: &AppState,
    event: &DaemonEvent,
) -> Option<ChimePlayback> {
    if !state.sync_error_chimes.should_play(event) {
        return None;
    }
    let chime_event = event_for_daemon_event(event)?;
    Some(play_configured(
        &state.config_snapshot().notifications.chimes,
        chime_event,
    ))
}

pub(crate) fn play_for_request_response(
    state: &AppState,
    source: ClientKind,
    request: &Request,
    response: &Response,
) -> Option<ChimePlayback> {
    if !plays_request_chimes(source) {
        return None;
    }
    let chime_event = event_for_request_response(request, response)?;
    Some(play_configured(
        &state.config_snapshot().notifications.chimes,
        chime_event,
    ))
}

/// The web plays its own chimes in the browser (where the person is, which
/// may not be this machine), so the daemon playing too would double every
/// sound. Daemon events such as new mail still chime for everyone.
pub(crate) fn plays_request_chimes(source: ClientKind) -> bool {
    source != ClientKind::Web
}

pub(crate) fn play_for_archive_completion(
    state: &AppState,
    source: ClientKind,
    completed: u32,
) -> Option<ChimePlayback> {
    let event = event_for_archive_completion(source, completed)?;
    Some(play_configured(
        &state.config_snapshot().notifications.chimes,
        event,
    ))
}

fn event_for_archive_completion(source: ClientKind, completed: u32) -> Option<ChimeEvent> {
    (plays_request_chimes(source) && completed > 0).then_some(ChimeEvent::Archived)
}

pub(crate) fn play_configured(config: &ChimeConfig, event: ChimeEvent) -> ChimePlayback {
    let playback = playback_decision(config, event);
    if playback.played {
        spawn_playback(playback.sound, config.volume);
    }
    playback
}

pub(crate) fn playback_decision(config: &ChimeConfig, event: ChimeEvent) -> ChimePlayback {
    let sound = config.sound_for(event);
    ChimePlayback {
        sound,
        played: config.enabled && sound != ChimeSound::None,
    }
}

pub(crate) fn event_for_daemon_event(event: &DaemonEvent) -> Option<ChimeEvent> {
    match event {
        DaemonEvent::NewMessages { .. } => Some(ChimeEvent::NewMail),
        DaemonEvent::MessageUnsnoozed { .. } => Some(ChimeEvent::Unsnoozed),
        DaemonEvent::ReminderTriggered { .. } | DaemonEvent::ReplyLaterReturned { .. } => {
            Some(ChimeEvent::Reminder)
        }
        DaemonEvent::SyncError { .. }
        | DaemonEvent::OperationFailed { .. }
        | DaemonEvent::MutationReconciliationFailed { .. } => Some(ChimeEvent::Error),
        DaemonEvent::SyncCompleted { .. }
        | DaemonEvent::LabelCountsUpdated { .. }
        | DaemonEvent::OperationStarted { .. }
        | DaemonEvent::OperationProgress { .. }
        | DaemonEvent::OperationCompleted { .. }
        | DaemonEvent::OperationCancelled { .. }
        // Internal resync signal, not a user-facing notification.
        | DaemonEvent::EventsLagged { .. }
        // A list line filling in is quiet by design.
        | DaemonEvent::ThreadGistReady { .. }
        // A move or newly sorted mail refreshes views; it is not news.
        | DaemonEvent::ModesChanged { .. } => None,
    }
}

pub(crate) fn event_for_request_response(req: &Request, response: &Response) -> Option<ChimeEvent> {
    let Response::Ok { data } = response else {
        return None;
    };

    match (req, data) {
        (
            Request::SendDraft { .. } | Request::SendStoredDraft { .. },
            ResponseData::SendReceipt { .. },
        ) => Some(ChimeEvent::Sent),
        (Request::Mutation { mutation, .. }, ResponseData::MutationResult { result })
            if result.succeeded > 0 =>
        {
            event_for_mutation(mutation)
        }
        (Request::Snooze { .. }, ResponseData::Ack) => Some(ChimeEvent::Snoozed),
        (Request::Unsnooze { .. }, ResponseData::Ack) => Some(ChimeEvent::Unsnoozed),
        _ => None,
    }
}

fn event_for_mutation(command: &MutationCommand) -> Option<ChimeEvent> {
    match command {
        MutationCommand::Archive { .. }
        | MutationCommand::ReadAndArchive { .. }
        | MutationCommand::Route { archive: true, .. } => Some(ChimeEvent::Archived),
        MutationCommand::Trash { .. } => Some(ChimeEvent::Trashed),
        MutationCommand::Spam { .. } => Some(ChimeEvent::Spam),
        MutationCommand::Star { .. }
        | MutationCommand::SetRead { .. }
        | MutationCommand::ModifyLabels { .. }
        | MutationCommand::Move { .. }
        | MutationCommand::Route { .. } => None,
    }
}

fn spawn_playback(sound: ChimeSound, volume: f32) {
    let _ = std::thread::Builder::new()
        .name("mxr-chime".into())
        .spawn(move || {
            if let Err(error) = run_player(sound, volume) {
                tracing::warn!(%error, "notification chime playback failed");
            }
        });
}

fn run_player(sound: ChimeSound, volume: f32) -> anyhow::Result<()> {
    let status = Command::new(chime_player_path())
        .arg("--sound")
        .arg(sound_arg(sound))
        .arg("--volume")
        .arg(volume.clamp(0.0, 1.0).to_string())
        .status()?;

    if !status.success() {
        anyhow::bail!("chime player exited with status {status}");
    }
    Ok(())
}

fn chime_player_path() -> PathBuf {
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            return dir.join(chime_player_filename());
        }
    }
    PathBuf::from(chime_player_filename())
}

fn chime_player_filename() -> &'static str {
    if cfg!(windows) {
        "mxr-chime-player.exe"
    } else {
        "mxr-chime-player"
    }
}

fn sound_arg(sound: ChimeSound) -> &'static str {
    match sound {
        ChimeSound::None => "none",
        ChimeSound::Bell => "bell",
        ChimeSound::Glass => "glass",
        ChimeSound::Pop => "pop",
        ChimeSound::Sent => "sent",
        ChimeSound::Archive => "archive",
        ChimeSound::Thud => "thud",
        ChimeSound::Alert => "alert",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::{AccountId, DraftId, MessageId};
    use mxr_protocol::{
        AccountMutationResultData, DaemonEvent, MutationCommand, MutationResultData, Request,
        Response, ResponseData,
    };

    #[test]
    fn disabled_config_suppresses_playback_without_losing_event_sound() {
        let config = ChimeConfig::default();

        let playback = playback_decision(&config, ChimeEvent::NewMail);

        assert_eq!(playback.sound, ChimeSound::Bell);
        assert!(!playback.played);
    }

    #[test]
    fn request_chime_fires_for_successful_send_and_archive() {
        let send_request = Request::SendStoredDraft {
            managed_attachments_reviewed: true,
            send_identity: Some(mxr_core::SendIdentity::for_revision(&DraftId::new(), 1)),
            draft_id: DraftId::new(),
            override_safety_token: None,
        };
        let send_response = Response::Ok {
            data: ResponseData::SendReceipt {
                local_message_id: MessageId::new(),
                provider_message_id: Some("provider-1".into()),
                rfc2822_message_id: "<sent@example.com>".into(),
            },
        };
        assert_eq!(
            event_for_request_response(&send_request, &send_response),
            Some(ChimeEvent::Sent)
        );

        let archive_request = Request::mutation(MutationCommand::Archive {
            message_ids: vec![MessageId::new()],
        });
        let archive_response = Response::Ok {
            data: ResponseData::MutationResult {
                result: mutation_result(1),
            },
        };
        assert_eq!(
            event_for_request_response(&archive_request, &archive_response),
            Some(ChimeEvent::Archived)
        );
    }

    #[test]
    fn web_requests_leave_their_chimes_to_the_browser() {
        assert!(!plays_request_chimes(ClientKind::Web));
        assert!(plays_request_chimes(ClientKind::Tui));
        assert!(plays_request_chimes(ClientKind::Cli));
    }

    #[test]
    fn recoverable_archive_chimes_only_for_new_completed_provider_effects() {
        for source in [ClientKind::Cli, ClientKind::Tui] {
            assert_eq!(
                event_for_archive_completion(source, 2),
                Some(ChimeEvent::Archived)
            );
            assert_eq!(event_for_archive_completion(source, 0), None);
        }
        assert_eq!(event_for_archive_completion(ClientKind::Web, 2), None);
    }

    #[test]
    fn request_chime_ignores_failed_or_noop_archive() {
        let request = Request::mutation(MutationCommand::Archive {
            message_ids: vec![MessageId::new()],
        });
        let noop_response = Response::Ok {
            data: ResponseData::MutationResult {
                result: mutation_result(0),
            },
        };
        assert_eq!(event_for_request_response(&request, &noop_response), None);

        let error_response = Response::error("provider failed");
        assert_eq!(event_for_request_response(&request, &error_response), None);
    }

    #[test]
    fn daemon_event_chime_maps_new_messages_to_new_mail() {
        let event = DaemonEvent::NewMessages {
            envelopes: Vec::new(),
            total: 0,
        };

        assert_eq!(event_for_daemon_event(&event), Some(ChimeEvent::NewMail));
    }

    #[test]
    fn sync_failure_chime_is_once_until_that_account_recovers() {
        let notifications = SyncErrorChimes::default();
        let account = AccountId::new();
        let error = || DaemonEvent::SyncError {
            account_id: account.clone(),
            error: "offline".into(),
        };

        assert!(notifications.should_play(&error()));
        assert!(!notifications.should_play(&error()));
        assert!(!notifications.should_play(&DaemonEvent::OperationFailed {
            operation_id: "sync-1".into(),
            operation: "sync".into(),
            account_id: Some(account.clone()),
            error: "offline".into(),
            retryable: true,
        }));
        assert!(!notifications.should_play(&DaemonEvent::OperationFailed {
            operation_id: "sync-2".into(),
            operation: "sync".into(),
            account_id: Some(account.clone()),
            error: "offline".into(),
            retryable: true,
        }));

        notifications.should_play(&DaemonEvent::SyncCompleted {
            account_id: account.clone(),
            messages_synced: 0,
        });
        assert!(notifications.should_play(&error()));
    }

    #[test]
    fn sync_chime_state_is_per_account_and_keeps_other_operation_failures() {
        let notifications = SyncErrorChimes::default();
        let account = AccountId::new();
        let other_account = AccountId::new();
        assert!(notifications.should_play(&DaemonEvent::SyncError {
            account_id: account.clone(),
            error: "offline".into(),
        }));
        assert!(notifications.should_play(&DaemonEvent::SyncError {
            account_id: other_account.clone(),
            error: "offline".into(),
        }));
        assert!(notifications.should_play(&DaemonEvent::OperationFailed {
            operation_id: "action-1".into(),
            operation: "rebuild_analytics".into(),
            account_id: Some(account.clone()),
            error: "failed".into(),
            retryable: true,
        }));
        assert!(!notifications.should_play(&DaemonEvent::SyncError {
            account_id: account,
            error: "offline".into(),
        }));
    }

    #[test]
    fn removed_accounts_can_chime_again_if_they_return() {
        let notifications = SyncErrorChimes::default();
        let account = AccountId::new();
        let error = || DaemonEvent::SyncError {
            account_id: account.clone(),
            error: "offline".into(),
        };

        assert!(notifications.should_play(&error()));
        notifications.retain_accounts(&HashSet::new());
        assert!(notifications.should_play(&error()));
    }

    fn mutation_result(succeeded: u32) -> MutationResultData {
        MutationResultData {
            requested: 1,
            succeeded,
            skipped: 1 - succeeded,
            failed: 0,
            accounts: vec![AccountMutationResultData {
                account_id: AccountId::new(),
                account_name: "Test".into(),
                succeeded,
                skipped: 1 - succeeded,
                failed: 0,
                error: None,
            }],
            mutation_id: None,
            undo_unavailable: false,
        }
    }
}
