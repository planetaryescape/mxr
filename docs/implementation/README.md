# mxr — Implementation Plans

Phased implementation plans for building mxr. Each document is designed to be consumed by a coding agent for autonomous implementation.

> **Current Layout Note**
> These plans were written before the workspace-boundary cleanup. Current code uses real internal workspace crates under `crates/`, with the repo-root package `mxr` as the product/install surface. Old `mxr-*` names are real internal crates again, but crates.io-specific release steps in these plans are historical unless explicitly updated elsewhere.

## Superseded items (treat as done)

These planned deliverables were **replaced** or **closed by an explicit product decision**. They are not missing work unless you want the *literal* old shape back.

| Original plan | What superseded it |
|---------------|-------------------|
| `mxr search --save` / `--saved` (Phase 1 doc) | `mxr saved add` / `mxr saved run` |
| Documentation via mdBook under `docs/book/` (Phase 4) | Docs site under `site/` |
| Tag workflow: publish all crates to crates.io + four-target musl matrix (Phase 4 long form) | Internal crates + `cargo install --git …`; see `.github/workflows/release.yml` (matrix and release-please integration) |
| `git-cliff` + commitlint exactly as in Phase 4 CI appendix | Release/changelog driven by current CI (no requirement for repo-root `cliff.toml` or commitlint to ship) |
| Hybrid plan §6: OCR for images / scanned PDFs | **Policy:** semantic indexing uses real text extraction only; no OCR (`CLAUDE.md` / `AGENTS.md`) |
| Pre–v0.5 experimental HTTP bridge (optional token, old paths, permissive CORS) | Managed + standalone bridge: `/api/v1/…`, token + host allowlist, strict CORS — see `docs/guides/http-bridge.md` and `docs/implementation/01-http-bridge-v0.5.md` |

## Still outstanding (verify in code / product)

Not covered by the table above — worth tracking intentionally:

- **`mxr accounts reauth NAME`** (Phase 1 checklist): no matching subcommand in `mxr accounts`; OAuth refresh / re-link may need a dedicated CLI or corrected remediation strings elsewhere (e.g. daemon status text that still says `mxr accounts reauth`).
- **Optional distro packaging** from Phase 4 body text: in-repo **AUR** / **Nix flake** if you want those install paths first-class.
- **HTTP bridge v0.5 slice ladder**: confirm full **`Request` ↔ route** parity and integration coverage against the OpenAPI contract (automated conformance helps; exhaustive audit may still find gaps).
- **Launch / marketing artifacts**: demo GIF, announcement checklist, HN post — Phase 4 “announcement ready” items.

## How to Use These Docs

1. Read the [blueprint](../blueprint/) first for requirements and design rationale
2. Read the [addendum](../blueprint/16-addendum.md) for post-blueprint amendments (A001-A008)
3. Read the [release pipeline](../blueprint/17-release-pipeline.md) for CI/CD, publishing, and release automation (D066-D071)
4. Implement phases in order (each phase depends on the previous)
5. Within each phase, follow the step ordering (dependency-driven)
6. Use the "Definition of Done" section to verify each phase before moving on
7. Refer to [decision log](../blueprint/15-decision-log.md) when making implementation choices — do not re-debate settled decisions

## Document Index

