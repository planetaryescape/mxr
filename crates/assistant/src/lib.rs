#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use panic and unwrap to keep fixture failures direct"
    )
)]
//! The in-app assistant: a chat agent that works on the user's mailbox
//! through mxr's own tools.
//!
//! This crate holds the parts that decide behaviour: the agent loop
//! ([`turn`]), the approval gate ([`gate`]), structured questions
//! ([`ask_user`]) and what the model reads ([`prompt`]). It knows nothing
//! about the daemon, storage or MCP; the daemon supplies a [`ToolHost`],
//! a [`ChatModel`] and a [`TurnSink`].

pub mod ask_user;
pub mod gate;
pub mod outcome;
pub mod prompt;
pub mod turn;

pub use ask_user::{Answer, Question, QuestionOption, ASK_USER_TOOL};
pub use outcome::{call_state, readable_result, CallState};
pub use prompt::ViewContext;
pub use turn::{
    cancelled_result, check_decisions, model_input, Agent, ChatModel, Decision, Limits,
    MessageMeta, PendingItem, PendingKind, Progress, ToolHost, ToolOutput, TurnOutcome, TurnSink,
};

#[cfg(test)]
mod turn_tests;
