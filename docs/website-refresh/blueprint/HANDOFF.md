# Rung 1 is live; rung 2 is integrated and being prepared for release

Current checkpoint, 23 October 2026: [PR #688](https://github.com/planetaryescape/mxr/pull/688) is merged as `cc00d4081bb5bd7875c39da4641ff95888817d80`. Its exact reviewed tree is deployed READY on [mxr.sh](https://mxr.sh/). The [production record](../evidence/rung1-production.md) contains deployment identity, public entry/CTA/export checks and remaining limits. BK's admin-merge authority and second-opinion waiver persist; do not ask about either again.

The control worktree remains `/Users/bhekanik/.t3/worktrees/mxr/docs-runtime-website-control`, now on `docs/runtime-agent-onboarding`, created from freshly fetched deployed main. The old control branch and original checkout/planning are preserved. The [Codex setup evidence](../evidence/rung2-codex-setup.md) is integrated and root-checked, including local stdio configuration, trusted-project scope, skill discovery paths and valid mxr profile aliases. It is help/source/documentation evidence, not an actual configured Codex host run. The [S3 proof](../evidence/s3-shared-draft-proof.md) is now integrated: root checked the raw MCP refusals, revision-2 draft identity and matching body/account in the actual web-client DOM capture. The UI does not display a numeric revision, and the replay was a protocol harness rather than a model run. Task `mxr-website-rung2-onboarding-20261023-r1` completed the bounded tutorial and its entry links. Root integrated it, rebased onto current main `549e10e6d928d6293c93ab3d0fef57c7f0b0e4f6`, corrected the demo exit sequence and policy wording, and verified the build, links and static phone layout. Exact task IDs, paths and resource ownership are in the existing durable ledger.

Next: finish final source checks, commit the root corrections, preserve the local author backup and normalise unpublished committer identity, refresh/rebase, then explicitly push `docs/runtime-agent-onboarding`, open/link its PR, check and admin-merge it, and verify the existing production deployment. Do not push main. No worker remains active. Keep the public normal-demo setup distinct from the private S3 fixture: do not copy its synthetic account selector as a universal demo default or claim that Codex itself was configured during S3. The account-limited MCP profile refuses unscoped draft listing, so the tutorial must retain the saved draft ID and use scoped retrieval. BK explicitly authorised doing all work needed to ship without further permission requests; second-opinion reviews remain waived. Do not add runtime features or invent model-quality evidence. Rungs 3–5 remain planned behind staged deployments. No real mail, second-opinion worker, destructive cleanup or duplicate fixture is authorised.

The following entries are historical checkpoints, superseded by the current status above.


Task: bring the docs website's explanation up to mxr as a local mail runtime, including the connector distinction, shared human/agent state, composable capabilities, modes and actual safety boundaries.

## Evidence and workspace

- Control worktree: `/Users/bhekanik/.t3/worktrees/mxr/docs-runtime-website-control`. The original planning source remains preserved at `/tmp/mxr-docs-positioning-clones/mxr`.
- Branch: `docs/runtime-website-control`, created from freshly fetched `origin/main` `f39ae66578e3099e464967ab2f042e24b1ad8ec4`.
- Feature reference: `origin/feat/classifier-assessment-r2` `2053f18ed406b981ad56639b64a29703b4fd9a7a`; audit merge base equals the main baseline.
- Installed binary observed: `mxr 0.6.155`; main release manifest observed: `0.6.156`. Published artifact status remains S1.
- [Research](../../research/website-refresh-2026-10-23.md) has primary code links and the narrowed/held claims; [page audit](../../research/website-page-audit-2026-10-23.md) accounts for all 80 authored sources.
- Full source ledger and task-relevant private ChatGPT exports are in `/tmp/mxr-website-research/`, not in the repo. The blueprint carries the decisions and necessary product context without private correspondence.

## Current outcome

The 14 numbered chapters, README, kickoff and handoff specify the site architecture, message, page work, design concept, trust model, metadata/media, checks and five implementation rungs. There are 24 decisions, seven explicit product assumptions and five evidence spikes. Only planning/research and a discovery link in the existing product blueprint were changed. No website, runtime, account or real mail was changed. No commit, push or publication was made.

The proposed concept and copy remain proposals: “The working mailbox”, “Put your whole mail history to work”, and the category “A local mail runtime for you and your agents.” BK's runtime direction is settled; these specific editorial choices are not attributed to him.

## Exact next action

When BK requests implementation, read KICKOFF.md and run S1/S2 against fresh source and an isolated synthetic profile. Prepare the rung 1 first-screen comp for the Q1–Q3 review. Do not begin by rewriting all 80 pages or building new mail capabilities.

## Remaining blockers and boundaries

There is no blocker to reading or reviewing the blueprint. The supplied preview URL requires Vercel authentication; its exact deployment SHA was not established. T3 preview snapshots timed out twice; public homepage text and pinned source were read, but no rendered-site visual audit is claimed. S1–S5 intentionally remain unexecuted implementation checks. Audience priority, concept and headline await BK's first-screen review; Q4–Q7 have documented defaults.

## Validation

Observed planning checks on 2026-10-23:

- `python3 -I /Users/bhekanik/.dotfiles/.skills/blueprint/scripts/check_blueprint.py docs/website-refresh/blueprint`: 17 documents, 24 decisions, 12 spikes/questions; zero errors and warnings.
- `/tmp/mxr-website-research/check-coverage.py`: 80 authored source files, all 79 non-homepage routes assigned to proposed navigation, 29 pinned source links with existing paths and valid line extents.
- `git diff --check`: passed. Invisible-mark pass over the 19 new Markdown files found no changes needed.
- Final fetch confirmed both audited refs unchanged.
- HTML companion inspected with T3 `html_preview` at 728 px dark and 390 px light. All four mechanism states and five rung controls passed the in-page interaction check; neither viewport overflowed horizontally. Opened in Dia. The companion is an explanatory mechanism/sequence, not an implementation of the redesigned site.

Companion: `/var/folders/3p/n76j7jp14ljcvr1bhf7qmlh00000gn/T/agent-plans/20261023-143639-mxr-website-runtime-blueprint.html`.

No website build or runtime demonstration is claimed by this planning-only change. S1–S5 remain implementation checks.

Unresolved questions: [13](13-open-questions.md) contains Q1–Q7 and S1–S5 with defaults, owners and gates.

## Orchestration resumed on 2026-10-23

BK invoked orchestrate. Durable control ledger: `/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-website-runtime-20261023/ledger.md`; the root orchestrator is its sole writer. Two T3-owned isolated child tasks are running on codex gpt-6-luna high, service default:

- Evidence: task `mxr-website-phase0-evidence-20261023-r1`, branch `t3/research-website-runtime`, worktree `/Users/bhekanik/.t3/worktrees/mxr/t3-b3d8050b`. Source/release/deploy-state discovery and safely isolated synthetic proof only.
- Comp: task `mxr-website-phase0-comp-20261023-r1`, branch `t3/first-screen-design-comp`, worktree `/Users/bhekanik/.t3/worktrees/mxr/t3-595080d7`. Standalone first-screen HTML comp only; site source unchanged.

Root's Vercel connector identified existing team Planetary Escape, project mxr-mail, production deployment `dpl_E4TfbQtPZzWm2ACvHR7FqoxcsgSo`, READY and aliased to mxr.sh, source `f39ae66578e3099e464967ab2f042e24b1ad8ec4` on main. No project settings or deployment changed.

Control npm ci completed in 4 seconds: 528 packages, zero reported vulnerabilities. node_modules measured 452204 KiB; no server started. Prior source/planning worktrees are retained; cleanup approval is pending and no deletion is authorised.

Exact next action: collect the two phase-0 packets, verify their evidence and comp, present the actual first screen for the Q1–Q3 review, then delegate the first implementation slice. Do not start dependent code before the preceding slice deploys. Do not launch duplicate workers.

### The first-screen choice is now pending

Both phase-0 packets are preserved in `../evidence/`: `phase0-evidence.md`, `rung1-design.md`, `rung1-first-screen.html` and `rung1-first-screen-light.html`. Root verified published release/tag, Vercel project/domain/source identity, READ-flag and limit semantics, and inspected the comp in both themes at desktop/mobile widths. The comp was shown inline and opened in Dia. It is illustrative and uses a system font fallback; no runtime data or exact production-font rendering is claimed.

Root baseline checks passed: 10 site-script tests; docs validator reports 262 OpenAPI paths. No full site build or runtime demonstration has run. S2 remains source-only because the default demo ignores explicit MXR path overrides for its home-derived demo paths/marker; an existing fully isolated fixture path must be established before claiming observed output.

One async editorial question asks BK to use the proposed direction or revise wording/visuals. Await that answer before central homepage implementation, as requested in the continuation. Then dispatch the first implementation slice; do not start dependent stage code yet. Detailed task IDs, resource sizes, SHA/tree and next actions are in the durable ledger. No resources deleted; control npm install retained for upcoming build/review.

## BK approved the first screen; two bounded tasks are active

BK approved existing-agent visitors first, the working-mailbox concept and “Put your whole mail history to work”. The concrete review gate is satisfied. BK also requires local mailbox ownership and daemon depth enabling client breadth. D-WEB-025–027 supersede the planning-only authority and proposal status above. The Aaron Francis reference is investigated in [the research note](../../research/aaron-francis-mxr-website-2026-10-23.md), with timestamped primary links.

Rung1 implementation runs in `/Users/bhekanik/.t3/worktrees/mxr/t3-830354f4` under task `mxr-website-rung1-implementation-20261023-r1`; S2 isolated proof runs separately in `/Users/bhekanik/.t3/worktrees/mxr/t3-726e9266` under task `mxr-website-s2-fixture-20261023-r1`. Both use T3 worktree isolation, codex gpt-6-luna high/service-default, verified returned settings. Each starts at parent committed f39ae6657 and reads uncommitted planning via absolute control paths. Root owns review/integration/deployment. No dependent rung code starts before rung1 deploys.

Exact next action: collect candidate and proof packets, review the whole diff and claims against pinned source, integrate only the reviewed candidate, run required checks and deliver through the existing mxr.sh project. Record any unmet evidence gate honestly. No deletion is authorised; all original planning/child source and caches remain owned in the durable ledger.

### S2 now has an observed result

Integrated worker evidence at control commit18b39c2cb8688e625f3049dfd8b6dd6d56c73dec. The v0.6.156 release checksum matched; two isolated fake accounts each held55messages and returned4eligible subscription buckets at limit200. Unscoped subscriptions selects the configured default account, so the public example must use explicit account commands. Root checked the rawJSON/count/search identity, archive checksum and source default-account handler at f39ae6657. The worker-owned daemon exited and socket is absent; no real mail was used. Raw small artifacts are copied under durable ledger s2-artifacts; the downloaded binary/data remain in the retained child fixture. Rung1 worker received this context during its active task; no duplicate worker or review round was created.

### Rung1 is reviewed locally and ready for its PR

The task-first revision and exact built-font layout checks are recorded in [rung1 evidence](../evidence/rung1-implementation.md). Root checked390/1440px in both themes with full compiled CSS and the matching Recursive asset; no horizontal overflow. Final hosting build passed10tests,262OpenAPIpaths and208pages. All36homepage internal links/anchors resolve. Only a trivial fixed row gap followed the reviewed candidate.

Exact next action: commit the integration evidence/spacing correction, refresh remote base, pre-push review, open/link the scoped PR, verify its exact tree/checks, merge and smoke the existing mxr.sh deployment. Rungs2–5 remain planned; do not begin dependent code yet. The external comprehension study remains unobserved. Resources stay retained without deletion authorisation.
