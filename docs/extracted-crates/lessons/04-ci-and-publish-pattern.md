# CI and publish pattern

The publish flow used for `mail-threading`. Token in GitHub Secrets, tag push triggers publish, no crates.io token on the developer machine.

This was a user preference, captured durably so future extractions default to it.

## Why CI publish over local `cargo login`

- No crates.io token persisted on the developer laptop
- Audit trail in GitHub Actions (who triggered the publish, when, which SHA)
- `git tag` *is* the publish trigger — collapses migration plan Phase 4 (publish) and Phase 5 (tag) into one action
- Same workflow scales to every future extracted crate without per-machine setup
- A junior contributor with PR rights but not crates.io rights can still propose a release

Trade-off: requires one-time secret setup per repo. Cheap relative to lifetime publishes.

## The two workflows

`mail-threading` ships two workflow files, both at `.github/workflows/`:

### `ci.yml` — run on every push and PR

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

The `cargo publish --dry-run` step is part of CI, not just publish. It catches manifest drift on every commit — not just on tag.

### `publish.yml` — run on tag push or manual dispatch

```yaml
name: Publish

on:
  push:
    tags:
      - "v*.*.*"
  workflow_dispatch:

jobs:
  publish:
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
      - name: Publish to crates.io
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo publish
```

`workflow_dispatch` is the emergency button when a tag was skipped or the workflow needs to be retried without a fresh tag.

## Secret-injection safety

GitHub Actions inline expressions in `run:` blocks are a known injection vector. Always pass secrets through `env:`:

```yaml
# unsafe — interpolated into the shell command
- run: cargo publish --token ${{ secrets.CARGO_REGISTRY_TOKEN }}

# safe — env var read by cargo natively
- name: Publish to crates.io
  env:
    CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
  run: cargo publish
```

Cargo reads `CARGO_REGISTRY_TOKEN` from env without needing `--token`. There's no reason to inline.

## Secret setup walkthrough

User-facing instructions to paste at this step:

1. Open https://crates.io and log in via GitHub OAuth (the page 404s if not logged in)
2. Go to https://crates.io/settings/tokens
3. Click **New Token**. Name it (e.g. `<repo>-publish`). Scope to `publish-new` + `publish-update`. Create.
4. Copy the token (shown once).
5. Open https://github.com/planetaryescape/<crate>/settings/secrets/actions/new
6. Name: `CARGO_REGISTRY_TOKEN`. Value: the token. Save.

The `crates.io 404 on cargo login` symptom is the most common stall. It's not broken — they just haven't browser-logged-in yet.

## Triggering the publish

After secret is in place:

```bash
git tag -a v0.1.0 -m "v0.1.0

Initial release of <crate>. <one-line summary>."
git push origin v0.1.0
```

Watch:

```bash
gh run watch --repo planetaryescape/<crate> $(gh run list --repo planetaryescape/<crate> --limit 1 --json databaseId --jq '.[0].databaseId')
```

`mail-threading` took 34 seconds from tag push to crates.io live.

## Verify

```bash
cargo search <crate> --limit 3
curl -sI https://docs.rs/<crate>/0.1.0 | head -1   # 302 = built
curl -sI https://crates.io/api/v1/crates/<crate> | head -1   # 200 = live
```

docs.rs builds asynchronously; expect 1–10 minutes after publish.

## Patch releases

```bash
# in standalone repo
# bump version in Cargo.toml
cargo publish --dry-run
git commit -am "release: v0.1.1"
git tag -a v0.1.1 -m "v0.1.1"
git push origin main v0.1.1

# in mxr
cargo update -p <crate>   # works once registry version exists
scripts/cargo-test -p <consumer-crate> --tests
```

No new secret needed — the existing `CARGO_REGISTRY_TOKEN` is good for the lifetime of the token. Rotate yearly.

## Cost of node20 deprecation

CI logs include this warning:

> Node.js 20 actions are deprecated. … forced to run with Node.js 24 by default starting June 2nd, 2026.

`actions/checkout@v4` is the affected one. When `v5` ships with Node 24 support, bump it in both workflow files. Not urgent.
