# mxr — Project Context for AI Agents

> A local-first, open-source, keyboard-native email client for terminal users, built around a daemon, a clean provider-agnostic model, and a programmable core.

## Language & Stack

- **Language**: Rust (edition 2021)
- **Async runtime**: Tokio
- **Database**: SQLite via sqlx (two-pool: single writer + concurrent reader pool, WAL mode)
- **Search**: Tantivy (BM25, field boosts, progressive indexing)
- **TUI**: Ratatui + crossterm
- **Email parsing**: Stalwart mail-parser
- **SMTP**: Lettre
- **Gmail**: Direct REST API via reqwest + yup-oauth2
- **IMAP**: async-imap (first-party adapter, CONDSTORE/QRESYNC + UID fallback + IDLE)
- **IPC**: Length-delimited JSON over Unix domain socket

## Architecture

Daemon-backed. The daemon is the system. TUI and CLI are clients connected via Unix socket.

```
TUI / CLI / Web / Scripts  <--- Unix socket (JSON) --->  Daemon
                                                           |
                                              Store (SQLite) + Search (Tantivy)
                                                           |
                                       Providers (Gmail, IMAP, SMTP, Fake)
```

Single unified binary: `mxr` with subcommands (`mxr` = TUI, `mxr daemon`, `mxr sync`, `mxr search`, etc.).

## IPC Contract Boundary

The transport is settled: length-delimited JSON over a Unix socket using `IpcMessage { id, payload }`.

Classify IPC into four buckets:

1. `core-mail`: stable mail/runtime capabilities
2. `mxr-platform`: accounts, rules, saved searches, subscriptions, semantic runtime
3. `admin-maintenance`: status, events, logs, doctor, bug reports, repair/inspection
4. `client-specific`: pane/selection/view shaping; keep this out of daemon IPC

Rules:

- The daemon serves reusable truth/workflows, not screen payloads.
- Product/platform capabilities are real first-class surfaces, not leftovers.
- Admin surfaces stay in IPC but stay conceptually separate from the core mail contract.
- Provider weirdness is handled below this layer in adapters.

## Core Principles (NON-NEGOTIABLE)

1. **Local-first**: SQLite is the canonical state store. Search index is rebuildable from SQLite. Works offline.
2. **Provider-agnostic internal model**: All app logic speaks one language (the mxr internal model). No provider-specific concepts leak into core code.
3. **CLI-first surface**: The CLI is the canonical user and automation surface. New capabilities land in the CLI first or at the same time. The TUI supports the CLI/daemon surface; it does not replace it.
4. **Daemon-backed architecture**: TUI is a client of the daemon, not the system itself. Background sync, indexing, and rules run regardless of TUI state.
5. **$EDITOR for writing**: Compose opens $EDITOR with markdown + YAML frontmatter. mxr does not compete with your text editor.
6. **Fast search is first-class**: Tantivy BM25 with field boosts. Search is navigation, not an afterthought.
7. **Saved searches are a core primitive**: User-programmed inbox lenses in sidebar and command palette.
8. **Rules engine is deterministic first**: Rules are data (inspectable, replayable, idempotent, dry-runnable). Scripts are escape hatches, not the foundation.
9. **Mutations are previewable**: Destructive or batch operations must have a dry-run or equivalent preview path before commit. Selection logic must match the real mutation path.
10. **Shell hooks over premature plugin systems**: Pipe data to shell commands. Unix composition over framework lock-in.
11. **Pipeable structured output**: JSON/JSONL output is a product feature, not an afterthought. Shells, scripts, and agents must be able to compose mxr commands cleanly.
12. **Adapters are swappable**: No provider-specific logic outside adapter crates. Ever.
13. **Correctness beats cleverness**: Plain, legible Rust. Compile-time checked SQL. Explicit error types. When in doubt, be boring.

## Data Model Design Philosophy

The internal model is the most important design decision in mxr. All application logic speaks this language. Gmail and IMAP (and any future provider) map INTO this model. The model never bends to accommodate provider quirks — that's the adapter's job.

