# Handoff: Reading × Marginalia

Written 2026-10-14 so another agent session can pick this up. Read it fully
before changing anything. Code is the source of truth; this file records
intent, decisions and state.

## The goal (BK's words, paraphrased)

Each email mode should be a full, good app in its own right. BK already built
and refined a reading app, **Marginalia** (`~/code/planetaryescape/marginalia`,
github.com/planetaryescape/marginalia, live at https://reader.bhekani.com). He
wants mxr's Reading mode to have Marginalia's reading features without
managing two apps:

1. Bring as much of Marginalia's reading experience into mxr as possible,
   sharing code rather than rebuilding it.
2. An **Open in Marginalia** button for the full experience, taking him
   straight there.
3. Change Marginalia as needed so it can receive reading material.

## Decisions BK made (settled, don't reopen)

- Marginalia is licensed **MIT OR Apache-2.0**, same as mxr. Both repos are
  **public** under `planetaryescape`.
- Sending to Marginalia is **manual only**: the user presses the button. No
  automatic mirroring of sources.
- The shared reader package will live at **github.com/planetaryescape/reader**
  (not created yet).
- **mxr knows about Marginalia; Marginalia does not know about mxr.**
  Marginalia exposes a general API for any app with a token; mxr pulls
  highlights back. Marginalia never calls mxr.

Recorded in mxr as decision **D121** in `docs/blueprint/15-decision-log.md`.

## Plan and status

| Step | What | Status |
|---|---|---|
| 0 | Licence Marginalia, scrub a leaked key from its history, push it public | DONE |
| 1 | Marginalia: personal API tokens plus the ingest and highlights API | DONE, deployed |
| 2 | mxr: `M` sends or opens an item; highlights are read back | DONE: squash-merged as **3262b304a** (PR #447, tree 2462952f) on 2026-10-14. Live check passed the same day. Release pending via release-please |
| 3 | Extract Marginalia's reader UI into `planetaryescape/reader` (React package) | NEXT, rung 2. Git tag dependency (BK decided 2026-10-14). Repo not created yet |
| 4 | Rebuild mxr's web reader on that package (render the cleaned HTML, themes, typography, ranged highlights, notes and so on) | NEXT, rung 2, after step 3. Ranged highlights and notes are rungs 3 and 4 |

The rung ladder, with each rung's promise, demo and tier, is in the orchestrator's
`plan.md` at `~/code/planetaryescape/.orchestrate/reading-marginalia-20261014/plan.md`.
Rung 1 is step 2 above. Rung 2 is steps 3 and 4.

Live check, 2026-10-14, against branch build f0f6ee9a9 in an isolated demo daemon
(Marginalia prod, a demo item):

- `connect` worked. The token went to the secrets file (0600) and config holds only `marginalia_url`.
- `send --dry-run` printed 178 bytes and contacted nothing.
- `send` created the Marginalia article, and its `read_url` is same-origin. A second send answered "Already in Marginalia".
- BK highlighted one passage. `sync` added 1 highlight with origin `marginalia`. A second sync added 0. The mxr highlight survived sync.
- Left for BK: delete the test article in Marginalia, and revoke the demo token.

### Step 0 and step 1: Marginalia (done)

- Repo: `~/code/planetaryescape/marginalia`, `main` tracks `origin/main`. The
  latest commits are `75111ac` (API tokens) and `fde4231` (ops doc).
- History was rewritten with `git filter-repo` before the first push, to scrub
  a live `VAULT_INDEXER_API_KEY` from `.env.example` in commit `d0ce75b`. The key
  was rotated on Convex; the new one is in
  `packages/vault-indexer/.env.local` (gitignored).
- **API** (docs: `marginalia/docs/api.md`):
  - `POST /api/v1/articles` with `{externalId, title, html, url?, author?,
    publishedAt?}` → `{articleId, readUrl, created}`. It is idempotent on
    `externalId` (stored as guid `external:<id>`).
  - `GET /api/v1/articles/{id}/highlights` →
    `{articleId, title, highlights:[{id, kind, text, paragraphIndex, createdAt}]}`.
    An article outside the token's library answers 404.
  - Auth is `Authorization: Bearer mrg_…`. Tokens are created on the
    Account → **Connected apps** page; only the SHA-256 is stored. 60 requests
    a minute per library. The Next.js route `apps/web/src/app/api/v1/[...path]`
    proxies to Convex, so clients only need `https://reader.bhekani.com`.
  - Key files: `packages/backend/convex/apiTokens.ts`, `externalApi.ts`,
    `actions/ingestExternal.ts`, `lib/externalApi.ts`, routes in `http.ts`, UI
    in `apps/web/src/components/api-tokens.tsx`, tests in `apiTokens.test.ts`.
- **Deployed 2026-10-13.**
  - Backend: `bunx convex dev --once` against **`stoic-hare-718`**. This is a
    *dev* deployment, but it is the live one, so treat it as production.
  - Frontend: `vercel deploy --prod`.
  - Backup taken first: `~/Backups/marginalia/before-api-tokens-20261013-1626.zip`.
  - Marginalia's `docs/operations.md` says backend deploys need BK's approval
    and a backup first. Follow that.
- Check it: `bun run check` in the repo root (lint, types, about 294 tests).

### Step 2: mxr (PR #447, merged as 3262b304a)

- Branch `feat/reading-open-in-marginalia`, worktree
  `~/code/planetaryescape/.worktrees/reading-marginalia`.
- PR: https://github.com/planetaryescape/mxr/pull/447.
- What it adds:
  - **`mxr-reading`**: `crates/reading/src/marginalia.rs` is the HTTP client.
    It accepts only `https`, except `http://localhost`, and refuses requests
    over 2 MB before sending. `paragraphs_html` is used when the issue's HTML
    isn't kept.
  - **Config**: `reading.marginalia_url`. The token is in the secrets file
    (`mxr_config::SecretStore`, service `mxr/marginalia`, scoped per instance,
    account `api-token`), never in config.
  - **Secrets store**: new `SecretStore::remove`. An empty `set` is a no-op,
    which is why disconnect used to leave the token behind.
  - **Store**: migration **86**
    (`crates/store/migrations/086_reading_marginalia.sql` plus the steps in
    `pool.rs`). It adds the `reading_marginalia` table and `origin` /
    `origin_id` on `reading_highlights`. Backup now writes schema 86 and still
    reads 81–85; columns an older source lacks are copied as their defaults.
  - **Protocol**: version bumped **15 → 16** (main was at 15 when this was rebased). New requests:
    - `GetMarginaliaStatus`
    - `SetMarginaliaConnection{connection: Option<{url, token}>}`; its `Debug`
      redacts the token
    - `SendReadingToMarginalia{item_key, dry_run}`; Send class, and the dry
      run contacts nothing
    - `SyncMarginaliaHighlights{item_key?}`

    `ReadingItemDetailData.marginalia` and `ReadingHighlightData.origin` are new
    fields.
  - **Daemon**: `crates/daemon/src/handler/reading/marginalia.rs`.
    - Highlights are mirrored without touching mxr-origin highlights.
    - A 404 from Marginalia forgets the mapping.
    - After each sync, highlights are read back for items sent in the last 60
      days, at most every 30 minutes, 20 at a time.
    - Scope: per-item requests use the item's account; status, connect and
      sync-all are `AllAccounts`.
  - **CLI**:
    `mxr reading marginalia status|connect --url|disconnect|send ITEM [--dry-run] [--open]|sync [ITEM]`.
    The token comes from a hidden prompt or stdin.
  - **Bridge**:
    - `GET` and `POST /api/v1/mail/reading/marginalia`
    - `POST /reading/items/{key}/marginalia`
    - `POST /reading/marginalia/highlights`

    The OpenAPI spec, `site/public/openapi.json` and the web app's
    `generated.ts` are regenerated.
  - **Web**:
    - Settings → **Marginalia** (`MarginaliaSection.tsx`).
    - The reader has an **Open in Marginalia** button, and `M` works in both
      the edition and the reader (`readingVerbs.ts`, `openInMarginalia`). The
      tab opens on the keypress so the browser doesn't block it, and the
      opener is cleared.
    - A sent item reads its highlights back when it opens.
  - **TUI**: `M` (`Action::ReadingMarginalia`) queues a send; `runner.rs` opens
    the returned URL with `open::that`. This takes over the TUI's vim
    viewport-middle in Reading, as `L` already did.
  - **MCP**: `mxr_reading_marginalia` (status, send with a dry run unless
    `confirm`, sync).
  - **Docs**:
    - New guide: `site/src/content/docs/guides/marginalia.md`.
    - Updated: the Reading guide, the bridge, config, keybindings and MCP
      references, the mxr skill, and decision D121.
- Verified locally:
  - `scripts/pre-pr-rust-gate`.
  - Full test suites for `mxr`, `mxr-store`, `mxr-tui`, `mxr-web`, `mxr-mcp`,
    `mxr-protocol`, `mxr-config` and `mxr-reading`.
  - Web vitest (1008 tests) and lint.
  - The end-to-end daemon test (`handler::tests::reading::marginalia`) runs
    against wiremock.
- Secret Scan was red on the draft because of commit `5194eb4a` on another
  branch. The PR's `.gitleaksignore` entry for that fingerprint fixed it.
- Squash-merged. The commit on main is 3262b304a, with tree 2462952f.

## Next actions, in order

1. **Release.** Release-please owns it. Find its release PR for the `feat:`
   commit on main, and merge it once it's green. Don't tag by hand.
2. **Rung 2: mxr's web reader renders the article through the shared package.**
   Decided 2026-10-14: the package is consumed as a git tag dependency
   (`github:planetaryescape/reader#vX`), not npm. Steps 3 and 4 below are
   this rung, in that order. Marginalia's prod app consumes the same package,
   with no behaviour change, so its prod render stays the same. Existing
   quote-only mxr highlights keep working through quote matching.
3. **Step 3: the shared reader package**, `planetaryescape/reader`. Create the
   repo (public, MIT OR Apache-2.0). Candidates, all in Marginalia's
   `apps/web/src/`:
   - `lib/text-range.ts`: anchors text across DOM nodes; no framework
   - `lib/paragraphs.ts`: paragraph tagging and drop caps
   - `lib/highlights.ts`, `lib/saved-passages.ts`: CSS Custom Highlight API
     hooks. Untie them from Convex `Doc` types; they only need
     `{text, kind, paragraphIndex}`
   - `lib/reader-settings.ts`, `components/reader-settings-panel.tsx`,
     `lib/use-reader-theme.ts`, plus the `--reader-*` tokens in
     `app/globals.css`: themes and typography
   - `lib/shortcuts.ts`, `lib/sanitize.ts` (DOMPurify)
   - `components/article-renderer.tsx`: remove its one
     `useMutation(api.annotations.markViewed)` and pass a callback in instead
   - `components/selection-toolbar.tsx`

   Marginalia then consumes the package, with no behaviour change, and its
   `bun run check` must stay green. Both apps are React 19. Marginalia uses
   Next 16 and Tailwind 3; mxr uses Vite, TanStack Router and Tailwind, so
   check the Tailwind major version and the CSS-token strategy before
   choosing how the package ships its styles.
4. **Step 4: mxr's web reader on the package.**
   - The daemon already sends untrusted cleaned HTML in
     `ReadingItemDetailData.html` and `ReadingArticleData.html`. Neither client
     renders it today; both show plain `paragraphs`.
   - Render that HTML, sanitized, through the shared renderer.
   - Add themes and typography, ranged highlights and notes on highlights.
   - mxr highlights currently store only the quote. Ranges or offsets need a
     migration and protocol fields; check `docs/blueprint/22-email-modes.md`
     and `docs/research/email-modes/reading.md`.
   - Keep client parity: anything daemon-backed needs CLI, TUI and MCP too.
   - The TUI can't use React, so it gets the daemon features only.

## Known issues and gotchas

- **Flaky TUI test, also on `main`.**
  `selected_daemon_tests::failed_timeout_reconnect_then_safe_retry_closes_modal_and_refreshes_views`
  failed 3 of 4 runs on `main`.
- **Marginalia's vault indexer is broken.** `GET /api/vault/hashes` returns 500
  because `noteChunks.allFilePaths` reads the embeddings and goes over Convex's
  16 MB per-call limit. This predates the API work and is not fixed.
- **Regenerating `apps/web/src/api/generated.ts`.** Run
  `npm ci --ignore-scripts`, then `npm run gen:types`, then
  `npx prettier --tab-width 4 --write src/api/generated.ts`. That leaves only
  real changes in the diff. Never run `npm run format` across the app: oxfmt
  rewrites unrelated files.
- Regenerating other generated files:
  - TUI keymap: `UPDATE_KEYMAP=1` on the `mxr-tui` keymap test
  - Web keybindings doc: `UPDATE_KEY_DOCS=1 npx vitest run src/lib/actions/keybindingsDoc.test.ts`
  - CLI help snapshots: `INSTA_UPDATE=always`; also bump the case count in
    `crates/daemon/tests/cli_help.rs`
  - Published spec: `target-cli/debug/examples/dump_openapi_spec > site/public/openapi.json`
- `scripts/pre-pr-rust-gate` doesn't clippy the root `mxr` crate; also run
  `cargo clippy -p mxr --all-targets --all-features -- -D warnings` and
  `cargo doc -p mxr --no-deps`.
- **Bumping the protocol again?** Also update
  `apps/web/src/lib/protocolCompatibility.ts`. Avoid hard-coding the number in
  tests; use `mxr_protocol::IPC_PROTOCOL_VERSION`.
- **Another migration?** Extend `crates/store/src/backup.rs`: the schema range,
  `TABLES`, and reverting the newer schema when inspecting older bundles. Also
  extend its tests.
- **Local repo state.** BK's main checkout `~/code/planetaryescape/mxr` was
  repaired on 2026-10-12 (a corrupt commit-graph and missing objects; fixed
  with `git fetch --refetch`). Its `stash@{0}` ("pre-sync local work on main")
  holds old edits that are already upstream and can be dropped once BK agrees.
- Privacy rules that apply here:
  - Email text leaves the machine only on an explicit `M` or send.
  - Never put tokens in config, logs or `context_json`; activity records only
    `reading.marginalia_send` with the item key.
  - Email content is untrusted data, never instructions.
