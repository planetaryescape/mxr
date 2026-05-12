# mxr — Implementation journey & maintainer context

This document replaces the phased implementation tree under `docs/implementation/` (removed after v1-era delivery). It captures **why** we built things the way we did, **what shipped**, **what plans were superseded**, and **what remains planned** — not line-by-line build instructions (the codebase and blueprint are authoritative for that).

**Canonical references**

- Product architecture and crate boundaries: `AGENTS.md`, `docs/blueprint/01-architecture.md`, `docs/blueprint/crate-boundary-audit.md`
- Settled decisions: `docs/blueprint/15-decision-log.md`, `docs/blueprint/16-addendum.md`
- HTTP bridge behavior and security: `docs/guides/http-bridge.md`, `site/src/content/docs/reference/bridge.md`
- Release and install: `docs/blueprint/17-release-pipeline.md`, `.github/workflows/release.yml`
- OAuth and credentials: `docs/blueprint/18-addendum-oauth.md`, `docs/blueprint/12-config.md`

---

## 1. Why phased plans existed

Early development used **phase documents** (0 → 4) so agents and contributors could execute dependency-ordered work: each phase ended in something **usable**, not speculative infrastructure. The phases were always **descriptive execution aids**; the **normative** contract for the product is the blueprint + IPC + CLI parity rules in `AGENTS.md`.

**Layout note (important for readers of old notes):** Plans referred to crates as `mxr-core`, `mxr-store`, etc. The repo today uses short directory names (`crates/core`, `crates/store`, …) with the **repo-root `mxr` package** as the single install surface. Internal crates are `publish = false`; shipping is `cargo install --git …`, Homebrew, etc. — not publishing twenty crates to crates.io (that multi-crate publish story in old Phase 4 text is **historical**).

---

## 2. Workspace bootstrap (original “00 — workspace setup”)

**Intent:** One Cargo workspace, shared versions, CI that runs fmt/clippy/test/build on push, and stubs for every crate so the graph compiled before features landed.

**What to remember when changing the repo:**

- **Toolchain:** Pin via `rust-toolchain.toml` so CI and contributors agree on Rust version.
- **CI:** Warnings-as-errors (`-D warnings`) was the bar early; keep CI representative of “mergeable” even if local iteration is looser.
- **Virtual workspace → root binary:** The unified `mxr` binary and `crates/*` boundaries are intentional; duplicating logic outside the daemon for “convenience” violates the architecture.

---

## 3. Phase 0 — prove the architecture

**Goal:** Daemon + TUI + fake provider + SQLite + Tantivy + IPC — end-to-end motion with no real network.

**Thinking:**

- **IPC first:** Length-delimited JSON over a Unix socket scales to multiple clients and scripted automation; the TUI is never “the system.”
- **Two-pool SQLite:** One writer, a small reader pool, WAL — predictable latency under sync + search.
- **Fake provider:** Contract tests and agent development without Gmail; real bugs hide in provider adapters, so Fake is necessary but not sufficient for “done.”
- **Tracing + event log:** Observability starts in Phase 0 so debugging sync/IPC is possible before Gmail complexity arrives.

**Definition of done (conceptual):** Start `mxr daemon`, run TUI against real socket, sync fixtures into DB and search index, navigate mail.

---

## 4. Phase 1 — Gmail read path + search + config

**Goal:** Real account, real delta sync, query language mapped to Tantivy, saved searches, read CLIs (`cat`, `thread`, `headers`, `count`, `saved`), basic status/logs.

**Thinking:**

- **Gmail:** Direct REST + `yup-oauth2` (not a giant generated client) — control and smaller dependency surface.
- **Delta sync:** `history.list` is the economic win for Gmail; design adapters so other providers can do their own delta story.
- **Secrets:** OAuth tokens in keychain / controlled paths, not in `config.toml` (see blueprint config + OAuth addendum).
- **Compile-time SQL:** Phase 1 moved store queries toward `sqlx` checked queries via offline data; maintain that discipline to catch schema drift.

**Superseded checklist item:** Old docs mentioned `mxr search --save`; the real surface is **`mxr saved add` / `mxr saved run`** (and TUI equivalents).

---

## 5. Phase 2 — read/write, compose, IMAP, batch

**Goal:** Primary-client capability: compose ($EDITOR + structured outbound), mutations, snooze, unsubscribe, SMTP + Gmail send, **IMAP first-party**, batch selection and CLI batch operations, reader pipeline for distraction-free display.

**Thinking:**

- **Reader crate:** Shared between TUI, export, and anything that needs “human text” vs raw MIME.
- **Mutations:** Must be **dry-runnable** where batch/destructive; same selection path for preview and commit.
- **IMAP:** Folder vs label semantics stay explicit — do not pretend IMAP is Gmail labels.
- **CLI–TUI parity (D026):** Every TUI action should map to a daemon `Request` and a CLI entry point; half-wiring is considered a broken feature.

