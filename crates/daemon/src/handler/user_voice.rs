use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_protocol::{
    ResponseData, UserVoiceProfileData, UserVoiceRegisterModeData, VoiceRegisterData,
};

pub(super) async fn get_user_voice(state: &AppState, account_id: &AccountId) -> HandlerResult {
    let profile = state
        .store
        .get_user_voice_profile(account_id)
        .await
        .map_err(|error| error.to_string())?
        .map(|profile| UserVoiceProfileData {
            account_id: profile.account_id,
            formality_score: profile.formality_score,
            avg_sentence_len: profile.avg_sentence_len,
            msg_count_used: profile.msg_count_used,
            register_modes: profile
                .register_modes
                .into_iter()
                .map(|mode| UserVoiceRegisterModeData {
                    register: match mode.name.as_str() {
                        "casual" => VoiceRegisterData::Casual,
                        "formal" => VoiceRegisterData::Formal,
                        _ => VoiceRegisterData::Neutral,
                    },
                    formality_score: mode.formality_score,
                    avg_sentence_len: mode.avg_sentence_len,
                    exemplar_message_ids: mode.exemplar_message_ids,
                })
                .collect(),
            computed_at: profile.computed_at,
            source_hash: profile.source_hash,
        });
    Ok(ResponseData::UserVoice { profile })
}

pub(super) async fn rebuild_user_voice(state: &AppState, account_id: &AccountId) -> HandlerResult {
    mxr_relationship::service::rebuild_user_voice_profile(&state.store, account_id)
        .await
        .map_err(|error| error.to_string())?;
    get_user_voice(state, account_id).await
}
