# mxr TODO

Comprehensive repo TODO distilled from the 2026-03-20 market + positioning review.

Goal: make mxr legible as local-first email infrastructure for humans + agents, not just another terminal mail client.

## P0 Product truth

- [ ] Ship a real `mxr` MCP server, not just CLI/skill positioning.
- [ ] Decide the primary category language to test on the site: `local-first email runtime`, `local-first email infrastructure`, or `programmable email client`.
- [ ] Make every public page honest about what works today vs what is roadmap.
- [x] Publish a capability matrix by provider: Gmail, IMAP, SMTP, fake.
- [x] Publish a capability matrix by interface: CLI, TUI, daemon socket, skill, MCP.
- [ ] Add end-to-end smoke tests for the real user journey: install -> auth -> sync -> search -> draft -> approve -> send.

## P0 Agent safety

- [ ] Add read-only mode for agents.
- [ ] Add draft-only mode for agents.
- [ ] Add explicit send approval flow.
- [ ] Add explicit destructive-action scopes: archive, trash, delete, label, unsubscribe.
- [ ] Add reversible batch ops where possible.
- [x] Add audit logging for agent-initiated actions.
- [ ] Surface action origin everywhere relevant: human, script, agent, MCP.
- [x] Make `--dry-run` coverage complete for all risky mutations.
- [x] Document the trust model clearly: local store, direct provider access, no hosted control plane.

## P0 Website + README

- [ ] Test a stronger hero than `The CLI for your email.`
- [x] Add a local-first/privacy paragraph directly under the hero.
- [x] Add `no cloud middleware / no third party in the data path` copy where accurate.
- [x] Add explicit positioning copy for `why mxr exists if Nylas CLI exists`.
- [x] Add explicit positioning copy for `why mxr exists if Composio / Zapier MCP exist`.
- [ ] Decide whether to explicitly contrast against Nylas, Superhuman, HEY, and Gmail MCP servers on the site.
- [x] Move one concrete agent workflow above the fold.
- [x] Add `no SDK needed` / Unix composability copy with a real pipeline example.
- [ ] Add copy for user-controlled encryption: `bring your own gpg/age/etc`.
- [ ] Surface the `Superhuman for terminal people, but local-first and scriptable` line if it survives review.
- [x] Mention the conformance suite on the landing page as a trust signal.
- [x] Add a short HTML-email story: clean text by default, open full HTML when needed.
- [x] Add a dedicated `Why mxr vs hosted agent connectors` section or page.
- [x] Add a dedicated `Why mxr vs terminal mail clients` section or page.
- [ ] Add a `local-first trust boundary` diagram to the landing page.
- [ ] Add an `agent-safe by default` section to the landing page once the product supports it.
- [x] Keep README and landing page copy in sync.

## P0 Proof, not promises

- [ ] Record at least 3 short demos: inbox triage, meeting prep, CI/build-failure cleanup.
- [ ] Record a canonical terminal demo with asciinema or VHS.
- [x] Add copy that shows concrete workflows, not generic AI claims.
- [x] Publish example commands with real JSON output.
- [ ] Publish example agent prompts paired with the exact `mxr` commands they trigger.
- [ ] Add benchmark/proof for local search speed and local-open latency.
- [ ] Add screenshots or GIFs for daemon + TUI + CLI working together.
- [ ] Add a GIF for HTML email fallback: TUI read -> open in browser.
- [ ] Add a demo of an agent safely using `mxr`.

## P1 Agent interface

- [ ] Decide the official agent surface area: CLI only, MCP only, or both.
- [ ] If both, define the contract: when agents should call CLI vs MCP.
- [x] Expand the agent docs from `install the skill` into a real guide for safe operation.
- [x] Add a dedicated `For agents` landing page/section.
- [ ] Add setup docs for Codex, Claude Code, Cursor, VS Code, OpenAI Agents SDK.
- [ ] Publish recommended permission presets for personal use vs work use.
- [ ] Add examples for approval-gated sending and draft review loops.
- [ ] Add docs for running `mxr` headless in CI/containers/remote shells.