Provider-agnostic does not mean lowest-common-denominator semantics. Keep provider truth visible where behavior differs: `LabelKind::Folder`, `provider_id`, provider cursors/capabilities, and native-thread-vs-JWZ threading. Labels-vs-folders is the delicate seam; do not simplify IMAP into Gmail labels.

### Key principles

1. **Provider-agnostic**: No Gmail-specific or IMAP-specific concepts in the core types.
2. **Correctness over cleverness**: We store enough data to round-trip back to the provider without loss.
3. **Eager body fetch**: Envelopes and bodies are always fetched together during sync. Opening a message is a pure SQLite read — no network call, no loading state.
4. **Typed IDs**: Newtypes prevent mixing up account IDs with message IDs at compile time.
5. **Time-sortable IDs**: UUIDv7 gives naturally ordered primary keys.

### Crate dependency rules

These are strict. Violations should be caught in code review:

1. **`core` depends on nothing internal.** It is the leaf node. All other crates depend on it.
2. **`protocol` depends only on `core`.** It defines the IPC contract between daemon and clients.
3. **Provider crates depend on `core` plus shared mail utility crates only.** Today that means `mail-parse` and `outbound`. They do NOT depend on store, search, sync, daemon, TUI, or web.
4. **`store` depends only on `core`, and `search` depends only on `core`.** They are storage backends, not business logic.
5. **`semantic` owns embeddings and dense retrieval.** It may depend on `core`, `config`, `reader`, and `store`. It must not depend on daemon, TUI, or provider crates.
6. **`sync` depends on `core`, `store`, `search`.** It orchestrates data flow between providers and local state.
7. **`daemon` is the integration point.** It depends on most crates. This is expected and acceptable — it's the application entry point.
   - **`daemon` MUST interact with providers only through `MailSyncProvider` / `MailSendProvider` traits.** Never import or call provider-specific types (e.g. `GmailClient`, `ImapClient`) from daemon handler/loop code. If a capability is needed, add it to the trait in `core` first, then implement it in the adapter. This is what makes providers swappable.
8. **`tui` and `web` are clients.** They may depend on `core`, `protocol`, and client-local utility crates such as `config`, `compose`, `reader`, and `mail-parse`, but they must not depend on daemon, store, search, sync, semantic, or provider crates.
9. **Architectural seams are Cargo seams.** Do not fake crate boundaries with `#[path]` source inclusion; use real workspace crates and normal path dependencies.

## Development Principles

### CLI first, TUI supported

If a feature only exists in the TUI, it is incomplete. The CLI is the fastest path to verification, scripting, and agent use. The TUI should layer on top of the same daemon request, not invent a separate capability.

### Test with the real system, not just unit tests

`cargo test` passing means nothing if the real daemon is broken. After implementing any feature:
1. Start the daemon (`mxr daemon --foreground`)
2. Test via CLI (`mxr star <id>`, `mxr labels`, `mxr compose --dry-run`)
3. Only then is it done

Unit tests with FakeProvider catch regressions but miss integration bugs. The label filtering bug shipped with 212 green tests because no test exercised the real sync→store→junction→query path.

### Wire both clients or wire neither

TUI and CLI are both IPC clients of the same daemon. Every `Request` variant in protocol must have both:
- A TUI action handler (in `app.rs`)
- A CLI subcommand (in `commands/`)

Wiring one and leaving the other as a stub means half the system is broken. The CLI is also the fastest way to smoke-test daemon features.

### Mutations must be dry-runnable

Batch or destructive mutations need a preview path. `--dry-run` should exercise the same selection/query path as the real mutation, then stop before provider or store mutation. Do not ship a mutation flow that can only be executed, not previewed.

### Pipeable JSON is mandatory

Read/list/status/search/export surfaces must keep machine-readable output stable. Prefer structured JSON for single payloads and JSONL for streams. Human-friendly table output is additive, not the only interface.

### Complete user journeys, not half-flows

A compose flow that opens `$EDITOR` but doesn't send is not a compose flow — it's a dead end. Think through the full journey: action → intermediate steps → result → feedback. If the user has to switch to CLI mid-flow, the TUI integration is broken.

### Integration tests over unit tests with fakes