| # | Document | Phase | What it covers |
|---|---|---|---|
| 00 | [Workspace Setup](00-workspace-setup.md) | Pre-phase | Cargo workspace, root `mxr` package, dependencies, toolchain, CI, project scaffolding |
| 01 | [Phase 0](01-phase-0.md) | 0 | Prove the architecture: core types, store, search, protocol, fake provider, sync engine, daemon, TUI. Includes A005 keybindings, A006 basic logging, event_log table. |
| 02 | [Phase 1](02-phase-1.md) | 1 | Gmail read-only + search: Gmail adapter, real sync, query parser, TUI enhancements, config. Includes A004 read CLIs (cat/thread/headers/count/saved), A005 g-prefix navigation, A006 basic status/logs. |
| 03 | [Phase 2](03-phase-2.md) | 2 | Compose + mutations + reader mode + IMAP. Includes A001 inline compose, A002 markdown rendering, A004 full mutation CLIs + batch --search, A005 Gmail-native keybindings, A007 basic batch ops (x/V select), A008 IMAP first-party adapter. |
| 04 | [Phase 3](04-phase-3.md) | 3 | Export + rules + polish. Includes A004 remaining CLIs (labels/notify/events), A006 full observability (logs/status/events/doctor --check), A007 advanced batch (pattern select/vim counts). |
| 05 | [Phase 4](05-phase-4.md) | 4 | Community + release. **Note:** long embedded snippets are partly historical; see [Superseded items](./README.md#superseded-items-treat-as-done). |
| 06 | [Hybrid Search](06-hybrid-search.md) | cross-phase | Lexical search stabilization, semantic schema/profile state, local model delivery, hybrid retrieval, CLI/TUI wiring, rebuild/status/profile flows. |

## Addendum Feature Distribution

Every addendum feature (A001-A008) mapped to its implementation phase:

| Addendum | Feature | Phase |
|---|---|---|
| A001 | CLI compose without $EDITOR | Phase 2 |
| A002 | Markdown invisible to recipients | Phase 2 |
| A003 | Web client feasibility | Future (informational, no implementation) |
| A004 | Complete CLI surface | Phase 1 (reads), Phase 2 (mutations + batch), Phase 3 (labels/notify/events) |
| A005 | Vim+Gmail keybindings | Phase 0 (nav), Phase 1 (g-prefix), Phase 2 (action keys) |
| A006 | Daemon observability | Phase 0 (tracing init + event_log table), Phase 1 (basic status), Phase 3 (full logs/events/doctor) |
| A007 | TUI batch operations | Phase 2 (x toggle + V visual + basic batch), Phase 3 (pattern select + vim counts) |
| A008 | IMAP first-party | Phase 2 (adapter + JWZ threading) |
| A009 | Bug reporting (`mxr bug-report`) | Phase 4 (sanitizer, report generator, CLI subcommand, log retention/pruning) |

## Key Decisions Encoded

These decisions are settled (see [decision log](../blueprint/15-decision-log.md) and [addendum](../blueprint/16-addendum.md)):

- Unified `mxr` binary with clap subcommands
- Rust edition 2021
- Runtime sqlx queries for Phase 0, compile-time checked from Phase 1+
- Two-pool SQLite architecture (single writer + concurrent reader pool)
- Length-delimited JSON over Unix socket for IPC
- `yup-oauth2` for Gmail OAuth2
- Tantivy indexing with body text at sync time (D049)
- Keybinding hierarchy: vim-native first, Gmail second, custom last (D035)
- IMAP is first-party, not community (D048, overrides D015)
- Every TUI action has a CLI equivalent (D026)
- Auto-format detection: TTY → table, piped → json (D032)
- CLI-first product surface, TUI supported by the same daemon requests
- Mutations are dry-runnable before commit
- JSON/JSONL output is part of the product surface, not an afterthought
- Semver + conventional commits; changelog tooling may be git-cliff **or** release automation as configured in CI (D066 — implementation varies)
- Cross-compilation: `cross` for Linux musl, native for macOS (D067) — release matrix may not build every historical target
- One install surface for `mxr`; Cargo installs use git/path rather than crates.io
- Homebrew tap with auto-update on release (D069)
- No Windows builds in v1 (D070)
- cargo-binstall compatibility via naming convention (D071)
- `mxr bug-report` single diagnostic capture command (D072)
- Automatic log sanitization with opt-out (D073)
- Log retention: 90 days, 250 MB max text logs (D074)

## Phase Dependencies

```
Workspace Setup -> Phase 0 -> Phase 1 -> Phase 2 -> Phase 3 -> Phase 4
```

Each phase produces something usable. Don't build infrastructure for future features — build features that work.
