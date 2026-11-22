# Request futures are boxed by hand to keep debug builds off a stack overflow

Status: open. Found: delete-comment sweep of `crates/daemon/src/handler`.

`dispatch` in `mod.rs` holds every request arm's state inline. Past about a megabyte it overflows a Tokio worker's 2 MiB stack in debug builds (the daemon aborted on its first request on Linux CI). The handlers work around it by boxing futures one by one: the Updates and D119 mode requests and Reading in `mod.rs`, the done steps in `mode_done.rs`, the digest in `updates.rs`, the arrivals variants. Each new deep handler can bring the overflow back, and only a debug build on a small stack shows it. A guard test in `tests.rs` (the dispatch future size) catches some of it.

Smallest fix: box the future once at the `dispatch` call boundary for every request class, so no arm adds to the poll frame, then delete the per-handler boxing.