## P1 Core product gaps to close

- [ ] Finish Gmail adapter work and document current completeness.
- [ ] Reduce Gmail setup pain in docs: OAuth, scopes, verification expectations, failure modes.
- [ ] Make IMAP + SMTP setup feel first-class, not fallback.
- [ ] Ensure every daemon capability has both CLI and TUI wiring where applicable.
- [ ] Harden export flows for agent use: markdown, JSON, thread context.
- [ ] Ensure search + batch mutation flows are reliable enough to be trusted by agents.
- [ ] Expose diagnostics that explain exactly why auth/sync/send failed.

## P1 Competitive framing

- [x] Add Nylas CLI to competitive framing as the closest modern all-in-one CLI + MCP analog.
- [x] Add Composio to competitive framing as hosted Gmail/agent middleware.
- [x] Add Zapier MCP to competitive framing as hosted Gmail/action middleware.
- [x] Add EmailEngine to competitive framing as self-hosted email gateway/integration infra.
- [x] Add Post to competitive framing as a recent local mail daemon + CLI + MCP analog.
- [x] Add `email-mcp` to competitive framing as local MCP plumbing without the broader mail runtime.
- [x] Add Gmail MCP servers to competitive framing as Gmail-only access paths.
- [ ] Keep the comparison table honest and narrow: direct peers in one table, hosted connectors in another section.
- [ ] Build a second comparison table specifically for agent/automation email access.
- [x] Write a short `when to choose mxr / when not to choose mxr` page.

## P1 Trust + operability

- [ ] Add a user-visible action history / event log page.
- [ ] Add undo/rollback affordances for supported bulk actions.
- [ ] Add better confirmations for unsubscribe, archive-all, trash-all, send.
- [ ] Add account scoping so agents can be restricted to one account.
- [ ] Add config to disable risky commands entirely in specific environments.
- [ ] Add docs for safe defaults in shared/company machines.
- [x] Add a dedicated `Security & Privacy` docs page.

## P1 Architecture docs

- [x] Create root-level `ARCHITECTURE.md` that summarizes the core principles and links to the full blueprint.
- [x] Link `ARCHITECTURE.md` from `README.md`.
- [x] Add an architecture page to the docs site.
- [x] Write the opinionated architecture post: why local-first + daemon-backed is the right shape.

## P2 Distribution + adoption

- [ ] Polish install paths: Homebrew, cargo, binaries, docs.
- [ ] Add a fast-start path for people who only want agent access.
- [ ] Add a fast-start path for people who only want human CLI/TUI use.
- [x] Add an honest status badge / support matrix for current platform + provider support.
- [ ] Publish one opinionated `agent inbox triage` recipe.
- [ ] Publish one opinionated `founder / manager meeting prep` recipe.
- [ ] Publish one opinionated `engineering incident / CI cleanup` recipe.
- [ ] Prepare launch assets: announcement post, HN copy, screenshots, feature bullets.

## P2 Provider compatibility

- [ ] Test Fastmail via IMAP and document results.
- [ ] Test Proton Mail Bridge via IMAP and document results.
- [ ] Add a `tested providers` list to the site once results exist.

## P2 Launch + outreach

- [ ] Test install on clean macOS and Linux machines before launch.
- [ ] Draft the HN launch post.
- [ ] Draft Reddit launch posts for `r/rust`, `r/commandline`, `r/selfhosted`, `r/neovim`, `r/privacy`.
- [ ] Prepare short outreach copy for creators/newsletters.
- [ ] Build a target list for terminal/Rust/privacy/self-hosted creators.

## P2 Content

- [x] Write `Why mxr?` / philosophy docs page.
- [x] Write the `spec is the innovation` post about the blueprint + conformance approach.
- [ ] Write a conformance-suite post or section as a marketing asset.

## P2 Repo hygiene

- [ ] Add an owner + target milestone next to each high-priority item once priorities settle.
- [ ] Review this list after each release; remove shipped items, split vague items, add evidence.
