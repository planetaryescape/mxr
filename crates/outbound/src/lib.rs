#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "unit tests unwrap rendered email fixtures for direct failures"
    )
)]

pub mod attachments;
pub mod email;
pub mod render;
