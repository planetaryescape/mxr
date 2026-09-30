# The scoped pre-PR gate never clippies the `mxr` package for daemon changes

Status: logged, not fixed.

`scripts/pre-pr-rust-gate` maps `crates/<dir>/...` to the package in
`crates/<dir>/Cargo.toml`, but `crates/daemon` has no manifest of its own (it
builds as the root `mxr` package), so a change only under `crates/daemon/`
selects no package and clippy skips it. Found on `feat/deferral`, where the
gate passed while `cargo clippy -p mxr --all-targets` failed. Until the mapper
sends `crates/daemon/*` to `.`, run `cargo clippy -p mxr --all-targets
--all-features --locked -- -D warnings` by hand when the daemon changes.
