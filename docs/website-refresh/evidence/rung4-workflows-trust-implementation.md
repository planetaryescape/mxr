# Rung 4 workflow and trust implementation

Checked 2026-10-23. This implementation started at parent `505c4aaadf22d53169354bdcd34a6cbc80665774`. A fresh HTTPS fetch confirmed `main` at `16b1adaed540bd0fdd9e2d447ecd6f6711abcf60` and `merge-base(parent, main)` at that same SHA. During this work GitHub published `v0.6.159` at `e98f499f8bf9072077a693e3e72d23528fe24fa2`; that tag is an ancestor of the fetched `main`. The retained S2 and S3 observations remain separate synthetic `v0.6.156` runs.

## What changed

- Added an ordered task chooser to `site/src/content/docs/examples.md` and a bounded one-account analysis recipe to `site/src/content/docs/guides/recipes.md`. The recipe shows the count `{query,count}`, search `.results`, and subscription-array shapes. It labels the 55-message counts, one search result, and four subscription rows per account as retained `v0.6.156` synthetic observations, not a current replay.
- Corrected subscription language in `site/src/content/docs/guides/analytics.md`: `opened_count` represents local READ flags, and ranking applies to the bounded result set. The recipe keeps the same explicit account selector across count, search, and subscriptions.
- Reworked the architecture diagram and description in `site/src/content/docs/guides/architecture.md` to show SQLite as canonical state, Tantivy as rebuildable search, and provider I/O as a separate adapter boundary beneath the daemon.
- Tightened `site/src/content/docs/guides/automation-contract.md` and `site/src/content/docs/guides/for-agents.md` around command-specific preview support, account selectors, and the distinction between identified MCP/agent IPC requests and ordinary CLI requests.
- Corrected activity-log scope and privacy details in `site/src/content/docs/guides/activity-log.md`: selected successful requests, local working history, configurable metadata, paused/coalesced rows, and bounded-queue drops.
- Separated external agent models from mxr-native features and the built-in assistant in `site/src/content/docs/guides/llm-features.md`, `site/src/content/docs/guides/for-agents.md`, `site/src/content/docs/guides/glossary.md`, and existing homepage trust/composition copy in `site/src/content/docs/index.mdx`. Narrowed planned `fast`/`smart` wording to model tiers; deterministic mode placement remains current. Kept feature-specific cloud recipients and background-call details in the LLM guide.
- Updated the agent and native-model entries in `site/public/llms.txt`.

## Source checks and boundaries

The current count renderer emits `{query,count}` at `crates/daemon/src/commands/count.rs#L66-L80` on `16b1ada`. Subscription handling resolves the optional account, requests the bounded list, sorts that returned list, and emits a JSON array at `crates/daemon/src/commands/subscriptions.rs#L95-L127`. The store filters and groups eligible sender rows by account, counts messages carrying the READ flag, and applies the SQL limit at `crates/store/src/message.rs#L1093-L1168`. The 200-row default and `--account`/`--rank`/`--format` flags are in the current generated CLI pages. No command was run against a mailbox.

The external agent/MCP gates are applied by IPC `ClientKind` at `crates/daemon/src/handler/mod.rs#L2331-L2398`; ordinary CLI origin returns outside those gates. Native LLM defaults and feature overrides are in `crates/config/src/types.rs#L384-L447`, and model-ID routing is in `crates/llm/src/route.rs#L127-L175`. Post-sync classification includes deterministic rule, record, delivery, and assessment work at `crates/daemon/src/loops.rs#L1192-L1252`. Activity recording uses a bounded `try_send`, can drop on backpressure, supports pause, and coalesces selected rows at `crates/daemon/src/activity/mod.rs#L98-L161`; default metadata filtering is in `crates/daemon/src/activity/privacy.rs#L9-L50`.

The current generated CLI help still calls subscription rank an “open-rate” and a “newsletter ROI” in `site/src/content/docs/reference/cli/subscriptions.md`; the current data is a local READ flag count. This source/generator wording was left untouched because generated CLI references and their Rust help inputs are outside this task's ownership. `site/src/content/docs/reference/json-output.md` already states that `opened_count` counts local READ flags. No generated reference pages were edited.

No invoice recipe or new runtime proof was added. Existing S2 local-query observations and S3 shared-draft/refusal observations were not combined or given public IDs. No mailbox, model, account, provider, or daemon command was used.

