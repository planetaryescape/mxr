# Delight Plan Maintainer Notes

This document preserves the implementation thinking behind the completed "delight plan" work. It replaces the old session handoff notes under `docs/archive/vision/`.

The goal of the work was not one feature. It was a parity push: make the useful mail workflows feel complete across the daemon, CLI, TUI, HTTP bridge, and desktop app without turning every client into a separate product. The daemon remains the source of truth; clients expose the same capabilities in shapes that fit their surface.

## What Shipped

The plan landed these broad areas:

- Faster navigation and feedback: optimistic mutation rollback, better command palette ranking/recents, richer inbox row formatting, type-ahead search, saved-search shortcuts, TUI saved-search tab strip, and visible pending row markers.
- Triage and follow-up workflows: reply-later, custom snooze, auto-reminders, Send Later, screener decisions, and bulk unsubscribe.
- Writing helpers: snippets, compose-time `;name` snippet expansion, sender profile, thread summarize, draft assist, and LLM provider integration through OpenAI-compatible HTTP.
- Recovery and diagnostics: crash-safe drafts, doctor findings with structured categories/severity/remediation, setup/demo onboarding, and desktop diagnostics rendering.
- Bridge and desktop parity: the HTTP bridge exposes the mail/platform routes needed by desktop dialogs and actions; the desktop app now has command/dialog surfaces for snippets, reply queue, sender view, screener, summaries, draft assist, reminders, send-later, saved-search shortcuts, and setup/demo guidance.

The only intentional product deferral from the parity push is a full-screen TUI sender profile. The modal is the supported TUI surface for now. Build the full-screen page later only if it will add richer comparison, charts, or sender history navigation beyond what the modal can responsibly hold.

## Implementation Philosophy

The implementation stayed daemon-first. If a workflow affects durable mail state, it belongs in daemon/store/IPC first, then CLI, then UI clients. The TUI and desktop should not invent state machines that only exist in that client.

CLI completeness mattered because it is the scriptable and easiest-to-test surface. Features like reply-later, reminders, send-later, snippets, sender view, screener, summaries, draft assist, setup, and crash-safe drafts all have CLI affordances. When adding future features, treat "desktop-only" or "TUI-only" as incomplete unless the feature is truly client-specific.

For the TUI, the chosen pattern was "small modal/browser surfaces before new screens." Snippets, sender view, screener, reply queue, summaries, snooze, setup, and platform/AI results are exposed through modals with consistent keys. This kept the implementation discoverable without creating a large set of half-finished full-screen pages.

For desktop, parity means using existing bridge routes and request coordination rather than duplicating daemon logic in Electron. Search uses explicit debounce plus request cancellation. Mutations and dialog fetches should continue to flow through `fetchJson` and `requestCoordinator` so stale responses do not overwrite newer UI state.

## Feature Notes

### Optimistic Mutations

TUI optimistic mutation rollback uses bounded snapshots. The important invariant is that the UI may show the user's intended result immediately, but there must be enough pre-mutation state to roll back on daemon failure. This is why snapshots are bounded and why row-level pending markers now exist: users need visible feedback that local state is waiting for daemon reconciliation.

Future mutation features should decide upfront:

- What local state changes optimistically.
- What snapshot is needed to undo it.
- What the user sees while reconciliation is pending.
- Whether the mutation is destructive or batch-like and therefore needs preview/confirmation.

### Command Palette and Hint Bar

The palette ranking intentionally prefers exact matches, then prefix, word-prefix, substring, and shortcut/category. Recents are persisted in TUI local state so useful commands survive restarts.

The hint bar was intentionally slimmed to top contextual hints. Avoid dumping every shortcut into the primary UI. Put full discovery in help/palette.

### Inbox Row Formatting

The row formatters exist to keep the list scannable, not decorative. Sender fallback order, relative dates, subject/snippet truncation, attachment chips, and pending markers should remain boring and predictable. If future rendering adds more badges, protect subject readability first.

### Search and Saved Searches

Search is navigation. TUI search uses debounce and cancellation, and desktop now mirrors that with explicit debounce plus replaceable requests. Saved-search shortcuts use `g` + digit. Index `0` means inbox/all-mail reset behavior; saved searches start at `1`.

The TUI saved-search tab strip is a visual affordance over existing state, not a second saved-search model. Keep it that way. If saved-search editing grows, still route durable changes through the daemon and CLI/IPC surfaces.

### Reply-Later

Reply-later is local-only state. It does not round-trip to providers. The queue exists to help users defer replies without changing provider state.

`mxr replies walk` was added as the CLI triage path. It walks queue items one at a time and supports reply, clear, skip, and quit. Reply reuses the existing compose/reply flow; it should not grow its own send path.

TUI and desktop expose queue browsing, while the CLI remains the stronger workflow for walking and composing.

### Snooze, Auto-Reminders, and Send Later

These are related but distinct:

- Snooze hides received mail until a wake time.
- Auto-reminders are follow-up reminders for sent mail.
- Send Later schedules drafts to be sent in the future.

Keep these concepts separate. Do not collapse them into one generic "scheduled thing" UI unless the underlying domain differences remain visible.

Daemon loops process due reminders and scheduled sends on a 60s cadence. The important operational pattern is a small testable `process_due_*` function plus a loop wrapper registered in runtime tasks for graceful shutdown.

