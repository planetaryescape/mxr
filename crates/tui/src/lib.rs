#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use panic and unwrap to keep fixture failures direct"
    )
)]

mod account_workflow;
mod accounts_helpers;
pub mod action;
pub mod app;
mod assistant_ipc;
mod async_result;
pub mod client;
mod compose_flow;
mod compose_persistence;
mod daemon_events;
mod editor;
pub mod input;
mod ipc;
pub mod keybindings;
mod local_io;
pub mod local_state;
mod runner;
mod runtime;
mod search_ipc;
mod send_recovery;
mod send_recovery_state;
pub mod terminal_images;
#[cfg(test)]
mod test_fixtures;
pub mod theme;
pub mod thread_context_rows;
pub mod ui;

pub(crate) use async_result::AsyncResult;
pub use runner::run;

#[cfg(test)]
mod selected_daemon_tests;