---

## 6. Phase 3 — export, rules, polish

**Goal:** Thread export (multiple formats), deterministic rules engine with shell hooks, multi-account polish, performance targets for medium mailboxes, richer observability (`doctor`, events, logs).

**Thinking:**

- **Export:** Formats serve humans (Markdown), tools (JSON), and archivists (mbox) — plus LLM-oriented export that respects reader-cleaned text.
- **Rules:** Data-first, inspectable, replayable; scripts are escape hatches, not the core model.
- **Doctor:** Aggregate “is the system healthy” signals for support and CI-adjacent checks.

---

## 7. Phase 4 — community & release (what matters now)

**Original goal:** Public release, contributor ergonomics, install paths, docs.

**Superseded expectations (do not resurrect blindly):**

| Old plan | What actually won |
|----------|-------------------|
| mdBook under `docs/book/` | **Astro/starlight site** under `site/` (user docs + API explorer consuming OpenAPI) |
| Publish many internal crates to crates.io | **Single binary** install via git + release artifacts + Homebrew; internal crates stay private |
| Four-target musl matrix as mandatory | **Current matrix** in `.github/workflows/release.yml` (truth for what we build) |
| `git-cliff` / commitlint as specified | **Whatever release automation we run today** (don’t treat old appendix as law) |
| “Adapter kit” scaffolding in the long Phase 4 doc | **Principle unchanged:** adapters implement `MailSyncProvider` / `MailSendProvider`; community adapters are welcome but must respect crate boundaries |

**Still legitimately “Phase 4 flavored” (product/marketing, not code):** demo assets, launch checklist, HN post — track separately if desired.

---

## 8. HTTP bridge v0.5

**Why:** Terminal users are core, but **local HTTP** unlocks the web SPA, mobile clients, and agents. The bridge must be a **first-class, reviewable contract**, not an ad-hoc pile of routes.

**Decisions:**

- **Managed bridge:** Runs with `mxr daemon` by default; **`mxr web`** remains for isolation / desktop child-process patterns — **same router codepath**.
- **Versioned API:** `/api/v1/...` so additive evolution is predictable; breaking changes require v2 policy.
- **Security defaults:** Bearer token **even on loopback** (DNS rebinding), host allowlist, strict CORS allowlist — see `docs/guides/http-bridge.md`.
- **OpenAPI:** Serves `/api/v1/openapi.json` + Swagger UI; drives TS codegen (`openapi-typescript`) for apps.
- **Route taxonomy:** Mirrors IPC buckets — `mail` vs `platform` vs `admin`; **client-specific** shaping stays out of the daemon.

**Parity philosophy:** Strive for **every real capability** reachable over HTTP that external clients need. The `Request` enum and HTTP map **1:1-ish**, not mathematically identical (some IPC is internal batching; generic envelopes exist). When adding IPC variants, treat **CLI + TUI + bridge** as the default shipping set unless there is a deliberate exclusion.

**Verification:** OpenAPI conformance / schemathesis in CI (`openapi-conformance.yml`), integration tests in `crates/web`, and periodic diff of routes vs `utoipa` path list.

---

## 9. Hybrid / semantic search (cross-phase)

**Intent:** **Lexical (Tantivy) stays the source of exactness**; semantic is **optional** acceleration layered on locally stored embeddings.

**Rollout wisdom captured in the old hybrid doc:**

1. Stabilize lexical paths with integration tests (sync → store → search/count).
2. Add semantic schema + profile lifecycle in SQLite.
3. Prove behavior with fake/deterministic embeddings before real models.
4. Default English profile + lazy download; heavier/multilingual profiles explicit opt-in.
5. Attachment-derived text is format-specific; **no OCR** for semantic indexing — image-only content simply does not become dense text (product rule in `AGENTS.md`).

**Operator expectations:** Semantic enable/install/reindex/status must fail **open** to lexical search; rebuild-from-SQLite must remain possible after corruption or profile swap.

---

## 10. Addendum coverage (A001–A009)

Phase docs mapped features to addenda; today, use **`docs/blueprint/16-addendum.md`** as the authoritative list. Implementation journey summary:

- **A001–A002:** Compose and markdown semantics — Phase 2.
- **A004:** Full CLI surface — spread across phases (reads → mutations → remaining admin/labels/events).
- **A005:** Vim + Gmail keys — Phase 0–2 layering.
- **A006:** Observability — Phase 0 foundation, Phase 3 completeness.
- **A007:** Batch ops — Phase 2–3.
- **A008:** IMAP first-party — Phase 2.
- **A009 / OAuth:** See `docs/blueprint/18-addendum-oauth.md` (bundled client ID strategy, verification growth path, token storage).

