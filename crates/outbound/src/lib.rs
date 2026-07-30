#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "unit tests unwrap rendered email fixtures and panic from assertion \
                  helpers so a missing MIME part fails loudly instead of silently \
                  reading as an empty one"
    )
)]

pub mod attachments;
pub mod email;
pub mod html;
pub mod render;
