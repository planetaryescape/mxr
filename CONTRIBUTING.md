# Contributing to mxr

## Non-negotiables

- local-first first
- SQLite is canonical state
- search index is rebuildable
- daemon is the system
- TUI and CLI are both clients
- provider-specific logic stays in adapter crates
- compose uses `$EDITOR`
- rules are deterministic before they are clever
- plain-text-first rendering wins over flashy rendering

## Crate boundaries

Keep these intact:

1. `mxr-core` depends on nothing internal.
2. `mxr-protocol` depends only on `mxr-core`.
3. Provider crates depend only on `mxr-core`.
4. `mxr-store` and `mxr-search` depend only on `mxr-core`.
5. `mxr-sync` depends on `core + store + search`.
6. `mxr` (daemon crate) is the integration point.
7. `mxr-tui` depends only on `core + protocol`.

## Required checks

Run all of these before sending changes:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo sqlx prepare --check --workspace
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
- Keep `.github/workflows/` aligned with the actual build and release process.
- Keep issue templates and bug-report flow current.

## Useful references

- `docs/blueprint/`
- `docs/implementation/`
- `docs/blueprint/15-decision-log.md`
- `docs/blueprint/17-release-pipeline.md`
- `docs/blueprint/18-bug-reporting.md`
