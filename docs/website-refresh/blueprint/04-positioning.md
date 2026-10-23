# 04: Explain what local ownership and a common mail runtime let someone do

The message is a chain of cause and consequence: synced history enables local queries and aggregates; common mail operations hide provider differences; the daemon lets a person, script and agent use those operations against the same state. The human modes make that state useful in daily life. (D-WEB-001 through D-WEB-005, D-WEB-016)

## The first paragraph answers the connector question

Entry direction approved by BK; refine the supporting copy during implementation:

> A Gmail connector gives your agent access to Gmail. mxr gives you a synced local mailbox and mail operations that you, your scripts and your agents can use together. Search across connected accounts, work with local history and aggregates, and review an agent's drafts in your own mail client.

The category sentence immediately beside it is “A local mail runtime for you and your agents.” This distinguishes the operating model without assuming every connector has the same account limits, cache architecture or tool inventory. The last phrase means mxr's human client; use “in mxr's web app or TUI” in copy where “your own mail client” could imply universal third-party draft sync. (D-WEB-009, D-WEB-013)

The Why mxr page should give the direct answer before commands, architecture or a provider matrix. Keep the answer short enough that a reader does not need to expand a detail panel to understand it.

## Owning the mailbox means being able to work with it locally

BK explicitly wants local ownership to lead the argument. Say what is on the user's computer and what that enables: search and inspect synced mail, calculate local aggregates, and combine mail operations with the user's own tools. The remote mail server still receives synchronization, sends and supported mutations; mxr does not replace the provider. Initial backfill, cached bodies and attachment availability define what can be used without a connection. The headline can speak about the whole history while the supporting sentence makes the sync condition clear. (D-WEB-027)

Candidate supporting line: “Your mailbox, on your computer. Still synced with your mail provider, ready for your tools.” Explain cached-content limits beside the first offline example rather than burying them in a generic disclaimer.

## The daemon does the mail work once so clients can reuse it

The CLI, web app, TUI and agent surfaces should read as ways into one product. Provider adapters, sync and common mail operations belong beneath them. A new client can reuse those capabilities and focus on its own interaction design. Do not claim every screen is trivial or every client has identical authority. This is an architectural benefit, not a measured development-time claim. (D-WEB-005, D-WEB-027)

The [Aaron Francis research note](../../research/aaron-francis-mxr-website-2026-10-23.md) connects this direction to his thin-client example. Use the principle to shape the explanation and reduce reader work; a motivational quotation is not needed on the homepage.

## Six reasons form one product story

| Reader benefit | Concrete example | Why the runtime matters | Evidence |
|---|---|---|---|
| Work over synced history | Rank subscription candidates by read rate, or inspect storage by sender | Aggregates are computed against local mail; the model need not read every raw body to calculate a count | F01, F02, F04, F05 |
| Keep accounts in one operating model | Search Gmail and an IMAP account together, then keep account scope when acting | Provider adapters translate differences underneath the common model | F03, F06 |
| Create workflows mxr did not package as a feature | Inspect invoice attachments, choose files, produce a manifest, then review proposed filing | Search, attachment access, export and previews compose with local tools | F03; full recipe requires S2 and fixture replay |
| Share work with an agent | Agent saves a draft; person reviews the same draft in web/TUI | Both use daemon-backed state rather than a separate agent-only mailbox | F07, F10, F12; observed handoff requires S3 |
| Choose how much authority to grant | MCP can prepare drafts but a send request is refused | The daemon checks an identified client's policy before dispatch | F06, F07, F10 |
| Use mail according to its job | A reply in Messages, a due bill in To do, a newsletter in Reading, a booking reference in Archive | Modes provide different views and actions above the common mailbox | F13 through F17 |

These reasons are linked, not a list of unrelated superpowers. Show at least one compound example: select locally, prepare work with an agent, review through a human surface. Keep the architecture underneath that example. (D-WEB-005, D-WEB-012)

## The agent is a choice, not an account migration

A shell-capable agent can use the CLI with instructions from the skill; an MCP-capable agent can use typed tools. The built-in assistant is another option over the runtime. Native summarisation, drafting and related model features have their own mxr configuration. Core mail does not require those features to be enabled.

Do not promise that any brand of agent runs every mxr capability identically, that remote hosted agents automatically reach a local socket, or that a subscription to an external agent covers native API calls. Show a small, verified setup for each named host and link to its current instructions. (D-WEB-008)

## Five modes explain how a person inhabits the runtime

The homepage and Why mxr should give a coherent human example alongside agent work:

1. Start in Now to see what needs attention.
2. Reply in Messages or act on a dated item in To do.
3. Glance at the Updates briefing and let the batch go.
4. Read chosen newsletters as an edition, save something for later.
5. Find a receipt or reference in Archive when needed.

A single conversation may carry more than one job. Do not imply a one-folder-per-thread model. If stored email homes ship from the preview branch, distinguish each email's home from the conversation's outstanding jobs. (D-WEB-016, D-WEB-018)

## Give connectors an honest place

A connector can be the convenient choice when someone wants a supported app tool inside an agent and does not want to run local sync or manage a local mailbox. mxr asks them to install software, store mail locally and let initial synchronization complete. The advantage must justify that work.

Keep the comparison in terms of fit: who owns the corpus, which runtime supplies consistent operations, how human review connects, where computation happens, and what policy is enforced. Current vendor-specific details belong in the dated research note unless the copy genuinely needs them. No adversarial matrix is required. (D-WEB-009)

## Copy exclusions prevent the old misunderstanding returning

Do not lead with terminal email, AI-native, all-in-one, a chat interface or one model's name. These obscure the category. Do not imply raw SQL is the normal agent interface: the daemon's APIs carry scope and mutation policy. Do not claim perfect triage, guaranteed personal voice, no data ever leaving the computer, every attachment cached, all actions logged or universal undo.

A current, bounded example is stronger than an inflated number: do not reuse “200 operations”, a canned result time, or a token-savings percentage without a maintained count or measurement. Credit prior mail tools where relevant, but avoid adding an unrelated lineage redesign to this refresh. (D-WEB-019, D-WEB-023)

Unresolved questions: S4 checks whether the approved explanation is understood. Q1–Q3 were approved by BK; S2/S3 still gate observed examples.