Desktop send-later saves the compose session first and then schedules the resulting draft ID. That bridge response includes `draft_id` specifically so desktop can schedule a durable draft instead of scheduling a transient editor session.

### Screener

Screener decisions are sender-level consent triage. The key dispositions are allow, deny, feed, and paper-trail. TUI and desktop both use `a`/`d`/`f`/`p` to keep muscle memory consistent.

Enhancements should focus on review speed and clear consequences. Avoid making screener decisions feel like normal labels; they are policy decisions about future mail from a sender.

### Snippets

Snippets are durable named text blocks. CLI manages them. TUI/desktop currently expose browse/read surfaces. Compose-time expansion recognizes known `;name` tokens before validation/save/send and leaves unknown snippets literal.

That "unknown remains literal" rule is intentional. It avoids destructive surprises while drafting and lets users write semicolon text normally.

Future snippet enhancements should consider variables carefully. If variables are introduced, expansion should remain pure/testable and should fail in a way that keeps the draft recoverable.

### Sender View

Sender view is an aggregate relationship/profile surface. It is useful for context, not a new mailbox model. The TUI modal is the supported shape for now; desktop has a richer dialog.

A full-screen TUI `Screen::SenderProfile` was explicitly deferred. Build it only if the design needs more than a modal can provide.

### LLM Features

LLM support is behind an OpenAI-compatible HTTP provider shape. This covers local and remote providers such as Ollama, LM Studio, OpenAI, Groq, and OpenRouter.

Thread summarize and draft assist should never silently send mail. Draft assist generates text grounded in thread context and user instruction; the user still owns editing and sending.

Semantic retrieval is not required for the basic draft-assist path. Do not make core compose/send behavior depend on semantic readiness.

### Crash-Safe Drafts

Crash-safe drafts treat `sending` drafts with stale heartbeat/activity as recoverable local state. Startup maintenance resets orphaned drafts back to `draft` so users can retry.

CLI recovery commands exist because they are the safest operational surface:

- `mxr drafts recover`
- `mxr drafts resume <id>`
- `mxr drafts discard <id>`

Enhancements should preserve the invariant that a daemon crash during send should not strand a draft permanently.

### Doctor Findings

Doctor 2.0 uses structured findings: category, severity, message, and optional remediation commands. This is better than clients parsing prose.

All clients should render findings as structured data. Keep free-text next steps as secondary. If new diagnostics are added, add categories rather than stuffing meaning into message strings.

### Setup and Demo Onboarding

`mxr setup --demo` is the low-risk first-run path. It writes a fake account and lets users explore without provider credentials.

The TUI welcome modal exposes demo/Gmail/IMAP paths. Desktop has a command/palette affordance pointing to demo setup. Future onboarding should keep demo setup easy to reach and should not require OAuth just to understand the product.

## Bridge and Client Parity

The HTTP bridge routes added or used in this arc cover reply queue, reminders, scheduled sends, snippets, sender profile, screener, summaries, draft assist, drafts, signatures, subscriptions, semantic status, and diagnostics.

The bridge is not a place for screen-shaped payloads. It should expose reusable mail/platform capabilities. Desktop can shape those responses for dialogs, but the daemon/bridge should stay useful for other clients.

OpenAPI/type generation has existed as part of the workflow, but the desktop also has local types for the pieces it consumes. If future bridge routes are added, audit both generated types and local client types so they do not drift.

## Known Deferred or Risk Areas

The `cli_journey_*` daemon-startup tests were flaky before this consolidation. The symptom was `Starting daemon... failed.` even though logs showed the daemon listening on the socket. The likely issue is a startup/status race or writer-pool contention during startup work.

Suggested investigation path:

1. Find the user-facing `Starting daemon...` path.
2. Check daemon startup ordering around socket bind, task spawn, startup maintenance, and first status response.
3. Confirm startup maintenance is not blocking socket accept.
4. Consider smarter status retry/backoff in the CLI.

Remaining intentional product gaps:

- Full-screen TUI sender profile.
- Richer TUI draft-assist invocation. Today draft assist is CLI/desktop-led, while TUI has thread summarize.
- Full OpenAPI route audit to confirm all parity routes are documented/generated exactly as intended.

## Enhancement Guidance

When extending these features:

- Start with daemon/store/IPC unless the feature is truly client-local.
- Wire CLI at the same time as TUI/desktop.
- Keep destructive or batch mutations previewable.
- Preserve JSON/JSONL output for automation.
- Prefer small modal surfaces in TUI before adding new screens.
- Use desktop bridge calls and `requestCoordinator`; do not duplicate daemon workflows in React.
- Keep snippet expansion, time parsing, and diagnostics classification pure enough to unit test.
- Update CLI help snapshots when adding commands.
- Regenerate sqlx cache after new checked queries.
- Register migrations in both migration files and the runtime migration list.

## Verification Baseline

The final parity pass was verified with:

```bash
cargo check --workspace
cargo test -p mxr --test cli_help
cargo test -p mxr snippet_keywords --lib
cd apps/desktop && npm run format && npm run lint && npm run typecheck && npm run test
```

Use this as the minimum regression set after touching the same surfaces.
