# Plan 014: Preserve unknown send outcomes until they are resolved

> Executor: this is an implementation handoff, not evidence that the fix or its tests already ran. Follow the steps and gates in order. The coordinating reviewer owns plans/README.md. Do not edit that index, push, or open a PR unless separately dispatched to do so.

## Status

- Priority: P1
- Effort: L, several days including provider and client coverage
- Risk: HIGH, duplicate delivery and permanently blocked drafts
- Depends on: plans/011-safe-verification-and-ci.md
- Category: bug
- Planned at: fetched origin/main 9160b1a12ef9dc5d3fb5513b45c68fe57183074f, 2026-09-04
- Source inspected: /tmp/mxr-polish-audit-9160b1a1
- Plan destination: /Users/bhekanik/code/planetaryescape/mxr/plans/014-send-outcome-recovery.md
- Serialize with plans 013, 016 and 023. Review 013's compose/draft helper changes before modifying send entry points; 016 and 023 also edit daemon mutation/startup code. Allocate the next free migration number at integration time, never assume another plan left a specific number available.

## Why this matters

An error returned from send does not prove that delivery failed. A provider can accept the message before its acknowledgment is lost. Today mxr turns every send error into an ordinary draft that can be sent again. Preserve the difference between definitely unsent, accepted, and unknown, including after daemon restart and scheduled delivery.

These are static findings. No duplicate delivery was reproduced during the audit. A stable Message-ID helps reconciliation; it does not guarantee that Gmail or an SMTP server deduplicates submissions. RFC 5321 section 4.5.3.2.6 describes the lost-acknowledgment case: https://www.rfc-editor.org/rfc/rfc5321.html#section-4.5.3.2.6.

## Current state

- crates/daemon/src/handler/mutations.rs:2783 claims a stored draft with a compare-and-set before invoking the provider.
- The same file:2844-2851 resets every provider error to Draft:

~~~rust
let receipt = match sender.send(&draft, &from, &rfc2822_message_id).await {
    Ok(r) => r,
    Err(e) => {
        let _ = state
            .store
            .update_draft_status(draft_id, DraftStatus::Draft)
            .await;
        return Err(crate::handler::HandlerError::Message(e.to_string()));
    }
};
~~~

- mutations.rs:2856-2903 performs local ingest, marks Sent, then persists the receipt in separate operations. The receipt lookup at :2739 protects only attempts whose receipt was persisted.
- crates/provider-smtp/src/lib.rs:214-237 treats a timeout as retryable RateLimited without preserving the SMTP phase.
- crates/daemon/src/server.rs:1750-1773 resets Sending drafts older than an hour without evidence about delivery.
- crates/daemon/src/loops.rs:1838-1846 clears send_at and records an attempt transactionally. Keep this at-most-once scheduling contract. It currently records all ordinary errors as failed at :1871-1872.
- crates/core/src/types.rs:1790 has Draft, Sending, Sent. The core MailSendProvider trait at crates/core/src/provider.rs:139 returns Result<SendReceipt>.
- crates/store/migrations/043_sent_draft_receipts.sql preserves successful receipts after the draft row is deleted. Migration 046_scheduled_send_attempts.sql records scheduled attempt outcomes. Extend these conventions with a new migration; do not edit old migrations.
- Existing test exemplar: crates/daemon/src/handler/tests/platform_and_export.rs:437, dispatch_send_stored_draft_replay_returns_original_receipt_without_resending. It asserts one provider call after a successful replay.

The project requires provider logic in adapters, daemon-owned durable state, scriptable CLI, and the same capability in TUI, web, and MCP. Recovery mutations need a preview before applying the selected resolution. Activity remains local and must exclude message bodies, credentials, and raw provider responses.

## Lessons applied from Obsidian

The read-only notes were context, not proof of current code. "Retrying a Permanent Error Is Data Loss" argues for distinguishing retryable failures from failures repetition cannot repair. For sending, add a third question: is acceptance unknown? "Nonblocking Work Needs a Visible Home" means unresolved delivery must remain visible beside its draft. "Provider Lanes Need Account Scope" means provider reconciliation shares the account lane without blocking other accounts. "Zero Change Can Be Success" means a resolution that finds an existing receipt succeeds without sending anything.

## Scope

