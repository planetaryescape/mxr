# A debug `mxr demo` overflows the main thread's stack on macOS

Status: open. Found 2026-10-29 while merging the daemon's integration tests into one test program.

## What happens

`demo_cli::demo_seeds_a_usable_mailbox_and_stops_cleanly` fails on macOS debug builds. `mxr demo --no-tui --messages 2000` seeds all 2,000 messages, prints "Starting sync", then aborts with `thread 'main' has overflowed its stack`. The test sees `exit None`. CI runs the same test on Linux with nextest and passes (CI on 2fc4b7a8c).

## Evidence

- On this Mac, the test fails on main 2fc4b7a8c with the merged test program, both alone and in the full run.
- It fails the same way in the old one-program-per-file layout: the `demo_cli` test program built at 4fbaf241b failed after 115 s with the same stack overflow. The merge did not cause it.

## Likely cause, unverified

`crates/daemon/src/main.rs` gives tokio's worker threads 8 MiB of stack because a debug `handler::dispatch` frame measured 1.5 MB. `block_on` runs `mxr::run_cli` on the main thread, which keeps the OS default stack. The demo sync path may build a frame on the main thread that is too large in a debug build. Settle it by running the demo under `lldb` and reading the overflowing frame, or by running `run_cli` on a spawned thread with an explicit stack size and checking the test passes.
