# mxr Delight Plan Maintainer Notes

This document preserves the product and implementation thinking behind the completed delight-plan work. The old `docs/vision/` implementation trackers have been retired; this is the durable maintainer note for future contributors.

The goal was not one feature. It was a parity push: make the useful mail workflows feel complete across daemon, CLI, TUI, HTTP bridge, and desktop without turning every client into its own product. The daemon remains the source of truth; clients expose the same durable capabilities in shapes that fit their surface.

## Thesis

The product bet was:

> Email that respects the keyboard, the network, and your data.

That meant local-first, keyboard-native, instant-feeling, and practical enough that a non-Rust user could install it, try it, and understand the value without a multi-day setup project.

The differentiation only mattered if the opposite was credible:

- Cloud-native sync-everywhere clients can claim the opposite of local-first.
- Mouse-first visual clients can claim the opposite of keyboard-native density.
- DIY mail stacks can claim the opposite of guided setup and integrated workflows.

So the work focused on areas where `mxr` could be meaningfully different: local state, scriptable CLI flows, daemon-backed consistency, keyboard-first review, and relationship-aware mail features.

## How To Maintain This Area

Keep durable behavior daemon-first. If a workflow changes mail state, it belongs in store/daemon/IPC before it appears in a UI.

Keep the CLI complete. CLI is the scriptable surface, the fastest integration test surface, and the safest operational fallback when a TUI or desktop flow regresses.

Prefer one real workflow over several shallow surfaces. During this push, small TUI modals and browser/dialog surfaces often beat new full-screen pages because they exposed the workflow without creating half-finished navigation.

Use vertical TDD slices for new behavior: one failing behavior test, one minimal implementation, then refactor. The important tests are public-behavior tests through daemon, store, search, CLI, or rendered TUI state. Avoid tests that only restate a helper's current implementation.

When adding a mutation, decide upfront:

- What local state changes optimistically.
- What snapshot is needed to roll it back.
- What visible pending state the user sees.
- Whether the mutation needs a dry-run or preview path.
- How the CLI, TUI, and bridge names map to the same daemon request.

## What Shipped

The completed work landed these broad areas:

- Fast-feeling TUI feedback: optimistic mutation rollback, pending row markers, command palette discovery, richer inbox rows, type-ahead search, saved-search shortcuts, and a saved-search tab strip.
- Triage workflows: reply-later, reply queue walk mode, custom snooze, auto-reminders, Send Later, screener decisions, bulk sender triage, and unsubscribe.
- Relationship and writing helpers: snippets, contextual snippet variables, sender profile, thread summarize, draft assist, and an OpenAI-compatible LLM provider.
- Resilience and onboarding: crash-safe stored-draft recovery, structured doctor findings, setup guidance, and a curated demo path.
- Bridge and client parity: the bridge exposes reusable mail/platform routes; desktop and TUI shape those responses locally.

## Design Principles That Survived Implementation

### Daemon First

The daemon is the system. Durable workflow state belongs in SQLite and is surfaced through IPC. TUI, CLI, web, and desktop are clients.

Do not create client-only versions of core concepts like reminders, screener policy, reply-later, scheduled sends, snippets, or sender profiles. Client-local state is fine for selection, focus, modal state, and recent palette commands; it is not fine for durable mail workflow truth.

### CLI First

The CLI is not a secondary convenience. It is how users script `mxr`, how agents operate safely, and how we smoke-test daemon behavior.

If a durable feature ships only in TUI or desktop, it is incomplete unless the feature is genuinely client-specific.

### Local First, But Not Purist Theater

The project stays local-first where it matters: mail state, search, sync state, rules, reminders, snippets, drafts, and relationship data live locally. LLM features are optional and degrade cleanly when disabled.

The LLM backend deliberately pivoted away from bundling `mistral.rs`. The shipped model is an OpenAI-compatible HTTP provider that covers Ollama, LM Studio, OpenAI, Groq, OpenRouter, Together AI, Mistral La Plateforme, and similar providers. That kept compile cost, binary complexity, and model-artifact management out of the core app while still supporting local engines.

Do not reintroduce embedded inference as a replacement for the current provider. If native inference ever returns, it should be an optional additional provider with clear install and maintenance costs.

### Exactness Before Cleverness

Search remains lexical-first. Semantic/LLM features assist; they do not replace exact BM25 search, fielded operators, provider state, or deterministic rules.

Scheduling and reminders are separate concepts. Snooze hides received mail until a wake time. Auto-reminders nudge the user about sent mail that did not receive a reply. Send Later schedules a draft to be sent. They share time parsing patterns, but they should stay distinct in UI copy and daemon behavior.

## Phase Notes

### Phase 1: Make It Feel Right

The first phase was about perceived latency and discoverability. The TUI already had async plumbing; the missing piece was honoring that with local optimistic state and better visual feedback.

