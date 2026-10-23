# 00: The website must make the local mail runtime understandable and usable

The first user is BK. The public reader is a technical person deciding how to put their mail to work: with an existing agent, through a script, or directly in a human client. They should understand why mxr is worth installing even when their agent already has Gmail access. (D-WEB-001, D-WEB-002, D-WEB-007)

## mxr gives mail history a common operating layer

mxr synchronises connected accounts into a local store, provides search and mail operations over that store, and puts human clients, scripts and agents on the same daemon. Provider adapters absorb differences underneath. Five modes and Now turn the same mail into conversations, tasks, briefings, reading and records. The site must explain the relationship between those capabilities. (D-WEB-003 through D-WEB-005, D-WEB-016)

The existing site's verified foundation is substantial. The task is to bring its explanation up to the product, not to invent missing capabilities. The [research](../../research/website-refresh-2026-10-23.md) records code evidence at `origin/main` `f39ae66578e3099e464967ab2f042e24b1ad8ec4` and the [page audit](../../research/website-page-audit-2026-10-23.md) assigns every authored page a disposition.

## A visitor should leave with a result and a correct mental model

1. Recognise “local mail runtime” and understand it in ordinary language.
2. See a result that benefits from locally available history, such as ranking subscription candidates using read state.
3. Understand how existing agents, scripts and human clients use the same mailbox.
4. Try a synthetic mailbox without granting provider access.
5. Choose a human or agent route, and understand the review boundary before enabling mutations.
6. Find the right mode, guide or reference when the task becomes specific.

The intended feeling is control with evidence: “I can see where my mail lives, what this operation used, and what happens next.” The design concept is “The working mailbox”, proposed by the blueprint and subsequently approved by BK. [03](03-design-concept.md) specifies the experience. (D-WEB-012)

## The scope is the whole docs website and its public entry points

| Surface | Required change | Spec |
|---|---|---|
| Homepage | Category, connector answer, one task proof, modes and safe trial | [05](05-homepage.md) |
| Onboarding | Surface-neutral demo and two complete routes | [06](06-onboarding-and-agent-journeys.md) |
| Guide navigation | Reader jobs, canonical explanations, every current page accounted for | [07](07-guides-and-navigation.md) |
| Trust and model docs | Actual enforcement, disclosure and failure boundaries | [08](08-trust-and-models.md) |
| Metadata and machine docs | Consistent category, generated outputs, media and source ownership | [09](09-discovery-and-maintenance.md) |
| Root project docs | Align current pitch and website pointers; preserve historical plans | [09](09-discovery-and-maintenance.md) |

Keep Astro/Starlight, current URLs and useful technical depth. Do not change runtime code in a website refresh merely to make a demonstration possible. No new telemetry, provider, hosted service, team inbox or brand replacement is in scope. (D-WEB-010, D-WEB-011, D-WEB-020, D-WEB-023)

## Use the same nouns everywhere

| Term | Meaning on this website |
|---|---|
| Local mail runtime | The daemon, synced local mailbox and programmable mail capabilities |
| Mailbox | The user's connected accounts as available through one runtime; not a team collaboration product |
| Account | A configured provider account; retain its identity and permission scope |
| Mode | A workflow for mail: Messages, To do, Updates, Reading or Archive |
| Now | The front page of work that needs attention across modes |
| Inbox | Arrival-order mail, not a synonym for every workflow |
| Agent | An external agent using CLI/MCP or another supported interface |
| Assistant | mxr's built-in agent interface, configured separately |
| Preview | What the named operation would affect; not a universal guarantee of no side effects across all commands |
| Archive | Capitalised: records mode. Lowercase `archive`: mailbox/provider action |
| Local | On the runtime's host; remote hosting changes which computer that is |

## Three editorial choices are approved; four defaults remain

Agent users lead the entry journey; the current design is refined through the working-mailbox concept; the approved headline and category lead the opening; the stack and routes stay; released behaviour is the public default; demo mail and an existing agent lead onboarding; acceptance adds no analytics service. [13](13-open-questions.md) defines Q1–Q7 and what each affects.

Unresolved questions: Q1–Q3 are approved; Q4–Q7 have explicit defaults. S1–S5 gate the claims and demonstrations they test.
