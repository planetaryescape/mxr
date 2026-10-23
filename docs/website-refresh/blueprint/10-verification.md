# 10: A successful build must be followed by a successful reader journey

This blueprint is a planning deliverable. Its current checks validate internal consistency, links, evidence references and page coverage. Website commands and acceptance journeys below are requirements for implementation; they are not claimed as already executed. (D-WEB-018, D-WEB-021, D-WEB-024)

## Reader outcomes define completion

| Outcome | Test | Pass condition |
|---|---|---|
| Understand category | Show the first screen without coaching | Visitor describes a local mail runtime, not only a TUI or an AI chat client |
| Answer connector objection | Ask why they might install mxr when Gmail access already works | Visitor identifies local synced history and common programmable state/workflows; does not repeat an unsupported connector limitation |
| Start safely | Follow primary CTA and tutorial | Visitor reaches synthetic mail and a useful local result without connecting real mail |
| See shared work | Follow agent tutorial | Saved draft ID/account/revision matches in the human client |
| Bound authority | Use the tutorial's MCP profile | Prohibited send is refused and no delivery occurs |
| Use human modes | Follow Your day | Visitor picks the appropriate view/action for reply, task, update, reading and record |
| Find a task | Start from a relevant search/deep link | Visitor reaches exact instructions/reference without returning to the homepage |
| Understand model choice | Read model setup and agent route | Visitor distinguishes external agent from native features and can find the cloud disclosure |

For S4, three unfamiliar readers are a proposed acceptance sample, not a statistically validated study. Record where each hesitated and what they actually said; do not turn opinions into a conversion metric. BK can choose a different review method under Q7 without adding telemetry.

## Source checks are bounded and repeatable

Before drafting or resuming implementation:

```bash
git fetch origin main
git rev-parse origin/main
git merge-base HEAD origin/main
```

If SSH fails as it did during this audit, use the existing authenticated HTTPS route without editing the remote. Compare the affected paths, retain the new baseline in the evidence register and rebase the implementation branch safely before pushing. Do not switch/reset a dirty main checkout.

Run the blueprint check from the repo root:

```bash
python3 -I /Users/bhekanik/.dotfiles/.skills/blueprint/scripts/check_blueprint.py   docs/website-refresh/blueprint
```

Also compare all authored source paths to the page audit, verify every proposed navigation target exists or is explicitly new, and inspect every changed source link at its named SHA. The blueprint checker resolves internal Markdown links/decision IDs; it does not check website routes or run CLI examples.

## Use the existing site build and generators

From `site/`, install existing locked dependencies when needed and run the real build through the shared queue:

```bash
agent-work run -- npm ci
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4   NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 npm run build
```

Keep one heavy command per queue job; no nested `agent-work run`. Cargo inside that job follows the existing wrapper's direct-child behaviour. Preserve the worktree-specific target directory and sccache. Do not add a project package manager or framework migration.

`npm run build` already invokes CLI/OpenAPI generation, script tests, selected docs validation, Astro and machine-document generation. Review generated diffs and do not hand-edit output artifacts. When HTTP/OpenAPI contracts change, run the existing spec check against live source separately through the queue; a hosting fallback to a committed spec is not conformance proof.

For this planning-only change, no Rust or web-app source changed, so a daemon rebuild or full test matrix would not validate the user's outcome. Implementation should run focused tests if it changes a generator or interactive component, not add tests that assert a marketing paragraph verbatim.

## Inspect the built site in a browser

After build, launch the existing static preview outside the build queue:

```bash
npm run preview -- --host 127.0.0.1 --port 4321
open -a Dia http://127.0.0.1:4321
```

Use the available collaborative browser for inspection, following tool ownership rules. Inspect mobile/tablet/desktop at 390, 768 and 1440 px, light/dark, reduced motion, keyboard-only navigation and no-JS fallback for the new proof. Check copy controls, media controls/posters, focus order and that exact flags/quotes survive rendered output.

Check the design gate in [03](03-design-concept.md). The proof must show one continuing object, not a disconnected montage. Never open a fake checkpoint as if it were the working product.

## Generated and retained URLs need their own pass

For the changed routes, inspect built HTML plus `/index.md`, relevant `.md` siblings, `/llms.txt`, `/llms-full.txt`, social image, sitemap and existing redirect targets. Follow sidebar links, on-page fragments, related pages and footer routes. Check the conditional new recipe is absent from navigation unless it shipped.

The existing `validate-docs.mjs` catches selected known problems and duplicate content IDs. It is not a general internal-link checker or proof that all examples work. Use an existing crawler if available, or add a small focused Node route/anchor check if the changed site lacks one; do not call a grep pass a complete crawl.

## Demonstrations use the installed contract and synthetic state

S2 and S3 record the binary version, source/release comparison, isolated profile paths, account selectors, selected IDs and outputs. Do not use BK's real active profile or change his sticky demo selection. Build the worktree binary if necessary through the queue, then run persistent daemons outside the queue.

For newly promoted snippets, copy from rendered docs into the isolated profile and verify the output shape and selection. A CLI-help snapshot proves argument parsing, not the resulting mail operation. A dry run proves its preview; a verification read proves state. Model quality needs separate evidence beyond canned demo replies.

## A released rung includes the deployed page

Use the current site's actual deployment pipeline. Record the deployment URL, source SHA and the release/binary version whose behaviour is described. Repeat the critical entry and next-step paths on that deployment, including Markdown and social-image URLs. If deployment protection blocks inspection, say what source/build checks passed and keep deployed acceptance pending.

Do not mark a rung complete because Vercel or npm returned success. It is complete when its reader outcome and design gate are observed. No public deployment is part of the present planning task.

Unresolved questions: Q7 proposes the review method; S1–S5 define the pending evidence checks.
