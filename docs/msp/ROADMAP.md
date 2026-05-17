# MSP roadmap

> **Current focus:** Step 1 — land spike artifacts. Step 2 (mxr
> alignment Phase A) is queued and ready to start.
>
> _Last updated: 2026-05-17._

This roadmap tracks the six steps from
[`spike-verdict.md`](./spike-verdict.md) plus the open-ended Step 7
that follows. Each step has explicit entry and exit gates so future
sessions can pick up without losing context.

## Status table

| Step | What | Status | Est. effort | Blocked on |
|------|------|--------|-------------|------------|
| 1 | Land spike artifacts in mxr | **In progress** | Done | — |
| 2 | mxr alignment Phase A — cheap wins | Not started | ~1 day | Step 1 |
| 3 | mxr alignment Phase B — opaque SyncCursor | Not started | ~2 days | Step 2 |
| 4 | Publish-or-hold decision | Not started | ~1 hour | Step 3 |
| 5 | Reference IMAP adapter as separate crate | Not started | ~1-2 weeks | Step 4 (if publish) |
| 6 | Further scope based on response | Not started | open-ended | Step 5 |
| 7 | Maintenance + v0.2 spec | Not started | ongoing | Step 6 |

## Step 1 — Land spike artifacts ✅

**Done:** The four spike artifacts plus this roadmap and the README
live under `docs/msp/`. Cross-references updated. Lesson 12
captures the meta-pattern.

**Exit gate met:** All files committed and referenced.

## Step 2 — mxr alignment Phase A (cheap wins)

The "cheap wins" from `mxr-alignment.md`. Pure clean-ups; no
behaviour change; mxr is strictly better afterwards.

**Tasks:**

1. **Namespaced `SyncCapabilities` restructure.** Replace the flat
   boolean soup in `crates/core/src/types.rs:1833-1843` with a
   nested struct (`{sync: {...}, mutate: {...}, push: {...},
   search: {...}}`). Same information, MSP-shaped.
2. **Typed `SyncCursorExpired` error variant.** Add to `MxrError`;
   adapters return it instead of `NotFound`; the daemon's recovery
   code in `crates/sync/src/engine.rs:243-264` becomes provider-
   agnostic (removes the Gmail special case).
3. **`Role` enum on Folder/Label.** Add a `role: Option<Role>`
   field to `Label`; populate from the existing `special_use`
   strings in `crates/provider-imap/src/session.rs:796-820` and
   Gmail's system labels.

**Entry gate:** Step 1 done.

**Exit gate:**
- Three commits land (one per task), each green on `cargo test
  -p <touched-crate> --tests`.
- Daemon smoke test passes against the fake provider via CLI (per
  AGENTS.md workflow): `mxr daemon --foreground &; mxr sync <fake>;
  mxr search ... --json`.
- No daemon regression for Gmail or IMAP if real credentials are
  convenient.

**Status:** Not started. Reading material: section "MSP §4 —
Capabilities," "MSP §5 — Resumability," and "MSP §2.3 — Folder" in
`mxr-alignment.md`.

## Step 3 — mxr alignment Phase B (opaque `SyncCursor`)

The biggest single architectural win. The `SyncCursor` enum at
`crates/core/src/types.rs:1751-1768` is a tagged union the daemon
pattern-matches on; the daemon's pattern-match couples it to
"which providers exist." MSP's opaque-cursor rule fixes this.

**Refactor sketch:**

- Change `SyncCursor` to `pub struct SyncCursor(Vec<u8>)` (or a
  thin wrapper around `serde_json::Value`).
