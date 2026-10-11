# The scoped pre-PR gate never clippies the `mxr` package for daemon changes

Status: logged, not fixed.

`scripts/pre-pr-rust-gate` maps `crates/<dir>/...` to the package in
`crates/<dir>/Cargo.toml`, but `crates/daemon` has no manifest of its own (it
builds as the root `mxr` package), so a change only under `crates/daemon/`
selects no package and clippy skips it. Found on `feat/deferral`, where the
gate passed while `cargo clippy -p mxr --all-targets` failed. Until the mapper
sends `crates/daemon/*` to `.`, run `cargo clippy -p mxr --all-targets
--all-features --locked -- -D warnings` by hand when the daemon changes.

At the annual-renewal task checkpoint `899277891de3224d4db55ede402e97f89d59fab6`,
`scripts/pre-pr-rust-gate --base refs/remotes/renewal-tasks/main --list`
selected only `mxr-mcp`, `mxr-records` and `mxr-store`, despite changes under
`crates/daemon`. The freshly fetched base was
`f1e19fba2055fa96bbaf73ce35a6bd8354cd4870`. The omission remains reproducible;
required workspace Clippy in CI covers the daemon for this change.
