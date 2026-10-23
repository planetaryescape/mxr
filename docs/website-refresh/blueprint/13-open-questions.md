# 13: Editorial choices have defaults; evidence gaps gate the claims they affect

The planning task is complete without answers to these product questions. BK approved Q1–Q3 after reviewing the concrete first-screen comp: “Use this direction”. The table preserves the original questions; Q4–Q7 remain implementation assumptions. See D-WEB-025–027 for execution authority and the ownership emphasis.

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
| S2 | Does the proposed first demo show a useful local result with complete, clearly described coverage? | Run a disposable synthetic profile, verify two-account results, subscription candidate limit and READ-flag semantics; capture IDs and output without touching BK's active demo state. | Rung 1 recording; rung 2 tutorial | Implementing agent |
| S3 | Can an agent create a draft visible in a human client, and does the demonstrated permission refusal occur? | Use an isolated MCP draft-only profile, save a known draft, find the same ID in the human client, attempt a prohibited request, inspect applicable activity. Do not send. | Rung 2 handoff proof; rung 4 trust demo | Implementing agent |
| S4 | Do new visitors understand the category and choose a useful next step? | Show the comp to three people unfamiliar with mxr. Ask them to explain the connector distinction and locate the safe trial. Observe; do not teach during the attempt. This is a proposed acceptance method, not a completed study. | Rung 1 acceptance | BK and implementing agent |
| S5 | Do generated outputs and retained routes carry the final story after a real build? | Inspect HTML, .md siblings, llms.txt, llms-full.txt, social image and redirect targets; follow complete reader journeys. | Each affected rung; rung 5 final pass | Implementing agent |

## The supplied preview does not establish released classifier behaviour

The audit fetched `origin/feat/classifier-assessment-r2` at `2053f18ed406b981ad56639b64a29703b4fd9a7a`. Its email-modes source differs from main in the stored-home explanation. The browser URL redirected to Vercel authentication, so its deployed commit was not established. S1 must settle that before publishing its exact behaviour. The blueprint can already explain the five shipped modes without this dependency.

## The questions do not authorise unrelated work

Do not add hosted MCP, a team mailbox, automatic email-derived agent instructions, or a new classifier to close a documentation gap. If a demonstration needs behaviour the runtime lacks, change the demonstration or file a scoped issue. (D-WEB-006, D-WEB-018, D-WEB-023)
