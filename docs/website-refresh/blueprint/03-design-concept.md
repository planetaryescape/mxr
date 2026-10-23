# 03: Show the working mailbox moving from evidence to reviewed action

BK approved “The working mailbox” after reviewing the concrete first-screen comp. Keep the current site's restrained blue surfaces, Recursive typography, rules and code. Give that vocabulary a single object to explain: the mailbox people and agents work on together. (D-WEB-005, D-WEB-010, D-WEB-012; Q2)

## The visitor is evaluating a tool they may trust with mail

They arrive with a plausible alternative: their agent already reads Gmail. They need to see a useful difference, where the data lives, and how their own work fits. The emotional sequence is recognition, evidence, control, then a small safe trial. The first screen should feel specific enough that the visitor can explain it to another person.

## The concept makes one object persist

Use a working surface with one task, one result and one continuing identity. For example: a subscription ranking is computed from local read-state aggregates; an agent selects candidates; the person reviews a proposed change. Or an agent saves a draft and the person sees that same draft in the web app. The runtime is the continuous line through those moments.

The hero is the result and its relationship to the user's mailbox. The architecture diagram supports it. A generic chat transcript or an animated terminal full of unrelated commands cannot carry the first explanation.

## The site has one map across all entry points

```text
UNDERSTAND          TRY                  WORK                   BUILD
Why mxr ───────► Install + demo ────► Now + five modes ────► Recipes
    │                   │                    │                  │
    └─ Trust            └─ Existing agent ───┴─ draft review     └─ Contract/reference
```

The same nouns survive every handoff. A recipe links to its exact mechanics and its trust boundary. A mode guide links back to the human overview. The agent tutorial links to the same draft guide the human compose journey uses. Settings and troubleshooting remain reachable through navigation and search; they do not compete with the first useful task.

## Four journeys must feel continuous

| Journey | What persists | Moment that proves the concept |
|---|---|---|
| Connector sceptic → Why mxr → demo | The reason for local history | A real aggregate, its scope and its underlying evidence |
| Existing agent → MCP/CLI → draft → human client | Draft ID, account and revision | The human sees the exact saved draft, awaiting review |
| Human reader → Now → Messages/To do/Reading/Archive | Same account identity and message context | The right action changes with the job without losing the source email |
| Builder → recipe → JSON/reference → verification | Selection, account scope and operation result | A workflow can be composed and checked without provider-specific plumbing |

## Use hierarchy to keep the promise legible

Lead with one category sentence, one outcome headline, one compact connector answer and one safe next action. Keep the install command within the opening area or immediately after it. Put architecture internals, exhaustive providers, tool counts and configuration below the proof or behind an appropriate link.

The five modes are a calm horizontal sequence on wide screens and a stacked sequence on narrow screens. Each has a different job and action, not five identical feature cards. The same-mailbox diagram is a simple account → runtime → clients map with clear ownership boundaries.

## The visual language already exists

| Role | Starting values | Use |
|---|---|---|
| Dark base/surface | `--ink #071522`, `--ink-soft #0d2234` | Reading and a distinct proof surface |
| Main/supporting text | `--paper #f4f9fc`, `--paper-soft #c7d8e5`, `--paper-mute #8fa8bc` | Argument, detail, provenance respectively |
| Active/action | `--signal #57d5ff` | Links, active step and selected evidence |
| Review | `--quiet #ffd166` | A proposal awaiting the person's decision |
| Light counterparts | Existing light-theme overrides in custom.css | Same meaning and hierarchy, reviewed independently |
| Typography | Existing Recursive Variable family and role axes | Display for the outcome, body for explanation, mono for exact operations |
| Proposed scale | Body 17–18 px; small 14 px; section 28–36 px; hero fluid 40–68 px | Long-form legibility without an oversized slogan |
| Proposed spacing | 8 px base; 16/24/32 px groups; 64–96 px section gaps | Separate ideas through spacing and rules |

Reuse actual site tokens. `--paper-faint` is not suitable for body text. Avoid introducing another font merely because stale PRODUCT.md refers to IBM Plex: current CSS imports Recursive. Use the project's icon conventions; remove emoji icons in touched homepage proof rows. Screenshots must use synthetic mail. No invented testimonials, decorative gradients, glow or robot imagery.

## Key moments make control visible

| Moment | Intended feeling | Interaction and fallback |
|---|---|---|
| First open | “I understand the category.” | Literal category is visible before any animation or interaction |
| Inspect local evidence | “This result has a basis.” | Select a workflow step to highlight the relevant store, result and boundary; all text exists without JS |
| Draft handoff | “My agent's work is here for me.” | Same synthetic subject/account/ID continues into a real capture of human review |
| Copy a command | “I can try this.” | Existing copy action says Copied without changing button width; expose original text |
| Permission refusal | “The rule was enforced.” | Show exact refusal and the profile that caused it; next action is review configuration, not automatic escalation |
| First sync is incomplete | “I know what this result covers.” | Show sync status and limits beside the result rather than a false zero-work success |
| Model is off | “The base product still works.” | Separate optional synthesis from core mail and show a no-model path |
| Empty mode | “Nothing is owed here.” | Use the mode's actual completion meaning; do not turn every empty view into an AI upsell |

## Motion explains state and can disappear

Use a click-controlled sequence, never an auto-advancing sales carousel, for the new task proof. A selected step changes emphasis in 120–180 ms; no scrolling hijack, parallax or continuous animation. Reduced motion uses immediate changes. Captures have a poster, controls, captions/transcript and a pause path. If a recording is kept from the existing carousel, it must not contradict the new surrounding story.

## The first screen has an intentional composition

```text
mxr                                    Docs  Why mxr  GitHub
A local mail runtime for you and your agents.
Put your whole mail history to work.

A Gmail connector gives your agent access to Gmail.
mxr gives you a synced local mailbox and mail operations
that you, your scripts and your agents can use together.

[Try the demo]  [Use my agent]       $ brew install ... [copy]

ONE SYNTHETIC TASK: local evidence → proposal → human review
[1 Evidence] [2 Agent work] [3 Review]     same mailbox boundary
```

The wireframe is a proposed hierarchy, not final approved copy or a claim that a new component is built. The demo action must land on a real runnable tutorial section. On mobile the outcome precedes the proof; do not squash a desktop terminal screenshot until its text is unreadable.

## A guide feels like the next step in that same product

```text
Breadcrumb      On this page
Use your agent with demo mail
Outcome, prerequisites, one safe route
1 Start the isolated mailbox
2 Attach the tool with a bounded profile
3 Read local evidence
4 Save a draft and inspect it in your client
What you should see / when it fails
Continue: modes | trust | exact tools
```

Guide pages use a readable column with exact commands and results, not homepage-scale marketing sections. A reader should always know whether the block is a command, captured output, illustrative output or explanatory copy.

## The design gate checks the relationship, not just the layout

Before implementing the central homepage comp, show BK the first screen at mobile and desktop widths in both themes, with the full task sequence and its no-JS fallback. This is the blueprint's proposed review gate; the current planning task does not pause for that approval.

For each implemented rung: inspect 390, 768 and 1440 px widths, keyboard focus and reading order, reduced motion, copy feedback, media controls, contrast and the four complete journeys. Confirm the main result dominates and that proof, trust and next action are adjacent. A page that technically fits but gives every feature equal weight fails.

BK supplied no new external taste references for this website. Reuse this repository's current visual vocabulary and the cited product constraints; do not attribute an invented preference or competitor aesthetic to him.

Unresolved questions: Q1–Q3 are approved; S4 tests whether the explanation is understood by new readers.