Tests that cross component boundaries catch the bugs that actually ship:
- For store/query features: test through the sync engine, not raw store calls
- For mutations: verify state persists in the store after handler dispatch
- For multi-step flows (compose → parse → validate → send): test the full chain
- Test delta sync, not just initial sync — delta sync has different (and more fragile) codepaths
- When tests pass but the real system is broken, the tests are wrong

### SQLite CASCADE traps

`INSERT OR REPLACE` triggers `ON DELETE CASCADE` on foreign keys. This means re-upserting a parent row (like a label) silently deletes all child rows (like junction table entries). Use `INSERT ... ON CONFLICT UPDATE` instead when the row has dependents.

## Non-negotiables for Contributors

- Local-first by default
- SQLite is the canonical state store
- Search index is rebuildable from SQLite
- CLI-first product surface
- Provider adapters are replaceable
- No provider-specific logic outside adapter crates
- Compose uses $EDITOR
- Core features do not depend on proprietary services
- Rules are deterministic before they are intelligent
- Mutations are dry-runnable before commit
- JSON/JSONL output must stay pipeable
- TUI is a client of the daemon, not the system itself
- Distraction-free rendering: plain text first, reader mode, no inline images

## Key Design Decisions (settled, do not re-debate)

See `docs/blueprint/15-decision-log.md` for full context. Highlights:

- **Rust over Go** (D001): Tantivy, ratatui, ecosystem fit
- **Daemon over monolithic** (D002): background sync, multiple clients, headless operation
- **sqlx over ORMs** (D003): async, compile-time checked, explicit SQL
- **Tantivy over FTS5** (D004): BM25 with field boosts, faceted search, scales to 100k+
- **Direct Gmail API over gws CLI** (D006): delta sync via history.list, no external deps
- **Split MailSyncProvider / MailSendProvider traits** (D007): SMTP can't sync
- **YAML frontmatter for compose** (D009): Hugo/Obsidian pattern, widely known
- **Plain text first rendering** (D010): distraction-free is a feature
- **JSON over Unix socket for IPC** (D017): simple, debuggable, fast enough
- **Keybinding hierarchy** (D035): vim-native for navigation, Gmail for email actions, custom last
- **IMAP first-party** (D048): overrides D015, IMAP is now official adapter in Phase 2
- **Every TUI action has CLI equivalent** (D026): full scriptability via CLI flags + `--search` for batch

## Documentation

- `docs/blueprint/` — What to build (requirements, design, decisions)
- `docs/blueprint/16-addendum.md` — Post-blueprint amendments A001-A008 (inline compose, full CLI surface, vim+Gmail keybindings, observability, batch ops, IMAP)
- `docs/implementation/` — How to build it (step-by-step implementation plans per phase)

## Workspace Layout

```
crates/
  core/           # Types, traits, errors (leaf node)
  store/          # SQLite persistence via sqlx
  search/         # Tantivy indexing and query
  semantic/       # Local embeddings, dense retrieval, attachment extraction
  protocol/       # IPC types (Request, Response, Command)
  mail-parse/     # Shared RFC 5322/mail parsing helpers
  outbound/       # Shared outbound message rendering/building
  provider-gmail/ # Gmail API adapter (first-party)
  provider-imap/  # IMAP adapter (first-party)
  provider-smtp/  # SMTP send adapter
  provider-fake/  # In-memory test provider
  sync/           # Sync engine (providers <-> store <-> search)
  compose/        # $EDITOR workflow, frontmatter, draft UX
  reader/         # Reader mode (HTML->text, signature/quote stripping)
  rules/          # Deterministic rules engine
  export/         # Thread export (markdown, JSON, mbox, LLM context)
  daemon/         # Background process, socket server
  tui/            # Ratatui frontend
  web/            # HTTP/WebSocket bridge client
```

Repo reality:

- The product/install/package surface is the repo-root package `mxr`.
- Internal crates under `crates/` are real workspace crates and are private by default (`publish = false`).
- The IMAP adapter depends on the published `mxr-async-imap` fork from crates.io; vendored source is not part of the workspace boundary model.
- Contributors should express boundaries through Cargo dependencies, not `#[path]` pseudo-crates.
