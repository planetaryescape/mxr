# S08a Mac validation, job 4921

Candidate `e2b670979a9076dbcd6a971a52da38227492c515`, tree `c2c78899eb47d9d98f38dce1a0561a3b6cdf95ef`.

The scoped local gate completed in 9m30.267s: four focused Rust tests, Clippy and `cargo build -p mxr` passed. The isolated fake-profile stop/restart smoke ran separately and passed in 5.744s, retaining four fake message IDs. SIGTERM allowed only the bounded relationship analytics cancellation; idle SIGINT had no timeout or failure. The exact candidate binary ran on arm64, reported `mxr 0.6.63 (Mixer)`, and had SHA256 `6cf5ba17b6384d0972894e6efa865ec33fead50d036bbaef829e0bb4dc023119`.

The local gate exceeded its initial three-minute checkpoint. Process ancestry showed active compilation with rustc workers at 98.7% CPU; no competing worktree lock was observed. Apple `ld` printed a non-fatal `__eh_frame` compact-unwind warning. All commands exited successfully.
