# mxr TODO

Status: refreshed on 2026-08-24 against `v0.6.26` / `main`.

This is a backlog index, not a sprint plan. Root TODO items stay here only when
they are cross-cutting or not yet scoped. Once work is validated, move it into a
scoped implementation task with `status:` frontmatter and explicit
owner/executor/milestone context.

Status labels:

- `Validated` means current code/docs show the gap still exists.
- `Needs validation` means positioning, market, or site work that should not be
  built until the problem is tested.
- `Stale / archived` means the old item appears shipped or contradicted by
  current code/docs; evidence is linked inline.

## Deferred Follow-ups Index

Tracked plan: `docs/implementation/backlog-refresh-2026-08/plan.md`.
Corrected assumptions: `docs/implementation/backlog-refresh-2026-08/build-log.md`.

- T1.1: batch-op reversibility matrix and confirmation review.
- T1.2: decide whether existing history/activity surfaces satisfy the action-history item.
- T1.3: fix the UDS `stop_accepting` test flake.
- T2.1: add or waive network-safe IMAP/SMTP live smoke; current script still emits `unavailable_no_live_smoke`.
- T2.2: reconcile what `scripts/v1_launch_proof.sh` proves; it covers fake-provider behavior, not installation or real auth.
- T2.3: clean-install verification for Homebrew, `install.sh`, tagged Cargo, and Gatekeeper notes.
- T2.4: diagnostics/remediation copy audit against shipped CLI/web commands.
- T3.1: VIPs remain browser-local; `docs/web-app.md` overstates the missing `useVips()` abstraction.
- T3.2: Gmail native reply threading appears already preserved by provider lookup/cache; reconcile stale docs before proposing schema work.
- T3.3/T3.4: in-process web bridge and `--format-version` exit codes remain evidence-gated.

## Validated

### Product and Docs Truth

- [ ] Make public pages explicit about what is shipped vs roadmap anywhere they imply a fully permissioned agent sandbox. Evidence: `site/src/content/docs/guides/for-agents.md` has current limits; `site/src/content/docs/guides/security-and-privacy.md` should stay aligned with those limits.

### Core Operability

- [ ] Reconcile end-to-end smoke coverage for install -> auth -> sync -> search -> draft -> approve -> send. Keep `scripts/v1_launch_proof.sh` as the deterministic fake-provider gate; add live-provider proof only where network-safe. Evidence: `docs/implementation/v1-agent-mcp-gmail-launch/launch-proof.md`, `.github/workflows/provider-live-smoke.yml`.
- [ ] Add network-safe IMAP and SMTP live smoke tests or document why the current `unavailable_no_live_smoke` result is acceptable for launch. Evidence: `scripts/live_provider_smoke_evidence.sh`.
- [ ] Keep diagnostics honest for auth/sync/send failures; verify user-facing remediation paths match shipped commands. Evidence: README setup failure path, `site/src/content/docs/troubleshooting.md`, and diagnostics surfaces under `apps/web/src/features/diagnostics/`.
- [ ] Harden export flows for agent use only where gaps are found against `mxr export` markdown/json/mbox/llm. Evidence: `site/src/content/docs/guides/for-agents.md` uses export; `crates/export/` owns formats.

### Trust and Bulk Actions

- [ ] Define which batch ops can be reversible beyond the current 60s undo window; document non-undoable cases. Evidence: `site/src/content/docs/guides/automation-contract.md`.
- [ ] Review confirmations for unsubscribe, archive-all, trash-all, and send across CLI/TUI/Web before adding new mutation features. Evidence: `site/src/content/docs/guides/automation-contract.md`, `crates/tui/src/ui/send_confirm_modal.rs`, and mutation docs.
- [ ] Decide whether "user-visible action history page" is already satisfied by `mxr history`, `mxr activity`, web `/activity`, and observability docs, or whether a new first-class page is needed. Evidence: `site/src/content/docs/guides/observability.md`, `apps/web/src/routes/activity.tsx`.

### Distribution and Proof

- [ ] Keep install paths polished: Homebrew, cargo-from-tag, release binaries, and Gatekeeper docs. Evidence: README install section and `docs/blueprint/17-release-pipeline.md`.
- [ ] Test clean macOS and Linux installs before launch; publish pass/fail notes.
- [ ] Prepare launch assets: announcement/HN/Reddit copy, screenshots, and concise feature bullets.
- [ ] Write a conformance-suite post or section as a proof asset, based on existing conformance docs.

## Needs Validation

