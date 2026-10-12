# Contributing to mxr

## Dev setup

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo build -p mxr
scripts/cargo-test -p mxr --test cli_help
```

If you need the docs site too:

```bash
cd site
npm ci
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
- semantic search is an `mxr-platform` feature, not a core mail primitive
- semantic embeddings stay local
- semantic indexing uses real text extraction only; do not casually reintroduce OCR
- lexical exactness and semantic recall are different layers; do not blur them

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

Semantic boundary:

- `mxr-semantic` is the local dense-retrieval/runtime layer
- sync may persist semantic chunk data even when semantic retrieval is disabled
- embeddings are generated only when semantic is enabled or explicitly reindexed
- field-aware hybrid search semantics are intentional
- OCR is out of scope for active semantic indexing

Lifecycle guardrails:

- sync itself guarantees SQLite persistence + lexical freshness
- daemon post-sync work ingests semantic chunks for changed messages
- lexical startup repair is mandatory and rebuilds from SQLite
- semantic readiness is optional derived state; do not make core mail depend on it

Repo reality:

- The product/install/package surface is the repo-root package `mxr`.
- Internal crates under `crates/` are real workspace crates and are private by default (`publish = false`).
- The IMAP adapter depends on the published `mxr-async-imap` fork from crates.io; vendored source is not part of the workspace boundary model.
- Provider paths are `provider-gmail`, `provider-imap`, `provider-smtp`, and `provider-fake`.
- `crates/web` is a current client/bridge surface.

## Required checks

Run checks for the surface you changed. For Rust changes, update the floating
stable toolchain, inspect the affected packages, and run the scoped gate:

```bash
rustup update stable
scripts/pre-pr-rust-gate --list
scripts/pre-pr-rust-gate
scripts/cargo-test -p <changed-crate> --tests
cargo build -p mxr
```

The gate checks formatting, Clippy for touched crates, architecture boundaries
and dependency policy. CI runs the workspace checks, tests, SQLx offline check,
Rustdoc and builds when the changed paths require them. Use `--full` only to
reproduce a CI failure that cannot be narrowed to one crate.

On a shared agent workstation, queue heavy commands with
`agent-work run -- <command>` and keep workers at four or fewer. The dotfiles
Cargo wrapper already queues Cargo; do not nest another queue around it.
Use a separate target directory for each worktree.

For test-quality and web-facing changes, also run:

```bash
scripts/pre-pr-test-gate
```

That gate validates the test-quality audit script, runs strict test-quality
audit, and runs the web typecheck/lint/unit test suite.

Test helpers may use narrow `#[expect(clippy::...)]` annotations when
panic/unwrap/default-mutation keeps fixture failures clearer. Keep the
annotation on the smallest test module or function that needs it, and include
the reason.

If you touch the docs site:

```bash
cd site
npm ci
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

If local dev state gets messy, prefer the built-in reset flow over manual file hunting:

```bash
mxr reset --hard --dry-run
mxr burn --dry-run
```

That wipes rebuildable local runtime state only. It does not delete `config.toml` or system credentials by default.

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
- If you touch semantic search, update both code and docs. The behavior is subtle enough that stale docs become bugs.

## Adapter work

Adapter crates are replaceable by design.

When adding or changing an adapter:

1. Keep provider-specific code inside the adapter crate.
2. Map into the mxr internal model, not the other way around.
3. Validate against fake/conformance coverage.
4. Document any provider semantic mismatch honestly.

## Docs and release hygiene

- Update `README.md` for changed user-facing behavior.
- Update `site/` docs and sidebar navigation for new commands or workflows.
- Update architecture/blueprint docs when code changes invalidate older assumptions.
- Keep `.github/workflows/` aligned with the actual build and release process.
- Keep issue templates and bug-report flow current.

### Regenerate and verify the docs site

Use Node 24, matching the Docs Build CI job:

```bash
cd site
npm ci
npm run build
```

The build generates CLI pages from committed help snapshots, runs the docs
validator tests and content checks, builds the site, then creates `llms.txt`
exports. Edit `site/scripts/generate-cli-reference.mjs` for handwritten CLI
examples; never edit generated pages under `reference/cli/`.

The OpenAPI generator uses Cargo when available and otherwise retains
`site/public/openapi.json`. For prose-only work, that committed spec is enough.
When changing the HTTP API, build `dump_openapi_spec` and run the built binary
directly into the output file; queue status text must never enter the JSON.

Check `info.version` against the workspace version in `Cargo.toml` even when
the routes have not changed. A successful Node-only build can retain an older
spec. Reference generation also preserves handwritten examples: run those
against an isolated profile, including their selection semantics. For example,
`from:alice@example.com` matches that full address; `from:alice` does not search
sender display names.

Add user guides to `site/astro.config.mjs` and check their rendered links and
anchors. `npm run validate` checks known content errors and schema invariants;
it is not a general broken-link checker. After merging, verify the changed
pages on mxr.sh. A docs-only change deploys the site without a binary release.

## Architecture pointer

Start with [ARCHITECTURE.md](ARCHITECTURE.md), then use the blueprint and [implementation journey](docs/implementation-journey.md) for phased history and maintainer context.

## Good first issues

Look for the [`good first issue`](https://github.com/planetaryescape/mxr/labels/good%20first%20issue) label if you want a bounded starting point.

## Useful references

- [ARCHITECTURE.md](ARCHITECTURE.md)
- `docs/blueprint/`
- `docs/implementation-journey.md`
- `docs/blueprint/15-decision-log.md`
- `docs/blueprint/17-release-pipeline.md`
- `docs/blueprint/18-bug-reporting.md`
