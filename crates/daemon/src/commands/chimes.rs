use crate::cli::{ChimeEventArg, ChimeSoundArg, ChimesAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{
    NotificationChimeEventData, NotificationChimeSoundData, NotificationChimesData,
    NotificationChimesPatchData, Request, Response, ResponseData,
};

pub async fn run(action: Option<ChimesAction>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let action = action.unwrap_or(ChimesAction::Status);
    let fmt = resolve_format(format);
    let mut client = IpcClient::connect().await?;

    match action {
        ChimesAction::Status => {
            let config = fetch_chimes(&mut client).await?;
            println!("{}", render_chimes(&config, fmt)?);
        }
        ChimesAction::Enable => {
            let saved = patch_chimes(&mut client, enabled_patch(true)).await?;
            println!("{}", render_chimes(&saved, fmt)?);
        }
        ChimesAction::Disable => {
            let saved = patch_chimes(&mut client, enabled_patch(false)).await?;
            println!("{}", render_chimes(&saved, fmt)?);
        }
        ChimesAction::Set { event, sound } => {
            let patch = event_sound_patch(event_arg(event), sound_arg(sound));
            let saved = patch_chimes(&mut client, patch).await?;
            println!("{}", render_chimes(&saved, fmt)?);
        }
        ChimesAction::Test { event } => {
            let event = event_arg(event);
            let response = client
                .request(Request::PreviewNotificationChime { event })
                .await?;
            let (event, sound, played) = crate::commands::expect_response(response, |r| match r {
                Response::Ok {
                    data:
                        ResponseData::NotificationChimePreview {
                            event,
                            sound,
                            played,
                        },
                } => Some((event, sound, played)),
                _ => None,
            })?;
            println!("{}", render_preview(event, sound, played, fmt)?);
        }
    }

    Ok(())
}

async fn fetch_chimes(client: &mut IpcClient) -> anyhow::Result<NotificationChimesData> {
    let response = client.request(Request::GetNotificationChimes).await?;
    crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::NotificationChimes { config },
        } => Some(config),
        _ => None,
    })
}

/// Only the changed field goes to the daemon, which applies it to the
/// setting as it is then, so another client's change is never undone.
async fn patch_chimes(
    client: &mut IpcClient,
    patch: NotificationChimesPatchData,
) -> anyhow::Result<NotificationChimesData> {
    let response = client
        .request(Request::PatchNotificationChimes {
            patch: Box::new(patch),
        })
        .await?;
    crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::NotificationChimes { config },
        } => Some(config),
        _ => None,
    })
}

fn render_chimes(config: &NotificationChimesData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(config)?,
        OutputFormat::Jsonl => serde_json::to_string(config)?,
        _ => [
            format!("enabled={}", config.enabled),
            format!("volume={}", config.volume),
            format!("new_mail={}", sound_label(config.new_mail)),
            format!("sent={}", sound_label(config.sent)),
            format!("archived={}", sound_label(config.archived)),
            format!("trashed={}", sound_label(config.trashed)),
            format!("spam={}", sound_label(config.spam)),
            format!("snoozed={}", sound_label(config.snoozed)),
            format!("unsnoozed={}", sound_label(config.unsnoozed)),
            format!("reminder={}", sound_label(config.reminder)),
            format!("error={}", sound_label(config.error)),
        ]
        .join("\n"),
    })
}

fn render_preview(
    event: NotificationChimeEventData,
    sound: NotificationChimeSoundData,
    played: bool,
    format: OutputFormat,
) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "event": event,
            "sound": sound,
            "played": played,
        }))?,
        OutputFormat::Jsonl => serde_json::to_string(&serde_json::json!({
            "event": event,
            "sound": sound,
            "played": played,
        }))?,
        _ => format!(
            "event={} sound={} played={}",
            event_label(event),
            sound_label(sound),
            played
        ),
    })
}

fn enabled_patch(enabled: bool) -> NotificationChimesPatchData {
    NotificationChimesPatchData {
        enabled: Some(enabled),
        ..Default::default()
    }
}

