# 13: Editorial choices have defaults; evidence gaps gate the claims they affect

The original planning questions and execution evidence are recorded separately. BK approved Q1–Q3 after reviewing the concrete first-screen comp: “Use this direction”. The table preserves the original questions; Q4–Q7 remain implementation assumptions. See D-WEB-025–027 for execution authority and the ownership emphasis.

## Product questions for BK

| # | Question | Current assumption | Blocks |
|---|---|---|---|
| Q1 | Should the first-screen journey prioritise people who already use an agent over the older terminal-power-user ordering? | Approved by BK; human use remains a full parallel route. | Resolved |
| Q2 | Does “The working mailbox”, showing one task across agent and human clients, express the product well? | Approved by BK; refine the current site design. | Resolved |
| Q3 | Should the first screen use “Put your whole mail history to work” with “A local mail runtime for you and your agents”? | Approved by BK; use the reviewed headline and category. | Resolved |
| Q4 | Should the site retain Astro/Starlight and its public paths? | Blueprint assumes yes. Existing deep links continue to work. | No blocker under this assumption |
| Q5 | Should public docs describe the released install path rather than preview-branch behaviour? | Blueprint assumes yes. Integrate the stored-assessment explanation only after merge and release verification. | Any branch-specific public claim |
| Q6 | Should new users start with synthetic demo mail and connect their existing agent before exploring the built-in assistant? | Blueprint assumes yes; install and direct non-agent use remain visible. | No blocker under this assumption |
| Q7 | Can acceptance use direct comprehension checks without adding a visitor analytics service? | Blueprint assumes yes. | No blocker under this assumption |

## Spikes settle evidence without exposing real mail

Record the exact commands, version, source SHA, output and conclusion. Use synthetic data. Do not run a real send, unsubscribe, account connection or mailbox mutation to make the website demo convincing.

| # | Question | Method and required result | Blocks | Owner |
|---|---|---|---|---|
| S1 | Which claims belong to the published install and which exist only on main or the preview branch? | Fetch main, inspect release/tag and artifact version; compare relevant handlers and confirm preview deployment SHA if accessible. Record released/main-only/branch-only per claim. | Rung 1 copy; later claims as they ship | Implementing agent |
| S2 | Does the proposed first demo show a useful local result with complete, clearly described coverage? | Observed with published v0.6.156 in an isolated fake profile: 55 messages and four eligible buckets per account; separate explicit account commands required. See [runtime proof](../evidence/s2-runtime-proof.md). READ flags are not actual opens. | Resolved for the captured result; later tutorial must use its own fixture | Implementing agent |
| S3 | Can an agent create a draft visible in a human client, and does the demonstrated permission refusal occur? | Observed in the isolated v0.6.156 MCP fixture: read-only save refusal, draft-only revision 2, the same draft/account/body in the web client and a send refusal. See [S3 proof](../evidence/s3-shared-draft-proof.md). This was deterministic protocol replay, not live agent reasoning. | Resolved for the recorded protocol/UI result; no live-host claim | Implementing agent |
| S4 | Do new visitors understand the category and choose a useful next step? | Show the comp to three people unfamiliar with mxr. Ask them to explain the connector distinction and locate the safe trial. Observe; do not teach during the attempt. This remains unobserved. The website releases do not establish visitor comprehension or conversion. | Evidence for the proposed reader-outcome claim remains open | BK and implementing agent |
| S5 | Do generated outputs and retained routes carry the final story after a real build? | Inspect HTML, .md siblings, llms.txt, llms-full.txt, social image and redirect targets; follow complete reader journeys. | Each affected rung; rung 5 final pass | Implementing agent |

## The supplied preview does not establish released classifier behaviour

The audit fetched `origin/feat/classifier-assessment-r2` at `2053f18ed406b981ad56639b64a29703b4fd9a7a`. Its email-modes source differs from main in the stored-home explanation. The browser URL redirected to Vercel authentication, so its deployed commit was not established. S1 must settle that before publishing its exact behaviour. The blueprint can already explain the five shipped modes without this dependency.

## The questions do not authorise unrelated work

Do not add hosted MCP, a team mailbox, automatic email-derived agent instructions, or a new classifier to close a documentation gap. If a demonstration needs behaviour the runtime lacks, change the demonstration or file a scoped issue. (D-WEB-006, D-WEB-018, D-WEB-023)

## Execution preserves the remaining evidence limits

Q1–Q3 are approved and Q4–Q7 retain the documented defaults. S1 was checked against published v0.6.156 for the early fixtures and v0.6.159 for current source; at final integration, `crates/`, Cargo manifests/lock and `apps/web` match the v0.6.159 tag `e98f499f8bf9072077a693e3e72d23528fe24fa2`. S2 and S3 remain separate old synthetic observations. S5's combined CI, exact-source preview and [final production checks](../evidence/rung5-production.md) passed. S4, a live Codex-host/model run and interactive phone rendering remain unobserved. They limit those claims; they are not silently reported as passed.
