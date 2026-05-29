use crate::state::AppState;
use mxr_protocol::{
    NotificationChimeEventData, NotificationChimeSoundData, NotificationChimesData, ResponseData,
};

use super::HandlerResult;

pub(super) async fn get_notification_chimes(state: &AppState) -> HandlerResult {
    Ok(ResponseData::NotificationChimes {
        config: chimes_data(state.config_snapshot().notifications.chimes),
    })
}

pub(super) async fn update_notification_chimes(
    state: &AppState,
    config: NotificationChimesData,
) -> HandlerResult {
    let config = chimes_config(config)?;
    let saved = state
        .mutate_config(|current| {
            current.notifications.chimes = config;
        })
        .await?;
    Ok(ResponseData::NotificationChimes {
        config: chimes_data(saved.notifications.chimes),
    })
}

pub(super) async fn preview_notification_chime(
    state: &AppState,
    event: NotificationChimeEventData,
) -> HandlerResult {
    let event = chime_event(event);
    let playback = crate::chimes::preview(&state.config_snapshot().notifications.chimes, event);
    Ok(ResponseData::NotificationChimePreview {
        event: event_data(event),
        sound: sound_data(playback.sound),
        played: playback.played,
    })
}

fn chimes_data(config: mxr_config::ChimeConfig) -> NotificationChimesData {
    NotificationChimesData {
        enabled: config.enabled,
        volume: config.volume,
        new_mail: sound_data(config.new_mail),
        sent: sound_data(config.sent),
        archived: sound_data(config.archived),
        trashed: sound_data(config.trashed),
        spam: sound_data(config.spam),
        snoozed: sound_data(config.snoozed),
        unsnoozed: sound_data(config.unsnoozed),
        reminder: sound_data(config.reminder),
        error: sound_data(config.error),
    }
}

fn chimes_config(config: NotificationChimesData) -> Result<mxr_config::ChimeConfig, String> {
    if !config.volume.is_finite() || !(0.0..=1.0).contains(&config.volume) {
        return Err("notifications.chimes.volume must be between 0.0 and 1.0".to_string());
    }
    Ok(mxr_config::ChimeConfig {
        enabled: config.enabled,
        volume: config.volume,
        new_mail: chime_sound(config.new_mail),
        sent: chime_sound(config.sent),
        archived: chime_sound(config.archived),
        trashed: chime_sound(config.trashed),
        spam: chime_sound(config.spam),
        snoozed: chime_sound(config.snoozed),
        unsnoozed: chime_sound(config.unsnoozed),
        reminder: chime_sound(config.reminder),
        error: chime_sound(config.error),
    })
}

fn chime_event(event: NotificationChimeEventData) -> mxr_config::ChimeEvent {
    match event {
        NotificationChimeEventData::NewMail => mxr_config::ChimeEvent::NewMail,
        NotificationChimeEventData::Sent => mxr_config::ChimeEvent::Sent,
        NotificationChimeEventData::Archived => mxr_config::ChimeEvent::Archived,
        NotificationChimeEventData::Trashed => mxr_config::ChimeEvent::Trashed,
        NotificationChimeEventData::Spam => mxr_config::ChimeEvent::Spam,
        NotificationChimeEventData::Snoozed => mxr_config::ChimeEvent::Snoozed,
        NotificationChimeEventData::Unsnoozed => mxr_config::ChimeEvent::Unsnoozed,
        NotificationChimeEventData::Reminder => mxr_config::ChimeEvent::Reminder,
        NotificationChimeEventData::Error => mxr_config::ChimeEvent::Error,
    }
}

fn event_data(event: mxr_config::ChimeEvent) -> NotificationChimeEventData {
    match event {
        mxr_config::ChimeEvent::NewMail => NotificationChimeEventData::NewMail,
        mxr_config::ChimeEvent::Sent => NotificationChimeEventData::Sent,
        mxr_config::ChimeEvent::Archived => NotificationChimeEventData::Archived,
        mxr_config::ChimeEvent::Trashed => NotificationChimeEventData::Trashed,
        mxr_config::ChimeEvent::Spam => NotificationChimeEventData::Spam,
        mxr_config::ChimeEvent::Snoozed => NotificationChimeEventData::Snoozed,
        mxr_config::ChimeEvent::Unsnoozed => NotificationChimeEventData::Unsnoozed,
        mxr_config::ChimeEvent::Reminder => NotificationChimeEventData::Reminder,
        mxr_config::ChimeEvent::Error => NotificationChimeEventData::Error,
    }
}

fn chime_sound(sound: NotificationChimeSoundData) -> mxr_config::ChimeSound {
    match sound {
        NotificationChimeSoundData::None => mxr_config::ChimeSound::None,
        NotificationChimeSoundData::Bell => mxr_config::ChimeSound::Bell,
        NotificationChimeSoundData::Glass => mxr_config::ChimeSound::Glass,
        NotificationChimeSoundData::Pop => mxr_config::ChimeSound::Pop,
        NotificationChimeSoundData::Sent => mxr_config::ChimeSound::Sent,
        NotificationChimeSoundData::Archive => mxr_config::ChimeSound::Archive,
        NotificationChimeSoundData::Thud => mxr_config::ChimeSound::Thud,
        NotificationChimeSoundData::Alert => mxr_config::ChimeSound::Alert,
    }
}

fn sound_data(sound: mxr_config::ChimeSound) -> NotificationChimeSoundData {
    match sound {
        mxr_config::ChimeSound::None => NotificationChimeSoundData::None,
        mxr_config::ChimeSound::Bell => NotificationChimeSoundData::Bell,
        mxr_config::ChimeSound::Glass => NotificationChimeSoundData::Glass,
        mxr_config::ChimeSound::Pop => NotificationChimeSoundData::Pop,
        mxr_config::ChimeSound::Sent => NotificationChimeSoundData::Sent,
        mxr_config::ChimeSound::Archive => NotificationChimeSoundData::Archive,
        mxr_config::ChimeSound::Thud => NotificationChimeSoundData::Thud,
        mxr_config::ChimeSound::Alert => NotificationChimeSoundData::Alert,
    }
}