Optimistic mutation rollback uses bounded snapshots. The invariant is: the UI can show the intended result immediately, but enough pre-mutation state must exist to roll back on daemon failure. Snapshots are bounded so a stuck daemon cannot grow memory without limit.

The command palette became the primary discovery surface. The hint bar should stay slim and contextual; the palette is where exhaustive action discovery belongs. Keep shortcut labels tied to the keybinding registry so docs, palette text, and actual behavior do not drift.

Inbox row work was intentionally restrained. The goal was scannability, not decoration. Sender fallback, relative dates, snippets, attachment chips, participation chips, and pending markers should remain predictable. If future badges are added, protect subject readability first.

Type-ahead search exists to make Tantivy's speed visible. Empty query should reset the workspace. Stale responses must not overwrite newer query state.

Saved-search tabs are a visual affordance over the existing saved-search model. Do not create a second tab model. Durable saved-search changes still route through daemon/CLI/IPC.

Known enhancement point: unread counts for saved-search tabs are represented in app state and rendered, but refresh-loop behavior should remain a small, explicit slice if expanded.

### Phase 2: Triage That Scales

The second phase made `mxr` useful for high-volume review without leaving the keyboard.

Reply-later is local-only state. It does not round-trip to providers. It exists to defer replies without mutating provider labels or folders. Replying through any send path should clear the flag when the sent draft is tied to the parent message.

`mxr replies walk` is the canonical CLI triage loop for reply-later. It walks queue items and reuses the normal compose/reply/send path. Do not grow a second send path inside the walker.

Custom-time parsing is shared across user-facing scheduling flows. Supported grammar is `in Nm/h/d/w`, `today <time>`, `tomorrow [time]`, weekday forms, and RFC3339. The stale `+30s` shorthand from the old plan was not adopted; do not document or depend on it without deliberately extending the parser and tests.

Auto-reminders are follow-up reminders for sent mail. The important behavior is receipt-based: `--remind-after` can only set a reminder once the send path returns a sent message id. If an older daemon only returns an ack, the CLI must fail clearly rather than creating a reminder against the wrong id.

Due reminders become reply-later work. When a reminder fires, the sent message is marked into the reply queue and the TUI refreshes open reply-queue state. This keeps "nudge me if they do not reply" attached to the same triage surface as other deferred replies.

Send Later schedules stored drafts, not ephemeral editor sessions. The flusher clears `send_at` before sending so a crash or retry cannot double-fire the same draft. Restart persistence is part of the contract.

Screener decisions are sender-level consent policy, not labels. The dispositions are allow, deny, feed, and paper-trail. Enhancements should focus on decision speed and clear consequences; avoid making screener feel like ordinary label management.

Unsubscribe pivoted away from adding an `_unsubscribed` label. The durable idempotency marker is the event log. A successful one-click or mailto unsubscribe logs success; a failed request does not log success, so retry is not silently blocked.

### Phase 3: Sender As Unit

The third phase was the unique product bet: `mxr` already has local relationship data, so the sender can become a first-class unit.

Snippets are durable named text blocks managed by daemon/CLI. Expansion happens after `$EDITOR` closes, before validation/save/send. Unknown snippets and unresolved variables intentionally stay literal so drafting remains recoverable and normal semicolon text is not destroyed.

Supported snippet variables include date built-ins and contextual values such as first name, full name, and thread subject. Contextual expansion is best-effort from compose frontmatter. Missing recipient context should not invent data; it should leave the token literal and let send-time validation warn.

Sender view is an aggregate relationship/profile surface, not a mailbox model. It should answer "who is this and what do I owe them?" without taking over navigation. The TUI modal is the supported surface for now; a full-screen `Screen::SenderProfile` was intentionally deferred. Build it only if there is a richer comparison, charting, or sender-history navigation job that a modal cannot responsibly hold.

Thread summarize and draft assist are optional LLM features. They must degrade cleanly when LLM is disabled. They must never silently send mail.

Thread summarize should stay actionable and low-ceremony. The prompt was tuned away from pleasantries. Caching is content-hash based, with relationship/style context included so stale summaries do not survive context changes.

Draft assist is grounded on the current thread plus the user's instruction. When semantic search is available, it retrieves prior outbound examples to match voice; when semantic is disabled or unavailable, it falls back to thread-only prompting. The user's instruction and current thread outrank weak relationship/profile background.

Token budgeting in draft assist is intentional: preserve instruction first, preserve a transcript floor, then fit grounding and relationship context. Truncation must stay UTF-8 safe.

### Phase 4: Onboarding And Resilience

The fourth phase made the system safer and easier to try.

Crash-safe drafts currently recover stored drafts that are stranded in `sending`. Startup maintenance resets stale sending drafts back to `draft`, and `mxr drafts recover` exposes the recovery surface.

