# Crate Boundary Audit

## Current state

Before this cleanup, mxr had logical crate seams on disk but not honest Cargo seams:

- the repo-root package `mxr` was the only real product package
- most code under `crates/` was not a real workspace crate yet
- `crates/daemon/src/lib.rs` pulled many lower layers in with `#[path = "../../.../src/lib.rs"]`
- internal code routinely referenced pseudo-crates like `crate::mxr_core`
- `mxr-search` still contained store-backed saved-search glue
- shared mail parsing lived under `mxr-compose`, which forced provider parsing to reach sideways into compose internals

## Problems / risk

- Cargo did not enforce the documented seams, so accidental coupling was easy
- ownership was muddy: code looked like separate crates but compiled like one bucket
- daemon `#[path]` inclusion hid the real dependency graph
- internal crates could not declare or verify their own dependencies
- packaging and publication intent were unclear because “logical crates” were not real workspace packages

## Target shape

- repo-root package `mxr` stays the product/install surface
- internal crates under `crates/` are real workspace crates with normal path dependencies
- internal crates default to `publish = false`
- daemon is the integration root and depends on lower crates normally
- `mxr-mail-parse` owns shared RFC 5322 / mail parsing helpers
- `mxr-outbound` owns shared markdown-to-message rendering/building helpers used by compose and send adapters
- `mxr-store` and `mxr-search` remain separate; `mxr-search` no longer owns store-backed saved-search service glue
- clients (`mxr-tui`, `mxr-web`) stay off daemon/store/search/sync/provider crates, while still being allowed to use client-local utility crates such as `config`, `compose`, `reader`, and `mail-parse`
- the IMAP adapter uses the published `mxr-async-imap` fork as a normal registry dependency, so vendored source no longer distorts workspace membership

## Dependency-direction rules

1. `mxr-core` is the leaf.
2. `mxr-protocol` depends only on `mxr-core`.
3. `mxr-store` depends only on `mxr-core`.
4. `mxr-search` depends only on `mxr-core`.
5. `mxr-sync` depends on `mxr-core`, `mxr-store`, `mxr-search`.
6. `mxr-semantic` depends only on lower utility/runtime crates it truly needs.
7. Provider crates depend on `mxr-core` plus shared mail utility crates only (`mxr-mail-parse`, `mxr-outbound`).
8. `mxr` is the integration root.
9. Use Cargo dependencies for seams; do not use `#[path]` pseudo-crates.

## Migration summary

- added real `Cargo.toml` manifests for the internal crates under `crates/`
- added `crates/mail-parse` for shared mail parsing
- added `crates/outbound` for shared outbound message building
- removed daemon-side `#[path]` inclusion of pseudo-crates
- rewrote internal imports from pseudo-root paths to real crate paths
- removed `mxr-search` re-export of the unused `SavedSearchService`
- switched IMAP to the published `mxr-async-imap` fork instead of a vendored path dependency
- kept the root package name/binary surface as `mxr` while marking it `publish = false`

## Remaining non-goals

- this cleanup does not move reader rendering behind daemon IPC
- this cleanup does not move the user-facing compose workflow out of `mxr-compose`
- this cleanup does not make crates.io publication part of the supported release/install story for `mxr`
