# 11: Ship one complete explanation or reader journey at each rung

## Build philosophy

Move from skateboard to scooter, bicycle, motorbike and car. Each rung is a working published improvement that can stand alone, sized for one implementation session. If scope does not fit, split it before building; use lettered rungs to keep cited numbers stable. At most two new unknowns enter a rung. Stop after any rung and the live site still has usable next steps.

No separate unusable foundation turn is needed: the site and its build already exist. Generation, scoped checks, design review and deployment travel with the nearest useful change. Passing tests and behaving correctly are required, but the reader outcome and design intent must also be observed. (D-WEB-010, D-WEB-021, D-WEB-024)

These are staged website releases under BK’s subsequent orchestrate authorisation. Rung 1 is deployed and its public entry paths are verified; [production evidence](../evidence/rung1-production.md) records the exact source and remaining comprehension limits. Rung 2 is deployed and its public agent tutorial is verified; [production evidence](../evidence/rung2-production.md) records the shared-draft evidence boundaries. Rung 3 is deployed with [production evidence](../evidence/rung3-production.md); rung 4 is next and rung 5 remains planned. Use the existing site delivery mechanism and do not start dependent code until its prerequisite deploys. Do not create a binary release/tag for copy-only work.

## Phase 0 verifies evidence and the first-screen direction

| Item | Action | Owner | Needed by |
|---|---|---|---|
| Evidence | Run S1: released/main/branch claim boundary | Implementing agent | Rung 1 |
| First proof | Run S2 for the existing subscription/local-query example | Implementing agent | Rung 1 |
| First-screen review | Q1–Q3 approved by BK after concrete comp | BK; resolved | Rung 1 implementation |
| Hosting access | Inspect existing Vercel build/production path; ask BK only if access is actually missing | Implementing agent; BK only for inaccessible credentials/approval | Deployed acceptance |

No account, billing, API key or real mailbox connection is required to write the first comp or prove the local synthetic query. No estimated delivery dates are claimed. Session scope, model waits and review availability determine elapsed time.

## Rung 1: Skateboard: understand why mxr exists and try a local result

Previously, the first screen can be read as an email client that an agent can query. Now it names the runtime and answers the connector question with a concrete local result.

Promise: a new reader can explain what mxr adds and find the safe trial.

Build: revise only the opening and first proof in index.mdx, the opening/fit of Why mxr, and the metadata/llms.txt/README introduction that repeat them. Keep install visible. Correct the social image's directly conflicting copy at source and regenerate it. Existing deeper sections remain reachable while later rungs refine them. (D-WEB-001, D-WEB-002, D-WEB-013, D-WEB-019, D-WEB-020)

Design: comp the first screen in both themes and mobile/desktop before implementation. Use the working-mailbox concept with a verified local result; a new interactive widget or recording is not required for this rung. Existing Use my agent links still land on For agents until rung 2 supplies the tutorial.

New unknowns: whether unfamiliar readers understand the runtime explanation; whether the proposed hierarchy works at small widths. Evidence S1/S2 is resolved in phase 0.

Demo: enter via a search/social snippet, read the opening, explain the connector distinction, follow the demo tutorial and reproduce the captured local query.

Done when:

- S4's comprehension check records the reader's own explanation and next action.
- Installation and a useful synthetic query are reachable without real account authorisation.
- Public HTML, curated llms.txt, README and social card do not contradict the new opening.
- Site checks, deployed-path verification and the design gate pass.

Left out: new agent tutorial, deep navigation reorganisation and new media production.

## Rung 2: Scooter: give an existing agent work and review its draft

Previously, an agent-curious visitor must assemble setup and safety rules across several pages. Now one tutorial takes them from the demo to a visible draft.

Promise: a newcomer can use their existing agent and see its saved work in a human client.

Build: revise the shared quick start, add the agent quick start, update agent-skill and For agents setup links, align MCP profile/confirmation guidance, and switch homepage agent CTA atomically. Preserve exact reference mechanics. (D-WEB-005, D-WEB-008, D-WEB-014, D-WEB-015, D-WEB-017)

Design: show a continuous synthetic account/thread/draft identity from tool result to human review. Setup feels progressive; permission refusal names the blocked capability and leaves the draft intact.

New unknowns: S3 shared-draft/refusal replay; current host-specific skill/MCP setup for the chosen named agent.

Demo: install/connect one supported agent path in an isolated synthetic profile, read evidence, create a draft under draft-only authority, open that draft in web/TUI and observe a prohibited send refusal. No mail is sent.

Done when:

- Same saved draft ID/account/revision appears across agent and human paths.
- Read-only and draft-only effects are demonstrated accurately; send remains prohibited.
- Plain CLI versus MCP authority and attribution are clearly explained.
- A reader can complete the tutorial from the deployed page with its actual prerequisites.
- Generator/build and design gates pass for touched surfaces.

