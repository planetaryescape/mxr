# 07: Organise documentation by the reader's job and preserve every useful route

Replace the large “Daily Use” and “Power Features” buckets with task-owned groups. The current content has substantial depth; the refresh mainly changes its entry, ownership and cross-links. Keep existing slugs and the technical reference hierarchy. (D-WEB-011, D-WEB-016, D-WEB-022)

## The sidebar gives each page one primary home

The proposed group labels are implementation choices under Q4. Keep all groups but Start here collapsed initially if Starlight supports the intended behaviour; verify keyboard and mobile navigation. Search remains global.

| Group | Ordered existing or proposed routes |
|---|---|
| Start here | `guides/why-mxr`, `getting-started/install`, `getting-started/quick-start`, **new** `getting-started/agent-quick-start`, `examples`, `videos`, `getting-started/gmail-setup`, `getting-started/imap-smtp-setup`, `getting-started/first-sync` |
| Work with your mail | `guides/your-day`, `guides/now`, `guides/email-modes`, `guides/messages`, `guides/focus-and-reply`, `guides/todo`, `guides/todo-resolution`, `guides/updates`, `guides/reading`, `guides/archive`, `guides/marginalia`, `guides/personal-categories` |
| Search, understand and automate | `guides/search`, `guides/semantic-search`, `guides/analytics`, `guides/sender-view`, `guides/archive-intelligence`, `guides/for-agents`, `guides/agent-skill`, `guides/recipes`, **conditional new** `guides/invoice-workflow`, `guides/rules`, `guides/assistant`, `guides/forgotten-work`, `guides/automated-followups`, `guides/deliveries`, `guides/timing-and-cadence`, `guides/briefings-and-loop-in` |
| Write and organise | `guides/compose`, `guides/linked-drafts`, `guides/crash-safe-drafts`, `guides/pre-send-safety`, `guides/snippets`, `guides/mail-merge`, `guides/calendar-invites`, `guides/labels-and-saved-searches`, `guides/triage-flow`, `guides/unsubscribe`, `guides/reading-and-paper-trail`, `guides/mailbox` |
| Clients and accounts | `guides/web-app`, `guides/desktop-app`, `guides/raycast`, `guides/accounts`, `guides/sound-hints-and-touch`, `guides/remote-mailbox`, `guides/headless-linux` |
| Control and maintain | `guides/security-and-privacy`, `guides/automation-contract`, `guides/llm-features`, `guides/activity-log`, `guides/backup-recovery`, `guides/pairing-authority`, `guides/observability`, `troubleshooting` |
| Build on mxr | `guides/architecture`, `guides/adapter-development`, `guides/public-rust-crates`, `guides/trace-model-calls`, `guides/glossary` |
| Reference | Existing `reference/cli` generated subtree, `reference/cli/concepts`, `reference/tui`, `reference/keybindings`, `reference/desk-and-places`, `reference/time-phrases`, `reference/config`, `reference/json-output`, `reference/bridge`, `reference/mcp`, API explorer, `reference/bug-report`, `reference/adapters`, `reference/conformance` |

Cross-links may put a page in several journeys without duplicating the page. Do not give generated CLI children a second handwritten navigation inventory.

## The main explanations have different jobs

| Page | Rewrite/revision brief | Completion evidence |
|---|---|---|
| Why mxr | Open with the connector answer; explain local corpus, composability, shared state, modes, trust and fit; include trade-offs | Reader can explain why installing a runtime may be worth it |
| Email modes | Preserve its strong five-jobs explanation; show how shared runtime underpins it; distinguish modes, categories, labels and Inbox | Reader can predict where to reply, act, read and retrieve a fact |
| Architecture | One accounts/runtime/clients diagram including web, MCP and skill accurately; explain provider abstraction before IPC taxonomy | Reader can place their agent and model on the map |
| Your day | A full human journey starting Now, then the five modes and their distinct completion meanings | Reader can finish one task without learning all commands |
| For agents | A practical operating guide: evidence → selection → preview → review → apply when authorised → verify; bounded authority near first mutation | A repeated operation retains account scope and review context |
| LLM features | Separate external agent, built-in assistant and native features; correct “one backend” simplifications if per-feature routing applies | Reader can choose no model, a local endpoint or an explicit cloud route |
| Security & Privacy | Trust boundary and actual data-flow table, paired with operation limits | Reader knows which data stays on the runtime host and what can leave |
| Automation contract | Exact structured-output, preview, operation identity, retry, undo and verification rules | Builder can recover correctly after a lost response |

## Outcome examples and recipes solve different stages

`/examples/` becomes a short selection of useful jobs, ordered by the new story: local analysis, cross-account retrieval, a shared draft, attachment workflow, human modes, then deeper automation. Link each to a full guide and reference; do not repeat every CLI subcommand.

`/guides/recipes/` remains shell-oriented task recipes. Preserve working examples, correct touched mutations to preview before application, and make output shapes explicit. For agents owns agent-led examples. Both link to the same underlying reference.

The invoice workflow is the proposed composability proof beyond mail triage: select synthetic invoice mail, inspect attachment metadata, download only selected files to a temporary directory, make a manifest with source/account IDs, review proposed organisation, verify files and preserve mail state. File-writing actions outside mail are the agent/tool's responsibility, not a new mxr feature. Publish only after S2 replay establishes exact attachment commands and no-clobber handling. Otherwise keep the recipe in planning and use an existing proven attachment example. (D-WEB-003, D-WEB-023)

## Each mode guide teaches its own action

| Mode | Hero object and action | Cross-links that must carry context |
|---|---|---|
| Now | A short attention list; choose the next job | Modes and relevant Messages/To do item |
| Messages | A person/conversation; reply | Compose, linked drafts, relationship context |
| To do | An evidenced dated action; schedule or complete | Later evidence review and source email |
| Updates | A source-grouped briefing; glance and let go | Source settings, deterministic facts and underlying mail |
| Reading | An edition/item; read or save for later | Highlights, article-fetch privacy, Marginalia |
| Archive | A sourced record field/document; retrieve a fact | Archive intelligence and cached-PDF limits |

A person should see why the view has its shape. Do not describe each as the same message list with a different filter. Retain any uncertainty/provenance already present. (D-WEB-016)

## Every authored page has a recorded disposition

The [complete page audit](../../research/website-page-audit-2026-10-23.md) covers each Markdown/MDX source at the audit SHA. It also accounts for the API explorer, legal pages, assets and generated surfaces. The table distinguishes rewrite, focused revision and keep-and-relink; “keep” does not assert every existing sentence has been runtime-tested. During implementation, verify any behaviour changed or newly promoted.

Unresolved questions: Q4 assumes URL preservation and the proposed grouping. The invoice recipe stays conditional on S2, not a requirement to add runtime behaviour.
