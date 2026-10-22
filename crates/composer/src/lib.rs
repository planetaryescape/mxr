//! The composer: how a draft is asked for, and how a proposal is checked
//! against what the user asked for (blueprint 34). The check stage is pure:
//! plain text in, grades out, with no store and no model, so the same
//! graders run in a snapshot test and against a live model.

pub mod check;