Heartbeat while `$EDITOR` is open was explicitly not implemented. The compose CLI writes to a temp file and only persists the draft to the daemon on save/send. Mid-editor heartbeat would require a different architecture: pre-save a placeholder draft, run a background heartbeat while the editor blocks, then reconcile final content on close. Treat that as a separate feature, not a bug in current crash recovery.

Doctor findings are structured data, not prose scraping. Findings carry category, severity, message, and remediation commands. The CLI and IPC use the same helper so `mxr doctor --json` and daemon status stay aligned.

OAuth remediation should reflect the current account flow. The old plan mentioned `mxr accounts reauth`; the current remediation points users through `mxr accounts add` or web OAuth reauthorization. Do not resurrect a stale command name without implementing it end to end.

`mxr demo` is now the canonical curated demo path. It starts an isolated fake inbox profile with seeded surfaces so users can explore without provider credentials. `mxr setup --demo` remains a legacy quick config helper.

## Cross-Cutting Contracts

### Mutations

Destructive or batch mutations need preview or dry-run discipline. The selection/query path used for dry-run must match the real mutation path.

Optimistic TUI updates must reconcile with daemon truth. If a daemon failure arrives, roll back visible state and tell the user what happened.

### Background Loops

Reminder and scheduled-send loops follow the same operational pattern:

- Keep the core work in a small `process_due_*` function that accepts explicit time.
- Wrap it in a loop registered with runtime tasks for graceful shutdown.
- Make due processing idempotent against crashes and restarts.
- Test persistence separately from loop timing.

### Time Parsing

Use the shared parser for human-facing scheduling. Do not add one-off time grammars per command. If the accepted grammar changes, update snooze, reminders, send-later, docs, and help snapshots together.

### Provider Boundaries

Provider weirdness belongs below the daemon IPC layer. The daemon should keep interacting through provider traits. Screener, unsubscribe, send-later, reply-later, reminders, snippets, and sender view are app/platform concepts, not Gmail or IMAP concepts.

### Search And Semantic Recall

Lexical search stays exact and rebuildable. Semantic recall is additive. Fielded dense behavior must respect source kind boundaries, and core mail/search behavior must not depend on semantic readiness.

### Documentation Surfaces

When a feature changes, update:

- CLI help snapshots and generated CLI reference.
- User guides for workflows and recipes.
- Config reference if keys changed.
- Search/operator docs if query grammar changed.
- TUI keybinding docs if bindings changed.
- JSON/OpenAPI docs if IPC or bridge shapes changed.

## Pivots And Non-Goals

The old implementation plan contained ideas that are intentionally not the shipped contract:

- No required embedded `mistral.rs` backend. Optional future provider only.
- No live Ollama integration test in default CI. Local provider constructor/runtime tests cover the shipped contract; live engine smoke belongs behind optional environment-gated QA.
- No streaming LLM chunks yet. Current summarize/draft-assist flows are single-shot; `supports_streaming` leaves room for future providers.
- No `_unsubscribed` label. Unsubscribe idempotency is event-log based.
- No `+30s` send-later shorthand. Use supported parser grammar such as `in 1m`.
- No `mxr accounts reauth` command in the shipped flow.
- No full-screen TUI sender profile until it solves a real product problem beyond the modal.
- No heartbeat while `$EDITOR` is open until compose persistence is redesigned.

## Future Enhancement Guidance

Good future work should make existing workflows more reliable, faster to operate, or easier to understand. Avoid adding adjacent features just because the primitives make them easy.

High-value extensions:

- Saved-search tab count refresh through daemon-backed counts, if it stays cheap and does not make the inbox feel noisy.
- Richer screener review tools, especially bulk decisions with explicit preview.
- A full-screen sender profile only if it adds comparison, history navigation, or charts that materially exceed the modal.
- Optional native LLM provider only if install, memory, and model lifecycle are first-class product surfaces.
- Mid-editor crash recovery only if compose persistence is redesigned around daemon-stored drafts from the start of editing.
- Environment-gated live provider smoke tests for LLM and mail adapters, kept out of normal CI.

Before adding a feature in this area, ask:

- Is this fixing a broken or confusing existing workflow first?
- Does the CLI surface exist and remain scriptable?
- Does the daemon own the durable state?
- Is there a dry-run or preview path for destructive/batch behavior?
- Does this preserve provider-agnostic boundaries?
- Can the behavior be tested through a public API rather than a private helper?

## Verification Baseline

The final consolidation passed:

```bash
scripts/cargo-test -p mxr --test cli_journey -- --nocapture
scripts/cargo-test -p mxr-store --lib scheduled_send -- --nocapture
scripts/cargo-test -p mxr --lib remind_after -- --nocapture
scripts/cargo-test -p mxr --test cli_help cli_help_snapshots_cover_all_commands -- --nocapture
scripts/cargo-test -p mxr-llm --lib -- --nocapture
scripts/cargo-test -p mxr --lib demo -- --nocapture
cd site && npm run build
```

Treat this as a focused baseline, not a substitute for broader release verification.
