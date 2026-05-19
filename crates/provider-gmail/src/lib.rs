#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use panic and unwrap to keep fixture failures direct"
    )
)]

pub mod auth;
mod auth_storage;
pub mod client;
mod cursor;
pub mod error;
pub mod parse;
pub mod provider;
pub mod send;
pub mod types;

pub use error::GmailError;
pub use provider::GmailProvider;