## Validation

- `git diff --check` passed.
- Layer A inspection of all edited prose files reported zero suspicious Unicode characters; no text cleaning was needed.
- `npm --prefix site run validate` passed after the final prose edits: `265 OpenAPI paths`.
- The Node-only website build (`PATH=/opt/homebrew/opt/node/bin:/usr/bin:/bin npm --prefix site run build`) passed once: 10 docs tests, 265 OpenAPI paths, and 209 pages. That build used the committed OpenAPI artifact because Cargo was absent from PATH. Astro also reported the existing >500 kB chunk warning and an empty `i18n` collection warning. The build completed before the final activity-queue sentence and `llms.txt` model-guide entry were added. The final docs validator and `git diff --check` passed after those edits. A rebuild was queued as `agent-work` task `253`; after the three-minute memory check the wrapper still had no npm build child, so I cancelled only that task. `memory_pressure` reported 53% system-wide free memory at the check.
- The built-site route checks returned 200 for the examples page's recipe and automation links and confirmed the new recipe anchors. The T3 snapshot call timed out twice (`Preview automation snapshot timed out after 15000ms`); `preview_evaluate` then read the built homepage, recipe, agent, and LLM guide. Resizing the preview to 390×844 still measured 1280×800, so this is desktop browser evidence only.
- The built pages opened at `http://localhost:4321/` and `/examples/` in Dia. Preview was served by this worktree's Astro PID `89612`; I stopped that PID and verified port `4321` has no listener.

The remaining doc consistency issue is generated CLI help wording for subscription ranking. Root owns any follow-up to the CLI help source and regeneration.


## Root integration closes the remaining wording and build gaps

Root read the whole candidate and source map at fresh main `16b1adaed540bd0fdd9e2d447ecd6f6711abcf60`. GitHub release metadata confirms v0.6.159 published from `e98f499f8bf9072077a693e3e72d23528fe24fa2`. S2 and S3 retain their original version and separate-account boundaries; no new mailbox or model replay was run.

Root restored the direct daemon-reuse explanation on the homepage, linked shared-draft review to the agent tutorial, replaced the remaining “who you never open” example with READ-state wording, and made queue drops explicit. The architecture diagram now shows provider adapters as their own daemon branch. The glossary no longer says every AI feature is read-only: the assistant can apply tools under its approval rules. The agent guide no longer calls MCP local-only: source `crates/mcp/src/lib.rs#L385` and `crates/daemon/src/commands/connect.rs` confirm selected paired HTTPS uses a registration grant. Native model wording follows `crates/config/src/types.rs#L475-L485`: a feature can override base `enabled=false`, and recognized hosted model IDs route independently of `base_url`.

The final root build passed 10 script tests, 265-path docs validation and 209 Astro pages, with 205 Markdown siblings. It used the committed OpenAPI fallback, not live conformance. Normal macOS memory pressure was observed (`kern.memorystatus_vm_pressure_level=1`, 54% free), so the earlier worker memory wait did not recur. Root jobs completed normally; no unrelated job was cancelled.

A compiled-HTML check passed 2,317 internal route/anchor occurrences across the 11 changed authored pages, including shared navigation. The local browser followed Examples → the account-scoped recipe; its selector, JSON envelopes, current READ-state limits and shared-draft tutorial link rendered. The LLM guide rendered separate external reasoning and per-feature enabled behavior; activity, security, agent, architecture and automation pages returned 200. The actual viewport remained 1280px and matched the document width. The real preview opened in Dia.

Static 390px compiled-CSS captures of the LLM guide in both themes loaded Recursive, measured document width 390px and found no overflowing code blocks. These are static phone layouts, not interactive phone navigation. Capture-only favicon/touch-icon requests were unresolved; production assets were not changed. The existing worker and root npm/dist artifacts remain retained. Root preview PID 68160 on port 4323 remains owned until the online handoff.

Inline simplifier review introduced no helper or executable logic; MDX changes are prose using existing structures, with no TypeScript contract change. Anti-slop JS/TS lint is not applicable. The new task chooser uses ordinary content links rather than bold-label teasers. Whitespace and suspicious-Unicode checks passed. No independent review was run after BK waived it. The old generated subscription-help terminology remains a recorded discovery follow-up for rung 5; reference output was not edited by hand.
