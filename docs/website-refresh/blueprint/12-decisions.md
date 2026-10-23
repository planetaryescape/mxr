# 12: mxr is a local mail runtime that people and agents work through

BK decides the product direction. Entries attributed to Codex are implementation proposals made under BK's request for a blueprint; they are not retroactive approvals. Add a replacing decision when new evidence changes one. This feature-scale log leaves the existing product decision log intact.

## The conversation moved from capabilities to the abstraction

1. Earlier discussions established that programmable mail capabilities, local history and provider abstraction are the product's foundation.
2. BK distinguished an external agent using mxr from mxr's optional native model features.
3. Earlier assistant claims about a competitor's missing APIs and account support were corrected after research. We retain the correction, not the original claim.
4. BK now asks for a comprehensive website plan because an email-client pitch loses that distinction.
5. The current site already contains much of the substance, but its opening, onboarding and machine index do not consistently explain it.
6. We propose an evidence-led entry: a useful result, the local runtime that makes it possible, and a safe way to try it.
7. The five modes explain how a person works with that same runtime. They belong in the core story.
8. This task ends with a verified blueprint. Website implementation and publication are subsequent work.

### D-WEB-001: Use “local mail runtime” as the product category. (BK)

Decision: The category must be visible on the first screen and consistent across entry points.

Rationale: BK said, “if you promote mxr as merely an email client then you miss the point of what makes mxr unique.” A terminal or web client is one way to use the product.

### D-WEB-002: Answer why someone would choose mxr over a Gmail connector. (BK)

Decision: Address this in the homepage opening and in the existing Why mxr guide.

Rationale: BK explicitly named the connector objection. “Works with agents” alone does not answer it.

### D-WEB-003: Lead with capabilities people can compose into their own workflows. (BK)

Decision: Explain outcomes across analytics, attachments, search, drafts, rules and mail state, rather than a fixed menu of AI features.

Rationale: In the recovered discussion BK said, “let your agent be able to do it, build whatever features you want. Script it.” Do not turn the historical “200 things” illustration into an unverified operation count.

### D-WEB-004: Explain the modern mail model above provider plumbing. (BK)

Decision: Show what mxr handles underneath: provider differences, synchronization and the common mail operations.

Rationale: BK said, “Mxr handles the archaic stuff behind the scenes and exposes a modern api.” Generic “email primitives” could mean exposing IMAP complexity unchanged.

### D-WEB-005: Explain one mailbox shared by human and machine clients. (BK)

Decision: Use the same draft or thread identity to demonstrate handoff across clients.

Rationale: The request explicitly identifies the daemon behind TUI, web, CLI, MCP and skill. Shared means the user's mailbox across their clients; it does not assert team inbox collaboration.

### D-WEB-006: Plan the website without building new runtime features. (BK)

Decision: Deliver a feature-scale blueprint, research, audit and implementation handoff. Do not implement the redesign, commit, or publish.

Rationale: The request is to investigate and create a plan and blueprint.

### D-WEB-007: Put the existing-agent visitor first in the opening journey. (Codex proposal)

Decision: Use the connector question as the first explanation while keeping a complete non-agent route.

Rationale: It is the specific confusion BK wants fixed. This reprioritises the older PRODUCT.md audience ordering and is assumption Q1, not an already-approved change.

### D-WEB-008: Keep external agents, the built-in assistant and native model features distinct. (BK; implementation detail proposed by Codex)

Decision: Explain three separate choices: an existing agent calling mxr, the built-in assistant, and optional synthesis features configured in mxr.

Rationale: Earlier discussions repeatedly corrected the belief that mxr requires one model or a second AI subscription. No promise that an agent subscription pays for mxr's own model calls.

### D-WEB-009: Make the connector comparison factual and bounded. (Codex proposal, following BK's research requirement)

Decision: Describe mxr's local store, common state and operating contract. Do not claim every connector is single-account, uncached, read-only, unable to draft, or universally slower.

Rationale: Current official connector documentation already describes actions, permissions and, for some apps, multiple accounts. No competitor latency or token-cost benchmark was measured.

### D-WEB-010: Reuse Astro, Starlight and the site's existing visual vocabulary. (Codex proposal)

Decision: Keep the current stack and source locations. Refine the presentation around the new argument.

Rationale: The problem is product explanation and information hierarchy. A framework or brand replacement adds unrelated work. Assumption Q4.

### D-WEB-011: Preserve existing public URLs while reorganising navigation. (Codex proposal)

Decision: Give each existing page an explicit keep, rewrite, revise or regenerate disposition. Add pages only where a journey lacks an owner.

Rationale: Deep links and installed-agent instructions already use these routes. Historical plans remain historical.

### D-WEB-012: Use “The working mailbox” as the site design concept. (Codex proposal)

Decision: Show one persistent mailbox, evidence being selected, work being prepared and a person reviewing the result.

Rationale: This makes the runtime visible through consequences. It is assumption Q2. BK did not supply a new visual concept in this conversation.

