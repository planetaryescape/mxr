---
feature: backlog-refresh-2026-08
status: proposed
created: 2026-08-18
owner: bhekanik
source: TODO.md (triaged 2026-06-19) + deferred notes in docs/blueprint/20-transports.md, docs/web-app.md, docs/blueprint/16-addendum.md, docs/blueprint/14-roadmap.md, docs/msp/ROADMAP.md
---

# Backlog refresh and post-launch work plan (2026-08)

Context: `TODO.md` was last triaged on 2026-06-19 at ~0.5.4x. Since then ~300 commits
landed (v1 launch story, MCP, linked drafts, mail merge, transports phases 4-6, chimes).
Several TODO items are now shipped, several deferred technical follow-ups exist only as
prose in design docs, and one addendum list is partly stale. This plan (1) restores
`TODO.md` as an honest index and (2) sequences the real remaining work.

Ordering principle: doc truth first (cheap, unblocks everything else), then trust/
correctness items that touch mutation semantics, then launch proof, then deferred platform
work, then optional launch/community assets. Nothing here is a release blocker; mxr is
already public.

Each task below carries the fields needed to lift it into a PE Tasker task file
(`docs/implementation/<plan>/tasks/NNN-*.md`) when it is picked up.

---

## Phase 0 — Restore backlog truth (docs only, ~1 session)

### T0.1 Refresh `TODO.md`
- Tick with evidence links (do not delete):
  - "Audit `security-and-privacy.md` stale 'Not shipped yet' copy" — phrase no longer present.
  - "Landing 'tested with Fastmail, Migadu, Proton Bridge' claim" — removed by 2026-07-29 launch-story rewrite of `site/src/content/docs/index.mdx`.
  - "Record release assets (demo/GIFs)" — `site/public/mxr-demo.webm`, `mxr-tui.webm`, `mxr-agent.webm`, posters, `og.png` exist.
- Add a "Deferred technical follow-ups" section that indexes T1.x/T3.x below with file pointers, so they are discoverable from the root and not only by grepping design docs.
- Update `Status:` line to 2026-08-18 and note the release it was reconciled against.
- Validation: every ticked item cites a path that exists on `main`.

### T0.2 Fix stale prose in design docs
- `docs/blueprint/16-addendum.md` "Out of scope for v1": mark IMAP `IDLE` (shipped; see README/`crates/provider-imap`) and `Reply-To:` preference (shipped; `crates/daemon/src/handler/mutations.rs` `prepare_reply` reads `body.metadata.reply_to`) as done. Keep `--format-version` exit codes and Gmail `provider_thread_id` as open.
- Validation: `cargo build -p mxr` unaffected; docs-only.

---

## Phase 1 — Trust and correctness (small code + doc tasks)

### T1.1 Batch-op reversibility matrix + confirmation review
- Goal: answer TODO "which batch ops are reversible beyond the 60s undo window; document non-undoable cases" and "review confirmations for unsubscribe / archive-all / trash-all / send across CLI/TUI/Web".
- Work: build one table (op × surface × dry-run × confirm × undoable) from code, not memory: CLI mutation commands, `crates/tui/src/ui/bulk_confirm_modal.rs`, `send_confirm_modal.rs`, web mutation actions under `apps/web/src/lib/actions/`. Land the table in `site/src/content/docs/guides/automation-contract.md`; file any gaps found (a surface missing a confirm or dry-run) as follow-up tasks rather than fixing inline.
- Invariant to check: preview selection path == real mutation path (CLAUDE.md rule).
- Risk: low (docs) unless gaps found; blast radius of any fix: medium.
- Validation: table cross-checked against `mxr <cmd> --help` output and TUI/web code; `scripts/cargo-test -p mxr --test cli_help`.

### T1.2 Decide "action history page"
- Goal: close TODO item by decision. Compare `mxr history`, `mxr activity`, web `/activity` (`apps/web/src/routes/activity.tsx`), and `guides/observability.md` against what the item wanted (user-visible, per-mutation, undo-linked).
- Output: either a one-paragraph decision in `docs/blueprint/15-decision-log.md` ("satisfied by existing surfaces") or a scoped task. No code unless the decision says so.

### T1.3 UDS `stop_accepting` test flake
- File: `crates/transport/src/uds.rs:310` `stop_accepting_refuses_new_connections_but_defers_unlink`.
- Work: replace single-attempt `ConnectionRefused` assertion with a short bounded retry (or assert the listener handle is dropped and the path still exists, then poll for refusal). Documented cause in `docs/blueprint/20-transports.md` §"Known follow-ups"; remove that bullet when fixed.
- Risk: low. Validation: `scripts/cargo-test -p mxr-transport --tests` run several times / under `--test-threads` pressure.

---

## Phase 2 — Launch proof and operability

