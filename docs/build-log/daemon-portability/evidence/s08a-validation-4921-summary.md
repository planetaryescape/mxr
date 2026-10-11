# S08a Mac validation, job 4921

Candidate `e2b670979a9076dbcd6a971a52da38227492c515`, tree `c2c78899eb47d9d98f38dce1a0561a3b6cdf95ef`.

The scoped local gate completed in 9m30.267s. Four focused Rust tests, Clippy and `cargo build -p mxr` passed. The isolated fake-profile stop/restart smoke passed in 5.744s and retained four fake message IDs. SIGTERM allowed only the bounded relationship analytics cancellation; idle SIGINT had no timeout or failure. The exact candidate binary ran on arm64, reported `mxr 0.6.63 (Mixer)`, and had SHA256 `6cf5ba17b6384d0972894e6efa865ec33fead50d036bbaef829e0bb4dc023119`.

The clean `target-cli` cache pushed compilation, Clippy and final code generation beyond the three-minute checkpoint. Rustc workers reached 98.7% CPU; no nested task or competing worktree lock was found. Apple `ld` printed a non-fatal `__eh_frame` compact-unwind warning. All commands exited successfully.