Only modify these paths as needed for this send lifecycle:

- crates/core/src/error.rs, crates/core/src/types.rs, crates/core/src/provider.rs
- crates/provider-smtp/src/lib.rs
- crates/provider-gmail/src/provider.rs, crates/provider-gmail/src/client.rs, crates/provider-gmail/src/error.rs
- crates/provider-fake/src/lib.rs and crates/provider-outlook/src/lib.rs only for trait/error compatibility
- crates/store/src/draft.rs, crates/store/src/draft_recovery.rs, crates/store/src/scheduled_sends.rs, crates/store/src/lib.rs
- One new migration in crates/store/migrations/ using the next free sequence number, and generated .sqlx/*.json files only for changed checked queries
- crates/daemon/src/handler/mutations.rs, crates/daemon/src/handler/mod.rs, crates/daemon/src/handler/error.rs
- crates/daemon/src/server.rs, crates/daemon/src/loops.rs
- crates/daemon/src/cli/mod.rs, crates/daemon/src/commands/mutations/mod.rs, crates/daemon/src/commands/mutations/compose.rs, crates/daemon/src/commands/draft.rs, crates/daemon/src/main.rs
- crates/protocol/src/types.rs
- crates/tui/src/ui/drafts_modal.rs, crates/tui/src/runner.rs, crates/tui/src/app/state/compose.rs, crates/tui/src/app/mod.rs
- crates/web/src/routes_v6.rs, crates/web/src/request_types.rs, crates/web/src/lib.rs, crates/web/src/snapshots/mxr_web__tests__openapi_spec_summary.snap
- apps/web/src/features/drafts/api.ts, apps/web/src/features/drafts/DraftsRoute.tsx, apps/web/src/features/drafts/DraftsRoute.test.tsx, apps/web/src/api/generated.ts
- crates/mcp/src/lib.rs
- crates/daemon/src/handler/tests/platform_and_export.rs, crates/daemon/tests/cli_journey.rs, crates/daemon/tests/cli_help.rs, draft-related snapshots under crates/daemon/tests/snapshots/
- site/src/content/docs/guides/crash-safe-drafts.md, site/src/content/docs/reference/mcp.md

Out of scope: a general outbound queue, automatic resend of unresolved attempts, delivery tracking/read receipts, new AI checks, provider protocol replacements, or modifications to real mail. If a named client file moved, find its replacement, record the path correction, and ask the coordinator to amend the scope before editing it.

## Git and verification setup

Work in an isolated implementation worktree on codex/014-send-outcome-recovery. Fetch origin main first. Run this drift check there, expanding the Scope list when reviewing the diff:

~~~sh
git fetch origin main
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/core crates/provider-smtp crates/provider-gmail crates/provider-fake crates/provider-outlook crates/store crates/daemon crates/protocol crates/tui crates/web crates/mcp apps/web/src/features/drafts apps/web/src/api/generated.ts .sqlx site/src/content/docs/guides/crash-safe-drafts.md site/src/content/docs/reference/mcp.md
~~~

Expected: no unexplained changes. Compare the excerpts above with the fetched implementation ref. Incorporate explicitly completed predecessor work; stop on an unexplained changed contract. Do not switch/reset/clean the user's primary checkout. Commit logical units as type: description, user author only, no attribution footer.

All commands below are FUTURE gates, not checks run during plan writing. Complete 011 before using scripts/cargo-test. Use its isolated MXR_* test profile and owned temporary paths. Never replace HOME or CODEX_HOME and never kill unrelated processes. Unit tests use in-memory stores or test-owned files and fake/Wiremock providers.

## Steps

### 1. Characterize the missing outcomes

Add tests prefixed plan014_ in the existing daemon delivery test file. Extend its fake sender so a test can record acceptance and then return a transport error. Add barriers to pause immediately before provider invocation, after acceptance, and before receipt persistence. Do not use real providers or wall-clock sleeps.

Cover acceptance followed by error, pre-send validation/configuration failure, two simultaneous send requests, restart after acceptance, and scheduled-send ambiguity. Keep the successful-replay exemplar passing.

Future gate: scripts/cargo-test -p mxr --lib plan014_ -- --nocapture. This is the deliberate RED step: the named ambiguity/restart assertions should fail on the current implementation, with no build/setup failures. Capture the actual failing assertions. If they pass, verify the test reaches the real send path before changing code.

### 2. Persist the attempt and its transitions atomically

Add a durable attempt record keyed by draft and unique attempt ID. Record account, stable Message-ID, draft revision/content identity, phase, timestamps, and minimal error category. Never persist full provider response bodies. Use explicit phases for prepared, in-flight/unknown, accepted, and definitely-not-sent; keep public naming consistent across clients. Store the attempt claim and Draft-to-Sending CAS in one transaction. Editing, resetting, and sending the same draft must check the same revision/attempt.

Create a store operation that atomically persists provider acceptance evidence and the Sent state. Receipt fields needed before local ingest may require an acceptance record separate from the existing final receipt. After acceptance, local ingest or search failure must leave durable accepted evidence and a repairable local step, never a resendable draft. Make final receipt persistence and the relevant local completion state transactional; network calls and Tantivy writes cannot be placed inside a SQLite transaction.

Migrate old Sending rows conservatively to unknown unless a persisted receipt proves acceptance. Preserve ordinary drafts, existing receipt replay, provider links, and scheduled attempts. Never infer non-delivery from age. Audit every existing recovery/reset writer so it cannot bypass the attempt CAS.

Future gate: scripts/cargo-test -p mxr-store --lib plan014_ -- --nocapture. Expected: at least six named transition/migration/atomicity tests pass, including reopen of a file-backed store and transaction rollback on injected failure.

### 3. Classify adapter failures without inventing certainty

At the provider boundary, return structured evidence of definite non-submission only when known, such as attachment/build failure before transport, or an explicit rejection proving non-acceptance. Preserve permanent versus transient classification separately. Treat a timeout or disconnected response after a submission may have started as unknown. If lettre cannot expose the required phase, choose conservative unknown; do not parse error prose to claim certainty.

Keep the MailSendProvider boundary provider-agnostic. Prefer additive core error/outcome types over changing every unrelated provider API. An optional adapter-owned lookup may reconcile accepted mail by account and stable Message-ID. Positive evidence may resolve acceptance; an absent search result, unsupported lookup, or lookup timeout leaves unknown. No remote deduplication promise is allowed.

Future gates: scripts/cargo-test -p mxr-provider-smtp --lib plan014_ -- --nocapture and scripts/cargo-test -p mxr-provider-gmail --lib plan014_ -- --nocapture. Expected: named rejection, timeout, lost-response, and successful-acceptance cases pass in each relevant adapter; at least three tests per changed adapter. Keep unrelated adapter tests passing.

### 4. Integrate normal sends, startup, and scheduled sends

Change send_stored_draft to advance through the persisted attempt operations. A definitely-unsent failure can return to editable Draft with its reason. An unknown outcome remains blocked from another provider send. A known accepted attempt replays its receipt or repairs local ingest. No branch may silently discard failure to persist delivery state.

Replace age-based automatic reset at startup with attempt reconciliation. Reuse the existing account provider lane for lookups, with a bounded timeout; release it before local follow-up work. Scheduled sends still clear their schedule transactionally before submission and now persist accepted/unknown/definite-failure outcomes from the same pipeline. An unknown scheduled attempt remains unresolved across repeated restarts rather than being converted to a completed interruption marker. Stopping the daemon mid-send must preserve its attempt even if the task is cancelled.

Future gate: scripts/cargo-test -p mxr --lib plan014_ -- --nocapture. Expected: all step 1 cases now pass, including zero additional provider calls after unknown outcome, restart, and accepted-but-local-ingest-failed replay.

### 5. Expose the minimum recovery contract everywhere

Extend existing draft read/list/recovery responses with structured delivery state and last attempt identity, using backward-compatible defaulted optional fields where possible. If dedicated status/resolution requests are needed, keep them limited to this draft lifecycle. Resolution has preview and apply using the same draft/attempt selection. It either records confirmed accepted evidence or explicitly acknowledges uncertainty and returns the draft to editable state. Resolution must never itself submit mail; a subsequent send still uses existing send confirmation and safety gates.

CLI JSON and text output, the TUI draft list, the web draft list, and MCP must distinguish sending, unknown, accepted with pending local repair, and definitely unsent. Keep the warning persistent on the draft. Expose the same preview/CAS resolution through all clients. MCP resolution retains agent permission and confirm requirements; a read request cannot resolve an attempt. Remove the stale reference to a nonexistent drafts resolve command unless this plan actually adds that exact command and documents it.

Add plan014_ protocol/daemon/MCP tests and focused TUI rendering tests. Add a web test that verifies unknown state is visible and its ordinary send control cannot bypass daemon resolution.

Future gates: scripts/cargo-test -p mxr-protocol -p mxr-mcp -p mxr-tui -p mxr-web --lib plan014_ -- --nocapture; npm --prefix apps/web run test -- src/features/drafts/DraftsRoute.test.tsx; npm --prefix apps/web run typecheck. Expected: nonzero matching tests in each changed Rust client crate, web tests pass, and typecheck exits 0. Regenerate bridge types using npm --prefix apps/web run gen:types. Its checked-in script invokes cargo run --quiet --example dump_openapi_spec -p mxr-web and openapi-typescript; it does not require a live bridge. Use the isolated build environment from 011.

### 6. Verify migrations and the whole send workflow

Follow the checked-in SQLx workflow at .github/workflows/ci.yml:361-365 with a new temporary file, never the user's database:

~~~sh
send_audit_sqlx_dir="$(mktemp -d /tmp/mxr-plan014-sqlx.XXXXXX)"
cargo run -p mxr-store --example create_db -- "$send_audit_sqlx_dir/check.db"
DATABASE_URL="sqlite:$send_audit_sqlx_dir/check.db" cargo sqlx prepare --workspace
DATABASE_URL="sqlite:$send_audit_sqlx_dir/check.db" cargo sqlx prepare --check --workspace
scripts/cargo-test -p mxr --lib dispatch_send_stored_draft_replay_returns_original_receipt_without_resending -- --nocapture
scripts/cargo-test -p mxr --test cli_help
scripts/cargo-test -p mxr --test cli_journey
cargo build -p mxr
git diff --check
~~~

Expected: all exit 0; replay test reports one passed; help and journey suites pass against the isolated fake profile. Changed SQLx cache entries correspond only to in-scope queries. Inspect regenerated help/OpenAPI diffs. The isolated CLI journey must preview and resolve an unknown attempt, then require a separate normal send action. Keep transport logs free of body/credential contents.

## Regression matrix

| Situation | Required outcome |
|---|---|
| Validation, attachment load, or configured sender unavailable before submit | Definitely unsent, editable, zero provider submissions |
| Explicit permanent rejection | No automatic retry; actionable definite-failure state |
| Accepted then acknowledgment lost | Unknown, ordinary resend blocked, original attempt preserved |
| Accepted then local store/index failure | Accepted evidence durable, repair/replay does not submit again |
| Simultaneous sends or resolution racing send/edit | Exactly one matching CAS succeeds |
| Daemon restart before submit/after submit/after acceptance persistence | Conservative state, no unknown attempt auto-resubmitted |
| Scheduled attempt unknown across two restarts | Schedule stays cleared, unresolved outcome stays visible |
| Provider lookup absent, unsupported, rate-limited, or timed out | Remains unknown; another account stays usable |
| Existing receipt, including zero local changes on replay | Same receipt, success, no new provider call |
| Migration of legacy sending/sent rows | No invented acceptance; no lost draft or known receipt |

## Done criteria and stop conditions

- Every future gate passes with the expected nonzero test count; the new failure cases were seen red first.
- No unknown attempt becomes resendable solely due to timeout, age, or restart.
- Accepted evidence, status, and final receipt/local completion use the documented transactional boundaries.
- Preview and apply identify the same draft revision and attempt; all clients expose the state and resolution.
- Only Scope paths changed. The coordinating reviewer receives actual command results and owns the index update.

Stop and report if a transport cannot distinguish a proposed definite failure, a migration would guess delivery, the preview can race an unchecked edit, the client protocol change requires unlisted files, or two reasonable attempts fail the same gate. A missing lookup is not permission to resend. Do not weaken safety or silently add automatic retry to get tests passing.

Maintenance: reviewers should inspect every transition out of Sending/unknown, every post-acceptance error path, and replay after a partial local write. Keep the send attempt model separate from delivery/read tracking.
