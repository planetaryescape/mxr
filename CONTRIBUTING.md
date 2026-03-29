# Contributing to mxr

## Dev setup

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo build --workspace
cargo test --workspace
```

If you need the docs site too:

```bash
cd site
npm install
npm run build
```

## Non-negotiables

- local-first first
- SQLite is canonical state
- search index is rebuildable
- daemon is the system
- TUI and CLI are both clients
- web is also a client of daemon IPC
- provider-specific logic stays in adapter crates
- compose uses `$EDITOR`
- rules are deterministic before they are clever
- plain-text-first rendering wins over flashy rendering

## IPC boundaries

Classify every IPC addition before you add it:

1. `core-mail`
2. `mxr-platform`
3. `admin-maintenance`
4. `client-specific`

Rules:

- Core mail/runtime should stay boring and stable.
- mxr platform capabilities are first-class. Do not bury them as misc.
- Admin surfaces stay in IPC, but separate them mentally and in code from the mail contract.
- Client-specific shaping stays in clients.
- The daemon serves reusable truth/workflows, not screen payloads.

## Crate boundaries

Keep these intact:

1. `mxr-core` depends on nothing internal.
2. `mxr-protocol` depends only on `mxr-core`.
3. Provider crates depend on `mxr-core` plus shared mail utility crates only (`mail-parse`, `outbound`).
4. `mxr-store` and `mxr-search` depend only on `mxr-core`.
5. `mxr-sync` depends on `core + store + search`.
6. `mxr` (daemon crate) is the integration point.
7. `mxr-tui` and `mxr-web` are client crates. They may use local utility crates (`config`, `compose`, `reader`, `mail-parse`), but never daemon/store/search/sync/provider crates.
8. Do not use `#[path]` includes to simulate crate boundaries.

Repo reality:

- The product/install/package surface is the repo-root package `mxr`.
- Internal crates under `crates/` are real workspace crates and are private by default (`publish = false`).
- The IMAP adapter depends on the published `mxr-async-imap` fork from crates.io; vendored source is not part of the workspace boundary model.
- Provider paths are `provider-gmail`, `provider-imap`, `provider-smtp`, and `provider-fake`.
- `crates/web` is a current client/bridge surface.

## Required checks

Run all of these before sending changes:

```bash
cargo fmt --all -- --check
cargo nextest run --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo sqlx prepare --check --workspace
cargo deny check
```

If you touch the docs site:

```bash
cd site
npm install
npm run build
```

## Real-system verification

Green unit tests are not enough. For user-facing work, also test the real flow:

```bash
cargo run -- daemon --foreground
mxr status
mxr search "label:inbox"
mxr doctor --check
```

If you changed rules, exports, labels, notify, events, or logs, exercise the matching CLI surface too.

## Running the daemon

`mxr daemon --foreground` is the canonical manual-test entrypoint. Keep it running in one terminal, then use a second terminal for CLI smoke tests like `mxr status`, `mxr sync --status`, `mxr search`, and the mutation flow you changed.

## PR process

1. Fork the repo.
2. Create a focused branch from `main`.
3. Keep the diff surgical.
4. Run the required checks.
5. Open a PR with enough context to reproduce and verify.

CI must pass before review or merge.

## Rules for changes

- Keep blast radius small.
- Do not refactor adjacent code unless the task requires it.
- Add tests for new behavior.
- Prefer integration coverage over mock-heavy unit tests.
- Do not wire a daemon feature for only one client surface when both TUI and CLI need it.

## Adapter work

Adapter crates are replaceable by design.

When adding or changing an adapter:

1. Keep provider-specific code inside the adapter crate.
2. Map into the mxr internal model, not the other way around.
3. Validate against fake/conformance coverage.
4. Document any provider semantic mismatch honestly.

## Docs and release hygiene

- Update `README.md` for changed user-facing behavior.
- Update `site/` docs for new commands or workflows.
- Update architecture/blueprint docs when code changes invalidate older assumptions.
- Keep `.github/workflows/` aligned with the actual build and release process.
- Keep issue templates and bug-report flow current.

## Architecture pointer

Start with [ARCHITECTURE.md](ARCHITECTURE.md), then use the blueprint and implementation docs for the settled design and phase plans.

## Good first issues

Look for the [`good first issue`](https://github.com/planetaryescape/mxr/labels/good%20first%20issue) label if you want a bounded starting point.

## Useful references

- [ARCHITECTURE.md](ARCHITECTURE.md)
- `docs/blueprint/`
- `docs/implementation/`
- `docs/blueprint/15-decision-log.md`
- `docs/blueprint/17-release-pipeline.md`
- `docs/blueprint/18-bug-reporting.md`
