//! The plan stage: pick the action from what is typed, set the length, and
//! build the prompt. Pure, so the same code serves the daemon, the snapshot
//! test and `mxr draft eval --set quick`.

mod action;
mod prompt;

pub use action::{resolve, target_words, PlanError, Resolved, MAX_TEXT_BYTES};
pub use prompt::{
    build, system_prompt, DraftMode, Help, Me, Prompt, PromptInput, Turn, VoiceExample,
};