fn event_sound_patch(
    event: NotificationChimeEventData,
    sound: NotificationChimeSoundData,
) -> NotificationChimesPatchData {
    let mut patch = NotificationChimesPatchData::default();
    let slot = match event {
        NotificationChimeEventData::NewMail => &mut patch.new_mail,
        NotificationChimeEventData::Sent => &mut patch.sent,
        NotificationChimeEventData::Archived => &mut patch.archived,
        NotificationChimeEventData::Trashed => &mut patch.trashed,
        NotificationChimeEventData::Spam => &mut patch.spam,
        NotificationChimeEventData::Snoozed => &mut patch.snoozed,
        NotificationChimeEventData::Unsnoozed => &mut patch.unsnoozed,
        NotificationChimeEventData::Reminder => &mut patch.reminder,
        NotificationChimeEventData::Error => &mut patch.error,
    };
    *slot = Some(sound);
    patch
}

fn event_label(event: NotificationChimeEventData) -> &'static str {
    match event {
        NotificationChimeEventData::NewMail => "new_mail",
        NotificationChimeEventData::Sent => "sent",
        NotificationChimeEventData::Archived => "archived",
        NotificationChimeEventData::Trashed => "trashed",
        NotificationChimeEventData::Spam => "spam",
        NotificationChimeEventData::Snoozed => "snoozed",
        NotificationChimeEventData::Unsnoozed => "unsnoozed",
        NotificationChimeEventData::Reminder => "reminder",
        NotificationChimeEventData::Error => "error",
    }
}

fn sound_label(sound: NotificationChimeSoundData) -> &'static str {
    match sound {
        NotificationChimeSoundData::None => "none",
        NotificationChimeSoundData::Bell => "bell",
        NotificationChimeSoundData::Glass => "glass",
        NotificationChimeSoundData::Pop => "pop",
        NotificationChimeSoundData::Sent => "sent",
        NotificationChimeSoundData::Archive => "archive",
        NotificationChimeSoundData::Thud => "thud",
        NotificationChimeSoundData::Alert => "alert",
    }
}

fn event_arg(value: ChimeEventArg) -> NotificationChimeEventData {
    match value {
        ChimeEventArg::NewMail => NotificationChimeEventData::NewMail,
        ChimeEventArg::Sent => NotificationChimeEventData::Sent,
        ChimeEventArg::Archived => NotificationChimeEventData::Archived,
        ChimeEventArg::Trashed => NotificationChimeEventData::Trashed,
        ChimeEventArg::Spam => NotificationChimeEventData::Spam,
        ChimeEventArg::Snoozed => NotificationChimeEventData::Snoozed,
        ChimeEventArg::Unsnoozed => NotificationChimeEventData::Unsnoozed,
        ChimeEventArg::Reminder => NotificationChimeEventData::Reminder,
        ChimeEventArg::Error => NotificationChimeEventData::Error,
    }
}

fn sound_arg(value: ChimeSoundArg) -> NotificationChimeSoundData {
    match value {
        ChimeSoundArg::None => NotificationChimeSoundData::None,
        ChimeSoundArg::Bell => NotificationChimeSoundData::Bell,
        ChimeSoundArg::Glass => NotificationChimeSoundData::Glass,
        ChimeSoundArg::Pop => NotificationChimeSoundData::Pop,
        ChimeSoundArg::Sent => NotificationChimeSoundData::Sent,
        ChimeSoundArg::Archive => NotificationChimeSoundData::Archive,
        ChimeSoundArg::Thud => NotificationChimeSoundData::Thud,
        ChimeSoundArg::Alert => NotificationChimeSoundData::Alert,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputFormat;
    use mxr_protocol::{
        NotificationChimeEventData, NotificationChimeSoundData, NotificationChimesData,
    };

    #[test]
    fn render_chimes_json_exposes_enabled_volume_and_action_sounds() {
        let config = NotificationChimesData {
            enabled: true,
            volume: 0.5,
            ..NotificationChimesData::default()
        };

        let rendered = render_chimes(&config, OutputFormat::Json).expect("render json");

        assert!(rendered.contains("\"enabled\": true"));
        assert!(rendered.contains("\"volume\": 0.5"));
        assert!(rendered.contains("\"new_mail\": \"bell\""));
        assert!(rendered.contains("\"archived\": \"archive\""));
    }

    #[test]
    fn set_event_sound_patches_only_requested_action() {
        let patch = event_sound_patch(
            NotificationChimeEventData::Archived,
            NotificationChimeSoundData::Glass,
        );

        assert_eq!(
            patch,
            NotificationChimesPatchData {
                archived: Some(NotificationChimeSoundData::Glass),
                ..Default::default()
            }
        );
    }
}