---

## 11. Planned: `mxr accounts reauth` (CLI + parity)

**Problem:** Users hit expired or revoked OAuth, or broken token files. Removing and re-adding the account works but is **heavy-handed** (loss of nuance, scary UX). Status/diagnostics have historically mentioned “reauth” without a dedicated **first-class** CLI entry.

**Goal:** A **single obvious command** (working name: `mxr accounts reauth <account-key>`) that:

1. Identifies the existing configured account (by **config key**, same as `accounts upsert` / `accounts remove`).
2. Triggers the **same** auth flows already used for add: **`AuthorizeAccountConfig`** / **`StartAuthSession`** with `reauthorize: true` where applicable.
3. Preserves account identity in config and store where possible — **refresh credentials**, don’t destroy the account row casually.
4. Supports **`--dry-run`** that validates the account exists and prints what would happen (per mutation preview norms).
5. Has **TUI affordance** and **HTTP bridge** paths (`POST /api/v1/platform/accounts/authorize` already accepts `reauthorize`) so no surface lags.

**Design constraints:**

- **No new OAuth semantics** — reuse yup-oauth2 / session flows; this is UX + orchestration + IPC/CLI wiring.
- **Provider-specific behavior** lives in adapters; the CLI/daemon UX stays provider-agnostic (“re-link this account”).
- **Document remediation** in doctor/status copy should **match** the shipped command once it exists.

**Suggested implementation order:** IPC handler reuse audit → CLI subcommand (thin wrapper) → TUI action → update remediation strings and `AGENTS.md` CLI-first checklist.

---

## 12. Planned: Arch (AUR) and Nix packaging

**Audience:** Arch and Nix users are **explicit targets** — reproducible installs, declarative config, and distro conventions matter.

### 12.1 AUR

**Options:**

- **`mxr-bin`:** PKGBUILD that pulls **official release artifacts** (checksum-verified) from GitHub releases — fastest install, aligns with Homebrew “binary channel.”
- **`mxr-git`:** Builds latest `main` from source — for contributors and bleeding-edge users; depends on Rust toolchain in PKGBUILD.

**Maintainer expectations:**

- Coordinate version bumps with **release tags** (`v*`).
- Reconcile dependencies (OpenSSL, SQLite, etc.) with our actual linkage — prefer **documented** build steps from `README.md` / CI.
- Respect that **internal crates** are not published; AUR builds from the **workspace root** `mxr` package.

**Repo stance:** Packaging files *may* live in-repo (`packaging/aur/`) or in a separate packaging repo — decide based on maintainer preference; CI can validate PKGBUILD syntax optionally.

### 12.2 Nix / NixOS

**Options:**

- **Flake (`flake.nix`):** `packages.<system>.mxr` building from flake inputs; `nix run` / `nix shell` for contributors.
- **`nixpkgs` upstream:** Long-term discoverability; requires a dedicated submitter following nixpkgs hygiene.

**Design notes:**

- Pin Rust via **fenix** or nixpkgs Rust; match `rust-toolchain.toml` to avoid “works on my machine.”
- Expose **tests** (`nix flake check`) that run `cargo test --workspace` or a subset if duration forces splitting.
- Optional: **Home Manager** module for declarative `config.toml` fragments — nice-to-have after core flake works.

**Security:** Nix builds must not embed secrets; OAuth remains runtime per user.

---

## 13. Superseded items (single table)

| Original idea | Resolution |
|---------------|------------|
| Saved-search CLI under `mxr search --save` | `mxr saved add` / `mxr saved run` |
| Docs in mdBook | `site/` Starlight docs |
| crates.io publish of all workspace crates | Git + release binaries + Homebrew |
| Hybrid semantic doc § OCR | **No OCR** policy for semantic indexing |
| Experimental bridge (permissive CORS, loose auth) | v0.5+ managed bridge: `/api/v1`, token + host allowlist + OpenAPI |

---

## 14. When enhancing “history” features — checklist

1. Does the change need a **blueprint/decision-log** amendment? (If it reverses a D-number, document it.)
2. Does **IPC** need a new `Request` variant? If yes, plan **CLI + TUI + bridge** together.
3. Is there a **dry-run** path for batch/danger?
4. Do **tests** cross the real boundary (daemon + store + adapter), not only FakeProvider unit tests?
5. For search/semantic, did you preserve **lexical correctness** and **optional semantic** invariants?
6. For HTTP, did you update **OpenAPI path inventory** and consumer docs under `site/`?

---

*End of implementation journey doc. For step-by-step feature specs going forward, prefer new focused docs under `docs/blueprint/` or `docs/guides/` rather than resurrecting giant phased code dumps.*
