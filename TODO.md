# mxr TODO

Status: triaged on 2026-06-19 from `main`.

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

## Validated

### Product and Docs Truth

- [ ] Audit public docs for stale safety copy. `site/src/content/docs/guides/security-and-privacy.md` still lists first-party MCP server, read-only/draft-only agent modes, account-scoped permissions, send approval, and config-based risky-command blocking under "Not shipped yet", while MCP/config/agent docs and code show those exist. Evidence: `crates/mcp/src/lib.rs`, `site/src/content/docs/reference/mcp.md`, `site/src/content/docs/reference/config.md`, `site/src/content/docs/guides/for-agents.md`.
- [ ] Make public pages explicit about what is shipped vs roadmap anywhere they imply a fully permissioned agent sandbox. Evidence: `site/src/content/docs/guides/for-agents.md` has current limits; `site/src/content/docs/guides/security-and-privacy.md` is stale.
- [ ] Decide whether the landing page provider line "tested with Fastmail, Migadu, Proton Bridge" should be backed by live evidence docs or softened. Evidence: `site/src/content/docs/index.mdx` claims tested providers, while `scripts/live_provider_smoke_evidence.sh` still emits `unavailable_no_live_smoke` for IMAP/SMTP when creds exist but no network-safe live smoke is committed.

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
- [ ] Record release assets: canonical terminal demo, short inbox-triage/meeting-prep/CI-cleanup demos, and screenshots/GIFs for CLI + TUI + daemon + HTML fallback.
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

- [x] First-party MCP server, CLI+MCP agent contract, and MCP tools shipped. Evidence: `crates/mcp/src/lib.rs`, `crates/mcp/Cargo.toml`, `site/src/content/docs/reference/mcp.md`, `docs/implementation/v1-agent-mcp-gmail-launch/build-log.md`.
- [x] Agent read-only/draft-only profiles, account allowlists, send gates, destructive gates, activity origins, and dry-run requirements are documented/shipped. Evidence: `site/src/content/docs/reference/config.md`, `site/src/content/docs/guides/for-agents.md`, `crates/config/src/types.rs`.
- [x] Provider and interface capability matrices exist. Evidence: `site/src/content/docs/guides/why-mxr.md`.
- [x] README/site positioning no longer uses "The CLI for your email." Evidence: README intro and `site/src/content/docs/index.mdx`.
- [x] Local-first/privacy/no-cloud/control-plane copy exists. Evidence: README "Fit and Non-Goals", `site/src/pages/privacy.md`, `site/src/content/docs/guides/security-and-privacy.md`.
- [x] Conformance suite is mentioned on site/docs. Evidence: `site/src/content/docs/index.mdx`, `site/src/content/docs/reference/conformance.md`.
- [x] Concrete agent workflows and examples exist in docs. Evidence: `site/src/content/docs/index.mdx`, `site/src/content/docs/guides/for-agents.md`.
- [x] IMAP+SMTP setup is documented as first-party. Evidence: README supported surfaces, `site/src/content/docs/getting-started/imap-smtp-setup.md`.
- [x] Security & Privacy docs page exists, but needs stale-section audit. Evidence: `site/src/content/docs/guides/security-and-privacy.md`.
- [x] Architecture root/docs/posts exist. Evidence: `ARCHITECTURE.md`, `site/src/content/docs/guides/architecture.md`, `docs/articles/why-local-first-daemon-backed-email.md`.
- [x] Fast-start/demo path exists. Evidence: README `mxr demo`, `site/src/content/docs/getting-started/quick-start.md`, `docs/demo.tape`.
- [x] Tested-provider list exists on landing/setup docs, but evidence backing needs audit. Evidence: `site/src/content/docs/index.mdx`, `site/src/content/docs/getting-started/imap-smtp-setup.md`.

## Hygiene

- [ ] Before moving any item to implementation, create or attach a scoped task with `status:`, owner/executor, and target milestone using the implementation-task convention.
- [ ] Review this file after each release; remove shipped items only with code/doc evidence.
- [ ] Keep root TODO small. Detailed plans belong under `docs/implementation/**`.