- Move the variant-specific logic into each provider crate.
- Update `crates/store/src/sync_cursor.rs` to persist opaque bytes
  (no schema change — it's already JSON).
- The daemon's "Gmail cursor not-found, reset to Initial" recovery
  becomes "adapter returns `SyncCursorExpired`, daemon clears
  state and re-syncs." (Builds on Step 2's typed error.)

**Entry gate:** Step 2 done.

**Exit gate:**
- mxr-search, mxr-sync, mxr-store tests all green.
- Daemon smoke test against fake provider passes.
- If feasible, smoke test against real Gmail + IMAP accounts.
- mxr's daemon no longer pattern-matches on `SyncCursor` variants
  anywhere outside the provider adapters.

**Status:** Not started.

## Step 4 — Publish-or-hold decision

Decision point. By this stage mxr is meaningfully closer to MSP
shape. The spec draft has had a few days to settle. We decide
whether to:

- **Publish** the blog post + spec to a Rust forum or HN, OR
- **Hold** and keep refactoring without external surface.

The decision is informed by:

- Does the spec still feel right after living with it for a few
  days?
- Do we have at least one concrete external person we'd want to
  reach with the post (a known mail-client builder, an existing
  Rust mail-crate maintainer)?
- Is there bandwidth to handle inbound feedback over the next 2-3
  weeks?

If yes: publish. Go to Step 5.

If no: hold; revisit in a month. Roadmap pauses until the next
decision window. mxr-internal refactor work can continue (Phases
C-F from the alignment audit).

**Entry gate:** Step 3 done.

**Exit gate:** Decision recorded in this roadmap's changelog. If
publish, the spec gets a version tag in its frontmatter and the
blog post gets a publish date.

**Status:** Not started.

## Step 5 — Reference IMAP adapter as a separate crate

If we published in Step 4 and got non-negative response, this is
the first hard external surface.

**Scope:**

- New crate at `planetaryescape/msp-imap` (or similar — naming
  TBD).
- Implements MSP server-side over stdio JSON-RPC.
- Talks IMAP via the existing `provider-imap` code, packaged for
  standalone use.
- mxr's daemon learns to subprocess `msp-imap` as one of its
  adapter options (alongside the existing in-process IMAP
  adapter).

**Entry gate:** Step 4 = publish.

**Exit gate:**
- `msp-imap` crate published on crates.io (subject to the standard
  publishing-bar discipline from `lessons/10-publishing-bar.md`).
- mxr's daemon successfully subprocesses `msp-imap` and uses it
  end-to-end against a real IMAP account.
- An external project (himalaya? pimalaya? a new project?) has
  said they'd try the adapter.

**Status:** Not started.

## Step 6 — Further scope based on response

Open-ended. Depends on what response the blog post + IMAP adapter
generated.

Possible paths:

- **Second adapter** (likely Gmail, to stress-test the abstract
  model against a wildly different provider).
- **mxr alignment Phases C-F** (mutation unification, custom
  keywords, sync-delta shape) — converts mxr's daemon into a more
  fully MSP-compliant client.
- **v0.2 spec round** — incorporate lessons from the IMAP adapter
  + any external feedback.
- **Co-lead recruitment** — if there's interest but we're capped
  on bandwidth.
- **Working group formation** — only if there are 2-3 committed
  people.

**Entry gate:** Step 5 done.

**Exit gate:** TBD — depends on which paths we take.

**Status:** Not started.

## Step 7 — Maintenance + v0.2 spec

Long-term. If MSP becomes a real thing externally, this is the
ongoing maintenance:

- Issue triage on the spec repo (if separated from mxr).
- Adapter conformance harness work.
- Periodic spec versioning (v0.2, v0.3, ...).
- Compatibility tracking across adapters.

**Status:** Not started. Likely needs co-leads before this is
sustainable.

---

## Roadmap revisions / changelog

Append below as the roadmap evolves. Most recent first.

### 2026-05-17 — Roadmap created
- Six-step plan defined from the spike verdict.
- Spike artifacts consolidated under `docs/msp/` with this
  roadmap + a README.
- Lesson 12 captures the meta-pattern of protocol-first design.
- All files cross-referenced.

### (template for future entries)
```
### YYYY-MM-DD — <short title>
- What changed
- Why
- Any gate met or missed
```

## Decision log

Substantive decisions made along the way. Append below.

### 2026-05-17 — Reframe sync-engine extraction as a wire protocol
- **Choice:** Pursue MSP instead of "extract sync as a Rust library."
- **Why:** Conversation with the project lead. The library framing
  was too small; the spec exercise has architectural value for mxr
  regardless of external adoption.
- **Risk accepted:** Larger scope (months of work, not weeks).
- **Mitigation:** Spike verdict's six-step gating limits the risk
  per step.

### 2026-05-17 — Spec/blog/alignment hosted under `docs/msp/`
- **Choice:** Single directory under `docs/`, not split across
  `extracted-crates/proposals/`.
- **Why:** Multi-month initiative deserves its own canonical home.
- **Side effect:** `docs/extracted-crates/proposals/` no longer
  exists.
