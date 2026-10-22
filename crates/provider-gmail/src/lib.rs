#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use panic and unwrap to keep fixture failures direct"
    )
)]

pub mod auth;
mod auth_refresh;
mod auth_storage;
pub mod client;
mod cursor;
pub mod error;
mod error_response;
pub mod parse;
pub mod provider;
mod request_log;
pub mod send;
mod token_cache_file;
pub mod types;

pub use error::GmailError;
pub use provider::GmailProvider;
