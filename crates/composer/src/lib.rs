//! The composer: how writing help is asked for, and how a proposal is checked
//! against what the user asked for (blueprint 34). The plan and check stages
//! are pure: plain data in, a prompt or a verdict out, with no store and no
//! model, so the same code runs in a snapshot test and against a live model.

#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic and unwrap for direct fixture failures"
    )
)]

pub mod check;
pub mod plan;