- [ ] Primary public category language: `local-first email infrastructure` vs `notebook for your email` vs `local mail runtime` vs `programmable email client`. README and site currently differ; decide by testing, not taste.
- [ ] Hero experiment: current site uses "Your inbox, on your computer." and README uses "Local-first email infrastructure." Test alternatives only if there is a concrete clarity/conversion problem.
- [ ] Explicit competitor comparison against Nylas, Superhuman, HEY, Gmail MCP servers, Composio/Zapier MCP, EmailEngine, Post, and `email-mcp`. Existing docs use fit/non-goals and lineage instead of direct comparison; add tables only if readers are confused.
- [ ] User-controlled encryption copy (`bring your own gpg/age/etc`). Keep out until product/docs show a real supported workflow.
- [ ] "Superhuman for terminal people..." line. Keep archival unless it survives positioning review.
- [ ] Dedicated `agent-safe by default` landing section and `local-first trust boundary` diagram. Current docs cover the substance; add visual/site sections only if they improve comprehension.

## Stale or Archived

- [x] Security & Privacy no longer has the stale "Not shipped yet" safety-copy section. Evidence: `site/src/content/docs/guides/security-and-privacy.md` documents MCP/agent permission profiles as shipped; `site/src/content/docs/reference/mcp.md`, `site/src/content/docs/reference/config.md`, `site/src/content/docs/guides/for-agents.md`, and `crates/mcp/src/lib.rs` describe the shipped surfaces.
- [x] The exact landing page "tested with Fastmail, Migadu, Proton Bridge" claim is gone. Evidence: `site/src/content/docs/index.mdx` now says Gmail/Outlook/any IMAP/SMTP; provider examples live in `site/src/content/docs/getting-started/imap-smtp-setup.md`. IMAP/SMTP live proof remains open in T2.1 because `scripts/live_provider_smoke_evidence.sh` still emits `unavailable_no_live_smoke`.
- [x] Release demo assets exist. Evidence: `site/public/mxr-demo.webm`, `site/public/mxr-tui.webm`, `site/public/mxr-agent.webm`, `site/public/mxr-demo-poster.jpg`, `site/public/mxr-tui-poster.jpg`, `site/public/mxr-agent-poster.jpg`, and `site/public/og.png`.
- [x] First-party MCP server, CLI+MCP agent contract, and MCP tools shipped. Evidence: `crates/mcp/src/lib.rs`, `crates/mcp/Cargo.toml`, `site/src/content/docs/reference/mcp.md`, `docs/implementation/v1-agent-mcp-gmail-launch/build-log.md`.
- [x] Agent read-only/draft-only profiles, account allowlists, send gates, destructive gates, activity origins, and dry-run requirements are documented/shipped. Evidence: `site/src/content/docs/reference/config.md`, `site/src/content/docs/guides/for-agents.md`, `crates/config/src/types.rs`.
- [x] Provider and interface capability matrices exist. Evidence: `site/src/content/docs/guides/why-mxr.md`.
- [x] README/site positioning no longer uses "The CLI for your email." Evidence: README intro and `site/src/content/docs/index.mdx`.
- [x] Local-first/privacy/no-cloud/control-plane copy exists. Evidence: README "Fit and Non-Goals", `site/src/pages/privacy.md`, `site/src/content/docs/guides/security-and-privacy.md`.
- [x] Conformance suite is mentioned on site/docs. Evidence: `site/src/content/docs/index.mdx`, `site/src/content/docs/reference/conformance.md`.
- [x] Concrete agent workflows and examples exist in docs. Evidence: `site/src/content/docs/index.mdx`, `site/src/content/docs/guides/for-agents.md`.
- [x] IMAP+SMTP setup is documented as first-party. Evidence: README supported surfaces, `site/src/content/docs/getting-started/imap-smtp-setup.md`.
- [x] Security & Privacy docs page exists. Evidence: `site/src/content/docs/guides/security-and-privacy.md`.
- [x] Architecture root/docs/posts exist. Evidence: `ARCHITECTURE.md`, `site/src/content/docs/guides/architecture.md`, `docs/articles/why-local-first-daemon-backed-email.md`.
- [x] Fast-start/demo path exists. Evidence: README `mxr demo`, `site/src/content/docs/getting-started/quick-start.md`, `docs/demo.tape`.
- [x] Tested-provider list exists on setup docs, but evidence backing needs audit through the T2.1 live-smoke follow-up. Evidence: `site/src/content/docs/getting-started/imap-smtp-setup.md`, `scripts/live_provider_smoke_evidence.sh`.

## Hygiene

- [ ] Before moving any item to implementation, create or attach a scoped task with `status:`, owner/executor, and target milestone using the implementation-task convention.
- [ ] Review this file after each release; remove shipped items only with code/doc evidence.
- [ ] Keep root TODO small. Detailed plans belong under `docs/implementation/**`.
