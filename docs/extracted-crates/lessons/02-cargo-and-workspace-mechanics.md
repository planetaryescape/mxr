# Cargo and workspace mechanics

What `mail-threading` taught about manifests, lockfiles, dependency resolution, and the workspace ↔ registry boundary.

## Workspace inheritance is invisible until publish

Looked fine in `mxr`:

```toml
edition.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true
homepage.workspace = true

[lints]
workspace = true

[dependencies]
chrono = { workspace = true }
serde = { workspace = true, optional = true }
```

Had to all become explicit in the standalone. Concrete target shape lives in [`06-reusable-artifacts.md`](./06-reusable-artifacts.md#standalone-cargotoml-template).

Rule: the standalone manifest is the real package contract. The in-tree one is a comfortable lie.

## `cargo update -p` fails before re-resolve

After flipping `mail-threading = { path = "...", version = "0.1.0" }` to `"0.1.0"`:

```bash
$ cargo update -p mail-threading
error: package ID specification `mail-threading` did not match any packages
```

The lockfile still has the old in-tree entry. `cargo update -p` won't bootstrap a new resolution.

Fix: any build command re-resolves and picks up the registry version.

```bash
cargo check -p <consumer-crate>
```

`Updating crates.io index / Locking 1 package to latest compatible version / Adding mail-threading v0.1.0`. Done.

Update the plan template: replace `cargo update -p <name>` in Phase 6 with `cargo check -p <consumer>`.

## `Cargo.lock` collateral changes are unavoidable on dirty branches

The cutover lockfile diff for `mail-threading` was three hunks:

1. `mail-threading` source changed from in-tree to registry (ours)
2. `mxr-outbound` chrono dev-dep added (unrelated in-progress work in the dirty branch)

Staging the whole file leaked unrelated work into our cutover commit. Fix:

```bash
git restore --staged Cargo.lock
git add -p Cargo.lock     # y on mail-threading hunk, n on the rest
```

On a clean branch (preferred — see preflight) the collateral disappears and this step is unnecessary.

## `Cargo.lock` for libraries: commit it

`mail-threading` ships `Cargo.lock` in the repo. Reasons:

- CI runs `cargo clippy --locked` — requires committed lockfile
- the publish workflow runs `cargo publish --dry-run` which needs lock
- the old "libs gitignore Cargo.lock" advice predates `--locked` CI

`cargo package --list` does include `Cargo.lock` in the published tarball (since Cargo 1.84). docs.rs uses it.

## docs.rs uses default features

Without explicit config, docs.rs only documents the default-feature surface. `mail-threading` has an optional `serde` feature; we want it documented:

```toml
[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
```

Copy verbatim into every extracted crate's manifest.

## `include` discipline

```toml
include = [
    "Cargo.toml",
    "README.md",
    "src/**",
    "testdata/**",
    "tests/**",
    "LICENSE-MIT",
    "LICENSE-APACHE",
]
```

Cargo defaults to a denylist (`target/`, `.git/`, etc.) but for testdata-heavy crates an explicit allowlist is safer. `mail-threading` shipped 49 files at 112.5 KiB — verify with `cargo package --list`.

Do **not** include `clippy.toml`, `.gitignore`, or `.github/`. Those are repo-level, not package-level.

## Categories slugs are checked at publish

Invalid slugs reject the upload. Validated for `mail-threading`:

```toml
categories = ["algorithms", "email"]
```

Both valid per https://crates.io/category_slugs. Catch invalid ones with the preflight dry-run, not on the live publish.

## Workspace dep entry is the cutover trigger

The cutover boils down to one line in the workspace root `Cargo.toml`:

```diff
- mail-threading = { path = "crates/mail-threading", version = "0.1.0" }
+ mail-threading = "0.1.0"
```

Plus removing the member from `workspace.members` and `rm -rf crates/<name>`. Consumers using `mail-threading = { workspace = true }` need zero changes.

Time-budget: 2 minutes mechanical work once the crate is live on crates.io.

## Lint blocks: re-declare, don't drop

Workspace had:

```toml
[workspace.lints.rust]
unsafe_code = "deny"
unused_must_use = "deny"

[workspace.lints.clippy]
unwrap_used = "warn"
panic = "warn"
todo = "warn"
```

Standalone has the same as `[lints.rust]` and `[lints.clippy]` blocks (not workspace-scoped). Don't drop them — they're part of the quality contract.

## MSRV pinned in two places

```toml
# Cargo.toml
[package]
rust-version = "1.88"
```

```toml
# clippy.toml
msrv = "1.88"
```

Both required. Cargo's `rust-version` blocks `cargo install` on older toolchains. `clippy.toml`'s `msrv` makes clippy aware so it doesn't suggest features introduced after MSRV.
