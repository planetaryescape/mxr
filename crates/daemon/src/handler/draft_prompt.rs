//! The prompt builder is the composer's plan stage (`mxr_composer::plan`);
//! these names stay importable here for the handlers that predate the crate.

pub(crate) use mxr_composer::plan::{
    build, DraftMode, Help, Me, Prompt, PromptInput, Turn, VoiceExample,
};