Left out: hosted-agent networking, new permission systems, automatic sending and live-model quality claims.

## Rung 3: Bicycle: work through the five jobs without losing the runtime model

Previously, mode explanations are deep in the site and compete with generic feature buckets. Now the human route is a coherent part of the same product.

Promise: a user knows where to reply, act, glance, read and retrieve a fact.

Build: refine homepage mode/common-mailbox sections, Your day, mode overview and five mode intros; implement navigation groups in [07](07-guides-and-navigation.md) while preserving paths. Keep and relink the remaining human/client guides per the page audit. Update the glossary and compatibility explanations. (D-WEB-011, D-WEB-016, D-WEB-022)

Design: Now is the front page; each mode has one distinct hero object and verb. Do not give the five modes five identical list-screen illustrations. Review a complete entry → mode → source → next action journey.

New unknowns: navigation findability; whether the runtime and mode stories remain connected for non-agent users.

Demo: a new reader starts Now, chooses a reply or task, then locates a newsletter and a booking reference using the appropriate mode. Account/source context survives each handoff.

Done when:

- All five modes and Now have correct active workflows and a completion meaning.
- Every existing authored route is assigned to the new navigation and still reachable.
- Stored-assessment wording is included only if its release status has been verified; otherwise existing supported explanations remain.
- Mobile search/sidebar and the mode journey pass the design gate and deployed check.

Left out: classifier implementation, new modes and reworking unrelated application UI.

## Rung 4: Motorbike: compose a custom workflow and understand its boundaries

Previously, the site exposes capabilities but leaves the composition and authority story distributed. Now an agent operator can build a bounded workflow with evidence and recovery.

Promise: a builder can select mail, prepare an action and verify the result without learning provider plumbing.

Build: revise Examples, Recipes, Analytics, Architecture, Automation contract, Security & Privacy, Activity and LLM features. Add the invoice workflow only if replay proves it; otherwise improve an existing attachment recipe. Refresh the homepage trust/composability explanation and the best proof capture. (D-WEB-003, D-WEB-004, D-WEB-017, D-WEB-019)

Design: the sequence is evidence → proposal → review → outcome, using the same objects as rung 2. Command detail recedes beneath the job; important limits remain next to the relevant claim.

New unknowns: exact end-to-end attachment workflow; whether users distinguish external-agent reasoning from optional native model features.

Demo: run a local aggregate or attachment workflow on synthetic data, preview the selection, inspect the permitted action and verify resulting state; show where a cloud model would receive data without making an unnecessary cloud call.

Done when:

- Examples use actual output shapes, explicit candidate limits and account scope.
- No claim of universal logging, actual-open tracking, identical permissions or universal undo survives in touched prose.
- Model and network disclosure matches the described feature/version.
- The canonical trust and contract pages agree with reference and the observed demonstration.
- Deployed journey and design gate pass.

Left out: new automation runtime, processor integrations, unrestricted SQL access and new telemetry.

## Rung 5: Car: every entry point tells the same current story

Previously, remaining media, machine documents and peripheral entries can still teach an older product. Now a search hit, agent document or deep link leads to the same model and useful next action.

Promise: people and agents can enter anywhere and find current, consistent docs.

Build: complete [09](09-discovery-and-maintenance.md), all page-audit dispositions, remaining media/captions, current root docs and generated-source corrections. Finish only the focused checks that catch real observed drift. Preserve old plans, URL compatibility and reference generation authority. (D-WEB-018, D-WEB-020, D-WEB-021, D-WEB-024)

Design: review the whole site as one map, including error/empty/help paths and shared patterns. Remove any repeated sales section that makes the product feel assembled from disconnected pages.

New unknowns: S5's generated/deep-link completeness; the effort to update any remaining stale recordings. Split media into a lettered rung if it exceeds one session.

Demo: enter through homepage, social card, Why mxr, a mode guide, an agent quick start, llms.txt and a CLI-reference link. Complete the appropriate next task from each.

Done when:

- The authored-route inventory and navigation agree, generated outputs are current, and no changed link silently fails.
- Media labelled as evidence was replayed on the recorded version; proposed/canned content is labelled.
- Main/release/branch claims are separated and current installation behaviour matches public instructions.
- Reader outcomes, accessibility/design checks and production routes pass; handoff records evidence URLs and remaining issues.

Left out: a public content platform, brand replacement and expansion into unrelated mail features.

## After the car (not planned yet)

Evaluate whether user questions still reveal a missing explanation. Propose another slice only for that demonstrated problem. Do not turn the website into an unbounded campaign or a reason to add runtime capabilities.

Unresolved questions: Q1–Q3 are approved; Q4–Q7 have stated assumptions. S1–S5 remain the evidence gates for implementation.
