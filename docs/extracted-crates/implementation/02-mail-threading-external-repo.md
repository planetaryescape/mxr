# Move `mail-threading` to its own repo

Status: **complete** (2026-05-16)
target repo: [`planetaryescape/mail-threading`](https://github.com/planetaryescape/mail-threading)
source crate: `crates/mail-threading` (deleted from `mxr` after cutover)
published crate: [`mail-threading`](https://crates.io/crates/mail-threading)

## Goal

Move `mail-threading` out of the `mxr` workspace into
`https://github.com/planetaryescape/mail-threading`, publish it to crates.io,
then switch `mxr` from a path dependency to the published crate.

The migration is complete only when:

- `planetaryescape/mail-threading` is the source of truth for the crate.
- `mail-threading` is published on crates.io.
- docs.rs builds the crate docs.
- `mxr-sync` depends on `mail-threading = "0.1.0"` from crates.io.
- `crates/mail-threading/` is removed from `mxr`.
- `mxr-sync` tests pass against the registry dependency.

References:

- Cargo publishing: <https://doc.rust-lang.org/cargo/reference/publishing.html>
- `cargo publish`: <https://doc.rust-lang.org/cargo/commands/cargo-publish.html>
- Cargo manifest metadata: <https://doc.rust-lang.org/cargo/reference/manifest.html>
- Cargo MSRV field: <https://doc.rust-lang.org/cargo/reference/rust-version.html>

## Why this needs a staged migration

The crate currently lives inside the `mxr` workspace and inherits workspace
metadata:

```toml
edition.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true
homepage.workspace = true
```

It also inherits workspace dependencies:

```toml
chrono = { workspace = true }
serde = { workspace = true, optional = true }
serde_json = { workspace = true }
```

That works inside `mxr`, but it is not a standalone crate manifest. Before the
crate can be published from its own repo, those fields must become explicit.

Publishing is also permanent. crates.io does not let us overwrite a published
version. If `0.1.0` is published with the wrong metadata, missing files, or a
bad README, the fix is `0.1.1`, not replacing `0.1.0`. So the order matters:
extract, validate, dry-run, publish, then cut `mxr` over.

## Phase 0: Finish the in-tree source

Do this in `mxr` first.

```bash
git status --short crates/mail-threading crates/sync Cargo.toml Cargo.lock clippy.toml
cargo fmt -p mail-threading -p mxr-sync -- --check
scripts/cargo-test -p mail-threading --all-features --tests
cargo test -p mail-threading --all-features --doc
scripts/cargo-test -p mxr-sync --tests
cargo clippy -p mail-threading --all-targets --all-features --locked -- -D warnings
cargo clippy -p mxr-sync --all-targets --no-deps --locked -- -D warnings
```

The in-tree crate is ready to extract when:

- README explains what the crate does, what it does not do, and how it maps to
  RFC 5256/JWZ.
- `testdata/conformance/*.json` is complete enough for `0.1.0`.
- `testdata/rfc5256-coverage.md` mentions every fixture.
- `tests/conformance.rs` enforces required fixtures and coverage matrix drift.
- `cargo publish --dry-run -p mail-threading --allow-dirty` passes from `mxr`.

If there are uncommitted changes, commit them before splitting. A clean source
commit makes the extracted repo auditable.

Suggested `mxr` commit:

```text
feat: prepare mail-threading for external publish
```

## Phase 1: Create the GitHub repo with history

Use `git subtree split` so the new repo keeps the history of
`crates/mail-threading` without carrying the whole `mxr` repository.

From the `mxr` repo:

```bash
git subtree split --prefix=crates/mail-threading -b split/mail-threading
gh repo create planetaryescape/mail-threading --public --source=. --remote=mail-threading --push=false
git push git@github.com:planetaryescape/mail-threading.git split/mail-threading:main
```

If `gh repo create --source` is awkward because the source is the `mxr` repo,
create the repo without `--source`:

```bash
gh repo create planetaryescape/mail-threading --public
git push git@github.com:planetaryescape/mail-threading.git split/mail-threading:main
```

Acceptance checks:

```bash
git ls-remote git@github.com:planetaryescape/mail-threading.git main
```

Expected repo contents at this point:

```text
Cargo.toml
README.md
src/
testdata/
tests/
```

## Phase 2: Make the new repo standalone

Clone the new repo:

```bash
git clone git@github.com:planetaryescape/mail-threading.git
cd mail-threading
```

Update `Cargo.toml` so it no longer depends on workspace inheritance.

Target manifest:

```toml
[package]
name = "mail-threading"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
rust-version = "1.88"
repository = "https://github.com/planetaryescape/mail-threading"
homepage = "https://github.com/planetaryescape/mail-threading"
documentation = "https://docs.rs/mail-threading"
description = "Spec-aligned client-side email threading using RFC 5256/JWZ references."
readme = "README.md"
keywords = ["email", "threading", "jwz", "imap", "rfc5256"]
categories = ["algorithms", "email"]
include = [
    "Cargo.toml",
    "README.md",
    "src/**",
    "testdata/**",
    "tests/**",
    "LICENSE-MIT",
    "LICENSE-APACHE",
]

[lints.rust]
unsafe_code = "deny"
unused_must_use = "deny"

[lints.clippy]
unwrap_used = "warn"
panic = "warn"
todo = "warn"

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]

[features]
default = []
serde = ["dep:serde"]

[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"], optional = true }

[dev-dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Copy licensing and lint config from `mxr`:

```bash
cp ../mxr/LICENSE-MIT .
cp ../mxr/LICENSE-APACHE .
cat > clippy.toml <<'EOF'
msrv = "1.88"
EOF
```

Add a repository README badge only if it is useful and true after CI exists.
Do not add badges that point nowhere.

## Phase 3: Add CI to the new repo

Add `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt -- --check
      - run: cargo test --all-features
      - run: cargo test --all-features --doc
      - run: cargo clippy --all-targets --all-features --locked -- -D warnings
      - run: cargo publish --dry-run
```

Run the same checks locally before pushing:

```bash
cargo fmt -- --check
cargo test --all-features
cargo test --all-features --doc
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo publish --dry-run
cargo package --list
```

Acceptance checks:

- CI passes on `main`.
- `cargo package --list` includes README, source, tests, testdata, and license
  files.
- `cargo package --list` does not include unrelated build artifacts.
- `cargo publish --dry-run` succeeds without `--allow-dirty`.

## Phase 4: Publish `mail-threading` to crates.io

Authenticate locally if needed:

```bash
cargo login
```

Publish from the standalone `mail-threading` repo:

```bash
cargo publish
```

Then verify the upload:

```bash
cargo search mail-threading
cargo info mail-threading
```

Also check these pages manually:

- <https://crates.io/crates/mail-threading>
- <https://docs.rs/mail-threading>
- <https://github.com/planetaryescape/mail-threading>

If the upload succeeds but the local Cargo index has not caught up yet, wait
and retry:

```bash
cargo update -p mail-threading
```

Do not republish the same version. If a real problem is discovered after
publishing, fix it and publish `0.1.1`.

## Phase 5: Tag the standalone repo

After crates.io publish succeeds:

```bash
git tag v0.1.0
git push origin v0.1.0
```

If we want package-specific tags for future multi-package conventions, use:

```bash
git tag mail-threading-v0.1.0
git push origin mail-threading-v0.1.0
```

Use one tag convention and keep it consistent. For a standalone repo,
`v0.1.0` is simpler.

## Phase 6: Cut `mxr` over to crates.io

Back in the `mxr` repo, change the workspace dependency in root `Cargo.toml`.

Before:

```toml
mail-threading = { path = "crates/mail-threading", version = "0.1.0" }
```

After:

```toml
mail-threading = "0.1.0"
```

Remove `crates/mail-threading` from the workspace members:

```toml
"crates/mail-threading",
```

Delete the in-tree crate:

```bash
rm -rf crates/mail-threading
cargo update -p mail-threading
```

`crates/sync/Cargo.toml` can stay as:

```toml
mail-threading = { workspace = true }
```

That keeps dependency ownership in the root workspace manifest.

Run the focused checks:

```bash
cargo fmt -p mxr-sync -- --check
scripts/cargo-test -p mxr-sync --tests
cargo clippy -p mxr-sync --all-targets --no-deps --locked -- -D warnings
```

Then run a broader dependency sanity check:

```bash
cargo tree -p mxr-sync -i mail-threading
rg -n "crates/mail-threading|mail-threading = \\{ path" Cargo.toml Cargo.lock crates docs
```

Acceptance checks:

- `Cargo.lock` resolves `mail-threading` from the registry.
- No workspace member points at `crates/mail-threading`.
- `mxr-sync` tests pass.
- No code changes are needed in `crates/sync/src/engine.rs`; the Rust import
  remains `mail_threading`.

Commit the cutover:

```text
chore: consume mail-threading from crates.io
```

## Phase 7: Clean up docs in `mxr`

After the cutover, update docs that still describe `mail-threading` as an
in-tree crate.

Likely files:

```bash
rg -n "crates/mail-threading|in-tree|workspace crate|mail-threading" docs README.md AGENTS.md Cargo.toml
```

Expected updates:

- `docs/extractable-crates/02-jwz-threading.md` should say the crate now lives
  at `planetaryescape/mail-threading`.
- `docs/extracted-crates/implementation/01-jwz-threading.md` should mark the
  in-repo implementation phase complete and point to the standalone repo.
- `docs/extractable-crates/README.md` should call `mail-threading` published,
  not merely publishable.
- Any mention of shared JSON corpus should point to the new repo as source of
  truth.

Do not delete the planning docs immediately. They are useful historical
context. Add a short status note at the top instead.

## Phase 8: Maintenance model after extraction

Once `mail-threading` is external:

- Bug fixes land in `planetaryescape/mail-threading` first.
- New conformance fixtures land in `planetaryescape/mail-threading` first.
- `mxr` consumes releases through normal Cargo version bumps.
- `mxr` should not vendor or copy `mail-threading` testdata.
- The future JS package should pull its conformance corpus from the standalone
  repo, not from `mxr`.

Release flow for a crate change:

```bash
cd mail-threading
cargo fmt -- --check
cargo test --all-features
cargo test --all-features --doc
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo publish --dry-run
cargo publish
git tag v0.1.1
git push origin main v0.1.1

cd ../mxr
cargo update -p mail-threading
scripts/cargo-test -p mxr-sync --tests
```

## Rollback plan

If the standalone repo is created but not published:

```bash
git branch -D split/mail-threading
```

Leave `mxr` unchanged.

If `mail-threading` is published but `mxr` cutover fails:

- Keep the published crate.
- Fix the standalone crate and publish a patch version.
- Cut `mxr` over to the fixed version.

Do not try to overwrite the existing crates.io version.

If `mxr` has already removed the in-tree crate and needs an emergency revert:

```bash
git revert <cutover-commit>
```

That restores the path dependency and `crates/mail-threading` as long as the
cutover was a clean commit.

## Final checklist

Standalone repo:

- [x] `planetaryescape/mail-threading` exists.
- [x] `Cargo.toml` is standalone.
- [x] Licenses are present.
- [x] CI exists and passes.
- [x] `cargo publish --dry-run` passes.
- [x] `mail-threading` is published to crates.io.
- [x] docs.rs builds.
- [x] `v0.1.0` tag exists.

`mxr` repo:

- [x] Root `Cargo.toml` uses `mail-threading = "0.1.0"`.
- [x] `crates/mail-threading` is removed from workspace members.
- [x] `crates/mail-threading/` is deleted.
- [x] `Cargo.lock` resolves the registry crate.
- [x] `mxr-sync` tests pass.
- [x] `mxr-sync` clippy pass succeeds.
- [x] Docs point to `planetaryescape/mail-threading` as source of truth.
