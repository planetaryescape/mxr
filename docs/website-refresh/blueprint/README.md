# mxr website: a blueprint for the local mail runtime

mxr's website must explain why a locally synced, programmable mailbox is useful even when an agent already has Gmail access, then let a visitor prove it safely. The five modes, human clients, scripts and agents belong to the same story. (D-WEB-001 through D-WEB-005, D-WEB-016)

This is a website feature blueprint, prepared on 2026-10-23 from BK's current request, recovered product discussions and fresh repository evidence. It specifies documentation and presentation work, not new mail-runtime features. The implementation baseline is `origin/main` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4`; the supplied feature branch is tracked separately. The original planning task is complete. BK subsequently authorised staged implementation through orchestrate and approved the first-screen direction. All five website rungs are deployed; the [final production record](../evidence/rung5-production.md) gives the source, checks and remaining evidence limits. (D-WEB-018, D-WEB-025–027)

## Read the explanation, then the surfaces and delivery checks

| # | Document | Purpose |
|---|---|---|
| 00 | [Overview](00-overview.md) | Reader outcome, scope, vocabulary and assumptions |
| 01 | [Site architecture](01-site-architecture.md) | Existing source ownership, route strategy and reader map |
| 02 | [Claims and content](02-claims-and-content.md) | Promises, proof, scope and canonical ownership |
| 03 | [Design concept](03-design-concept.md) | The working mailbox, key moments, tokens, mockups and design gate |
| 04 | [Positioning](04-positioning.md) | Connector answer, six connected reasons and copy boundaries |
| 05 | [Homepage](05-homepage.md) | Opening copy, proof sequence, section order and changes to current content |
| 06 | [Onboarding and agent journeys](06-onboarding-and-agent-journeys.md) | Shared demo tutorial, agent setup, saved draft and refusal proof |
| 07 | [Guides and navigation](07-guides-and-navigation.md) | Complete sidebar membership, canonical guide briefs and mode workflows |
| 08 | [Trust and models](08-trust-and-models.md) | Actual enforcement, untrusted mail, recovery and model/data disclosure |
| 09 | [Discovery and maintenance](09-discovery-and-maintenance.md) | Metadata, machine docs, media, root docs and release alignment |
| 10 | [Verification](10-verification.md) | Reader outcomes, focused checks, browser and deployed acceptance |
| 11 | [Roadmap](11-roadmap.md) | Five usable website releases with demos and completion criteria |
| 12 | [Decisions](12-decisions.md) | 28 decisions, with BK's direction separated from agent proposals |
| 13 | [Open questions](13-open-questions.md) | Seven product assumptions and five evidence spikes |
| — | [Kickoff](KICKOFF.md) | First implementation-session brief |
| — | [Handoff](HANDOFF.md) | Current state, exact next action, blockers and validation evidence |

Evidence and inventory:

- [Research and claim register](../../research/website-refresh-2026-10-23.md): 25 entries grounded in named code/source, historical context and bounded connector research.
- [Complete page audit](../../research/website-page-audit-2026-10-23.md): historical baseline of 80 authored Markdown/MDX sources, with the later 81-route implementation inventory plus metadata, media, legal and generated surfaces.
- [Existing product blueprint](../../blueprint/README.md): product design history, preserved rather than rewritten.

## The design shows one task moving through one mailbox

The approved concept is “The working mailbox”: evidence is selected locally, an agent or script prepares work, and the person reviews the same object in a human client. Refine the site's existing Recursive/deep-blue/cyan visual language around that relationship. BK approved the concept and headline after the first-screen comp. (D-WEB-010, D-WEB-012, D-WEB-013)

## Keep the implementation honest and small

1. Start with current evidence. Re-fetch main and identify the published binary version before promoting a claim. A branch preview is not release proof. (D-WEB-018)
2. Keep source ownership. Edit authored pages and generator inputs; inspect generated outputs. No duplicate command inventory or new CMS. (D-WEB-010, D-WEB-020, D-WEB-022)
3. Prove complete tasks. Use synthetic state, scope queries and previews, carry draft identity into human review and verify outcomes. (D-WEB-005, D-WEB-014, D-WEB-017)
4. Preserve URLs and useful depth. Reorganise navigation and entry paragraphs without rewriting unrelated technical prose. (D-WEB-011, D-WEB-023)
5. Attribute decisions. Update [12](12-decisions.md) when the direction changes; never silently convert an agent proposal into BK's decision. (D-WEB-024)
6. Keep the product separate from this site task. Do not implement classifiers, new safety mechanisms, team mail or new hosted services as part of the refresh. (D-WEB-006, D-WEB-023)

Unresolved questions: Q1–Q3 are approved for rung 1. Q4–Q7 have explicit defaults for stack/routes, released behaviour, demo/agent ordering and direct acceptance checks. [13](13-open-questions.md) lists all assumptions and evidence gates.