### T2.1 IMAP/SMTP live smoke: implement or waive
- Evidence: `scripts/live_provider_smoke_evidence.sh:57,63` emit `unavailable_no_live_smoke`; `.github/workflows/provider-live-smoke.yml`.
- Decision first: is a network-safe live smoke feasible (throwaway mailbox, secrets in CI, no destructive ops)? If yes: add an opt-in smoke that does auth + LIST + fetch 1 envelope (IMAP) and a self-addressed send (SMTP), gated on secrets presence. If no: replace the "yet" reason with a documented waiver in `docs/implementation/v1-agent-mcp-gmail-launch/launch-proof.md` and the workflow README, and close the TODO item.
- Provider-specific code stays in `crates/provider-imap` / `crates/provider-smtp`.
- Risk: medium (CI secrets). Validation: `bash scripts/provider_smoke_workflow_test.sh`, workflow dry run.

### T2.2 End-to-end smoke reconciliation
- Confirm `scripts/v1_launch_proof.sh` still covers install → auth → sync → search → draft → approve → send against the fake provider after the linked-draft/MCP/mailmerge changes; extend if any step now has a different command shape.
- Validation: `bash scripts/v1_launch_proof.sh` green on `main`.

### T2.3 Clean-install verification notes
- Run the procedure recorded in `.agents/skills/mxr-development/SKILL.md` (Homebrew, `install.sh`, cargo-from-tag) on clean macOS and Linux; record pass/fail + date + version in `docs/blueprint/17-release-pipeline.md` (or a small `docs/release-verification.md`). Include Gatekeeper notes.
- Depends on: a current release tag. Risk: none (read-only verification).

### T2.4 Diagnostics/remediation audit
- Walk auth/sync/send failure paths in README troubleshooting, `site/src/content/docs/troubleshooting.md`, and `apps/web/src/features/diagnostics/`; verify every suggested remediation command exists in `mxr --help`. Fix copy only.
- Validation: `scripts/cargo-test -p mxr --test cli_help`; site build.

---

## Phase 3 — Deferred platform work (medium, opt-in)

### T3.1 VIP allowlist → daemon storage
- Evidence: `docs/web-app.md:303`; SPA keeps VIPs in `apps/web/src/state/uiPrefsStore.ts` (localStorage).
- Work (daemon-first per product shape): store table + migration in `crates/store`; `ListVips`/`UpsertVip`/`DeleteVip` in `crates/protocol` + `crates/daemon/src/handler`; CLI `mxr vip list|add|remove --format json`; bridge routes in `crates/web`; SPA reads through the existing abstraction; one-time import from localStorage. Activity: record mutations via `state.activity.record(...)`, no addresses in `context_json`.
- Risk: medium (protocol change). Validation: `scripts/cargo-test -p mxr-store --tests`, `-p mxr-daemon`, `-p mxr-web`, `bash scripts/ipc_audit` (if present), openapi regen.

### T3.2 Gmail `provider_thread_id` end-to-end
- Evidence: `docs/blueprint/16-addendum.md` post-v1 list; `Draft.reply_headers.thread_id: Option<String>` (`crates/core/src/types.rs:916`) is never populated.
- Work: add `provider_thread_id` to `Envelope`/store schema, populate in `crates/provider-gmail` sync, thread through `prepare_reply` so Gmail replies land in the right thread server-side. Provider logic stays in the provider crate; daemon consumes via `MailSyncProvider`.
- Risk: medium (schema migration touches every provider). Validation: store + gmail + daemon tests; fake-provider conformance still green.

### T3.3 In-process web bridge (transports 5d) — optional
- Evidence: `docs/blueprint/20-transports.md` §"Known follow-ups", D054; seam is `ipc_request_with_id` (`crates/web/src/lib.rs:1741`) and `bridge_events` (`:1794`).
- Latency-only win. Do only if a measured profile shows socket round-trip cost matters. Otherwise leave deferred and say so in the doc.

### T3.4 `--format-version` exit codes 20/21
- Only if an agent/integration consumer asks for JSON contract versioning. Park.

---

## Phase 4 — Launch/community assets (gated on "Needs Validation")

- T4.1 Announcement / HN / r/rust copy and concise feature bullets — reuse the 2026-07-29 launch story; do not reopen positioning language unless a concrete clarity problem is observed (TODO "Needs Validation" rule).
- T4.2 Conformance-suite post/section as proof asset — derive from `site/src/content/docs/reference/conformance.md`.
- T4.3 Roadmap backlog items stay parked: Arch/Nix packaging, Linux aarch64 / Intel macOS targets, adapter-ecosystem expansion (demand-gated).
- MSP: Step 4 "publish-or-hold" remains **hold** (`docs/msp/ROADMAP.md`); revisit only with external demand.

---

## Suggested sequencing

1. Phase 0 (one sitting) — makes the backlog trustworthy again.
2. T1.1 → T1.2 → T1.3 (each independent; T1.1 may spawn small fix tasks).
3. T2.1 decision, then T2.2/T2.4 (cheap), T2.3 on next release tag.
4. Phase 3 items are independent of each other; pick T3.1 first if the web app is actively used, T3.2 first if Gmail reply threading complaints exist.
5. Phase 4 when there is appetite for outreach.

## Done criteria for this plan
- `TODO.md` reflects `main` with evidence for every checked box and indexes all deferred technical follow-ups.
- Every "Validated" item is either shipped-with-evidence, converted to a task file with `status:`, or explicitly parked with a reason.