### D-WEB-013: Pair an outcome headline with a literal category sentence. (Codex proposal)

Decision: Candidate headline: “Put your whole mail history to work.” Category: “A local mail runtime for you and your agents.”

Rationale: “Runtime” needs a concrete job beside it. Candidate copy is assumption Q3, open to BK's editorial choice.

### D-WEB-014: Let people prove value using synthetic mail before connecting an account. (Codex proposal)

Decision: Keep installation visible, then offer one small demo journey ending in a verifiable local result.

Rationale: The existing demo avoids asking for mailbox access before the visitor understands the product. Assumption Q6. Demo model replies are canned, not evidence of live model quality.

### D-WEB-015: Give human and agent onboarding equal product legitimacy. (Codex proposal)

Decision: A user can finish with CLI or web/TUI; a user with an agent can attach it after the demo. Do not tell everyone they will mainly use the TUI.

Rationale: The surfaces are choices over the same system, not tiers of users.

### D-WEB-016: Present five modes plus Now as workflows above the same mailbox. (BK's product constraints)

Decision: Explain Messages, To do, Updates, Reading, Archive and Now together, with a job and completion meaning for each.

Rationale: The request points to email modes as part of today's product. They are not merely labels or an optional future direction.

### D-WEB-017: Put trust mechanisms beside demonstrations of power. (BK; wording narrowed by code evidence)

Decision: Explain profiles, account scopes, preview, send gates, pre-send checks, undo and local activity with their actual limits.

Rationale: Daemon enforcement is stronger than a prompt convention, but host access, operation-specific undo and optional logging prevent blanket guarantees.

### D-WEB-018: Separate released behaviour from feature-branch work and future plans. (Codex proposal, following repo rules)

Decision: Main is the audit baseline; verify the published release before shipping copy. Stored assessments in the supplied branch get a conditional integration instruction.

Rationale: A preview page and a blueprint are not proof that an installed release contains their behaviour. Assumption Q5.

### D-WEB-019: Use exact evidence for every public performance and engagement claim. (Codex proposal)

Decision: Use “synced history”, disclose incomplete coverage, explain local READ flags, and never call a limited ranked result “every newsletter” without checking coverage.

Rationale: The subscription command ranks a limited returned set; read state is not distinct-open telemetry. No timing or token-reduction percentage was measured.

### D-WEB-020: Treat metadata, media and agent-readable exports as website surfaces. (Codex proposal)

Decision: Update descriptions, social-card source, captions, curated llms.txt, generated Markdown and full text, repo entry points and navigation alongside prose.

Rationale: Visitors and agents encounter these surfaces without the homepage's surrounding explanation.

### D-WEB-021: Judge success through observed reader tasks and deployed journeys. (Codex proposal)

Decision: Check comprehension, safe trial and handoff directly. No new public analytics service is required.

Rationale: A successful site build does not prove that a visitor understands why mxr exists. Assumption Q7.

### D-WEB-022: Give each subject one canonical explanation and cross-link it. (Codex proposal)

Decision: Why mxr owns fit; email modes owns the human model; for-agents owns agent operation; trust pages own policy and data flow; generated reference owns mechanics.

Rationale: Repeated pitches and hand-maintained command inventories drift.

### D-WEB-023: Keep implementation proportional to the docs problem. (Codex proposal)

Decision: Add no mail-runtime features, production demos, custom CMS, new hosting system, metrics pipeline or framework migration. Record runtime gaps as repo Markdown issues if found.

Rationale: Fix confusing workflows before adding capabilities. Existing tools and generators already cover most needs.

### D-WEB-024: Ship in complete reader journeys and preserve a handoff. (Codex proposal)

Decision: Sequence five usable rungs with named acceptance checks; preserve questions, evidence and exact next action in HANDOFF.md.

Rationale: The next agent must be able to implement without this chat. Never silently rewrite a decision or treat a proposed rung as delivered.

### D-WEB-025: Execute the website blueprint in staged deployments. (BK)

Decision: The subsequent orchestrate invocation authorises in-scope implementation, normal commits/PRs and the existing deployment path. It supersedes the planning-only limit in D-WEB-006. Preserve the old planning tree and do not delete resources without exact approval.

### D-WEB-026: Use the reviewed first-screen direction. (BK)

Decision: BK approved Q1–Q3 after the concrete comp: existing-agent visitors first, the working-mailbox concept, and “Put your whole mail history to work”. This confirms the proposals D-WEB-007, D-WEB-012 and D-WEB-013 from this point forward.

### D-WEB-027: Make local ownership and reusable daemon capabilities explicit. (BK)

Decision: Explain the value of a mailbox on your own computer alongside continued remote-server sync. Explain that clients reuse the daemon's mail operations rather than each rebuilding provider/sync logic. Do not imply every attachment is cached, remote access is automatic, or new clients require no work. The ownership claim applies to synced content and the user's local tools.
