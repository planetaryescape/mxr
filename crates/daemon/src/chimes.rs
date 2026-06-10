use crate::state::AppState;
use mxr_config::{ChimeConfig, ChimeEvent, ChimeSound};
use mxr_protocol::{
    ClientKind, DaemonEvent, IpcMessage, IpcPayload, MutationCommand, Request, Response,
    ResponseData,
};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChimePlayback {
    pub sound: ChimeSound,
    pub played: bool,
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
    let chime_event = event_for_daemon_event(event)?;
    Some(play_configured(
        &state.config_snapshot().notifications.chimes,
        chime_event,
    ))
}

pub(crate) fn play_for_request_response(
    state: &AppState,
    request: &Request,
    response: &Response,
) -> Option<ChimePlayback> {
    let chime_event = event_for_request_response(request, response)?;
    Some(play_configured(
        &state.config_snapshot().notifications.chimes,
        chime_event,
    ))
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
        DaemonEvent::ReminderTriggered { .. } => Some(ChimeEvent::Reminder),
        DaemonEvent::SyncError { .. }
        | DaemonEvent::OperationFailed { .. }
        | DaemonEvent::MutationReconciliationFailed { .. } => Some(ChimeEvent::Error),
        DaemonEvent::SyncCompleted { .. }
        | DaemonEvent::LabelCountsUpdated { .. }
        | DaemonEvent::OperationStarted { .. }
        | DaemonEvent::OperationProgress { .. }
        | DaemonEvent::OperationCompleted { .. }
        | DaemonEvent::OperationCancelled { .. } => None,
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
        };

        assert_eq!(event_for_daemon_event(&event), Some(ChimeEvent::NewMail));
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
