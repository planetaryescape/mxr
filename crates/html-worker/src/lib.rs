//! Untrusted mail HTML is parsed in a helper process, not in the daemon, the
//! TUI or the CLI.
//!
//! html5ever and html2text have super-linear paths that no cheap pre-scan
//! bounds, and an allocation failure aborts the process where `catch_unwind`
//! cannot help. Input limits (the reader's HTML budget) are the first line.
//! This crate is the second: every parse of mail HTML is a [`Job`] that runs
//! in a child process with a memory ceiling, a CPU backstop and a wall-clock
//! timeout. When the child is lost the caller gets `None` and uses the
//! fallback it already has for a document over budget; the next request
//! starts a fresh child.
//!
//! * Library crates define their parses as `static` [`Job`]s and call them
//!   with [`Job::call`]. With no helper installed the job runs in-process, so
//!   unit tests and library users see no difference.
//! * The `mxr` binary calls [`install`] at start-up for the daemon, TUI and
//!   CLI, and [`run_helper`] when started in helper mode with every crate's
//!   jobs registered.
//! * [`stats`] feeds `mxr doctor` and the diagnostics page.

mod job;
mod limits;
mod supervisor;
mod wire;
mod worker;

pub use job::{AnyJob, Job, DEFAULT_MAX_HTML_BYTES};
pub use supervisor::{
    install, stats, with_supervisor, Config, FallbackReason, Outcome, Stats, Supervisor,
};
pub use worker::run_helper;
