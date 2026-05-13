# Build Sequence

This file is the cross-session execution plan. Keep diffs surgical. Each slice
must leave CLI, daemon IPC, store, and TUI either all wired or explicitly not
started.

## Ground Rules

- No broad refactors.
- Preserve current dirty worktree. Before implementation, inspect status and
  avoid unrelated files.
- Each vertical slice starts with behavior tests.
- JSON/JSONL surfaces are acceptance criteria.
- TUI never invents behavior that CLI/daemon do not expose.
- When an LLM feature is disabled, deterministic fallback must still be useful
  or the error must be explicit.

## Track 1: Pre-Send Safety Bundle

### Slice 1.1 Deterministic Safety Crate

Add a small `mxr-safety` crate or module owned below daemon. Prefer a crate if
multiple clients/tests need it directly.

Implements:

- missing attachment regex
- PII/secrets detectors
- reply-all heuristic
- recipient history/domain checks
- report structs or internal structs converted to protocol types

Tests:

- Unit tests for every deterministic check.
- No LLM dependency.

Acceptance:

- `cargo test -p mxr-safety` or equivalent module tests pass.

### Slice 1.2 Protocol And CLI Check

Add:

- `Request::CheckDraftSafety`
- `ResponseData::DraftSafetyReport`
- `mxr send <draft-id> --check`
- `mxr compose ... --check` only if inline compose can build a draft without
  sending

Tests:

- Protocol category test.
- CLI help snapshot.
- CLI JSON report snapshot.

Acceptance:

- `mxr send --check --format json` works against daemon.

### Slice 1.3 Send Gate Integration

Wire safety into:

- `SendDraft`
- `SendStoredDraft`
- scheduled-send flusher

Rules:

- blockers prevent provider call
- warnings return report unless override provided
- override token is explicit and single-use

Tests:

- FakeProvider send not called when blocker exists.
- Override allows send.
- Scheduled send with blocker keeps draft unsent and logs event.

Acceptance:

- Real send path and check path share the same function.

### Slice 1.4 LLM Answer Coverage

Add LLM-backed answer coverage after deterministic checks.

Tests:

- Mock LLM returns 3 asks, 1 missing.
- Invalid citation rejected.
- Disabled LLM degrades without blocking.

Acceptance:

- Safety report includes cited missing ask.

### Slice 1.5 TUI Safety Modal

Show safety report in compose confirm.

Tests:

- TUI snapshot for warning report.
- TUI snapshot for blocker report.

Acceptance:

- User can edit draft, rerun send, and see updated report.

## Track 2: Commitments And Owed Replies

### Slice 2.1 Draft Commitment Candidates

Add extraction to safety report and promotion after sent ingest.

Tests:

- `--check` shows candidate, no ledger row.
- successful send creates `contact_commitments`.
- failed send creates no row.

Acceptance:

- `mxr commitments --status open` sees outgoing promise after send.

### Slice 2.2 Owed Reply IPC And CLI

Add `ListOwedReplies`, `mxr owed`, and `is:owed-reply` only after daemon query is
stable.

Tests:

- Store/query behavior from real SQLite fixtures.
- CLI table/json/csv/ids.
- Search operator matches list output.

Acceptance:

- `mxr owed --format ids` is scriptable.

### Slice 2.3 TUI Owed Lens

Add sidebar lens and keybindings using same IPC.

Tests:

- Snapshot empty state.
- Snapshot populated lens.
- Reply send removes row after refresh.

## Track 3: Archive Intelligence

### Slice 3.1 Archive Ask

Add `mxr ask` with citations.

Tests:

- Retrieval mode fallback.
- Date/from filters enforced before prompt.
- Invalid citations rejected.

Acceptance:

- JSON output includes answer, citations, retrieval metadata.

### Slice 3.2 Decision Log

Add schema, extractor, CLI.

Tests:

- Extract explicit decision.
- Ignore brainstorming.
- Rebuild idempotent.

Acceptance:

- `mxr decisions --topic X --format json` works.

## Track 4: Timing And Cadence

### Slice 4.1 Send-Time Optimizer

Compute from `reply_pairs`.

Tests:

- bucket choice
- low sample suppression
- multi-recipient report

Acceptance:

- `mxr send-time <email>` works and `send --check` can include info.

### Slice 4.2 Cadence Watchlist

Add watchlist schema and CLI.

Tests:

- watch/unwatch
- drift list
- list sender default rejection

Acceptance:

- `mxr cadence drift --format json` returns watched contacts only.

## Track 5: Briefings And Collaboration

### Slice 5.1 Thread Briefing

Add briefing cache and CLI.

Tests:

- dormant threshold
- cache invalidation
- citations
- LLM disabled fallback

Acceptance:

- `mxr briefing thread <id>` works.

### Slice 5.2 Recipient Briefing

Use relationship profile and context cache.

Tests:

- unknown contact
- long gap
- privacy block fallback

Acceptance:

- Compose can show non-invasive briefing hint.

### Slice 5.3 Maybe Include

Add collaborator suggestions.

Tests:

- repeated thread support required
- no auto-add
- Bcc not leaked

Acceptance:

- `mxr suggest-recipients --draft <id>` works.

### Slice 5.4 Expert Finder

Add expert suggestion query.

Tests:

- answerers ranked above askers
- citations point to answer messages
- LLM disabled fallback

Acceptance:

- `mxr expert <message-id>` works.

## Track 6: Knowledge Graph

Do this last. Start query-time with no schema. Add entity tables only after
`mxr whois` is useful and slow enough to justify persistence.

Tests:

- email query uses sender/relationship profile
- term query cites messages
- ambiguity returns candidates

Acceptance:

- `mxr whois <term>` is citation-backed.

## Session Handoff Checklist

At the end of each implementation session, update the relevant doc with:

- shipped slices
- commands run
- failing or skipped tests
- schema migrations added
- CLI/API surfaces changed
- next smallest slice

Do not mark a slice done unless:

- store/protocol/daemon/CLI are wired or explicitly not part of that slice
- tests pass or failure is documented
- JSON output works
- TUI has not diverged from daemon behavior

