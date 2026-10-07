# Docs inventory for the email modes

Every page on the docs site (`site/src/content/docs/`) and every repo doc
that describes the product, sorted by when the email modes plan
([blueprint/22-email-modes.md](blueprint/22-email-modes.md)) changes it.
The rule behind every row: the site documents what ships. Philosophy and
positioning change now (phase 0); a feature page changes in the phase that
ships the feature, and the phase's Docs line in 22 names it so the release
can't go out without it.

Audited at `2fec6686` on `docs/email-modes`, whose code is v0.6.47
(`3da0c119`, also `origin/main`).

## Categories

| Code | Meaning |
|---|---|
| R | Philosophy or positioning: rewritten in phase 0 |
| P1 to P7 | Feature page that changes when that phase of 22 ships; the first phase listed is the first that touches it |
| O | Becomes wrong or obsolete under the model; the phase that retires or replaces it is named |
| U | Unaffected by the modes |

A page can be R and still change later; the table gives its phase 0 status
and the later phases in the notes.

## Counts

| | R | P | O | U | Total |
|---|---:|---:|---:|---:|---:|
| Site pages (`site/src/content/docs/`, excluding generated CLI pages) | 8 | 34 | 3 | 20 | 65 |
| Repo docs and agent files (table rows; a few rows group several files) | 9 | 15 | 1 | 21 | 46 |
| Marketing files (listed, not rewritten) | 0 | 4 | 0 | 13 | 17 |

The site count includes the new `guides/email-modes.md`. The generated CLI
reference (`reference/cli/<command>.md`, built from `--help` snapshots by
`site/scripts/generate-cli-reference.mjs`) is not counted: it regenerates
in every phase that adds a command.

## Site pages

| Page | Category | What changes, and when |
|---|---|---|
| `index.mdx` | R | Phase 0: a section on sorting mail by what you do with it, today's commands, and a link to the modes page. P2: the desk capability becomes Now; recordings re-shot once the rail is the modes. P6: PDF prefetch ends "downloads an attachment into its cache when you open it" |
| `guides/email-modes.md` | R | New in phase 0: the model, today's status per mode, the plan. Every phase updates its status table |
| `guides/why-mxr.md` | R | Phase 0: the fit now says mail is sorted by what you do with it, with the modes as the plan |
| `guides/glossary.md` | R | Phase 0: an Email modes section with each unbuilt term marked planned; the decision log range fixed (D001 to D114). Every phase drops "planned" from the terms it ships |
| `guides/llm-features.md` | R | Phase 0: what each request sends and where, every override key, the planned tiers. P1: `llm.tiers` ships; P7: background classification and its opt-in |
| `guides/security-and-privacy.md` | R | Phase 0: what else reaches the network, model features and cloud, what deletion removes today. P1 and P7: tier opt-ins; P5: article fetch contacts the article's domain; P6: PDF prefetch for records. Deletion table changes when `fix/delete-derived` lands |
| `guides/your-day.md` | R | Phase 0: a note that this is the v0.6.47 workflow. P2: rewritten around Now; P4 and P6: the Paper trail step becomes Updates and Archive |
| `guides/desk.md` | R, then O at P2 | Phase 0: a note that the desk is Now's first version. P2: replaced by `guides/now.md`, with a redirect from `/guides/now/` |
| `examples.md` | P2 | Desk and Paper trail examples; P4 and P6 move the Paper trail ones |
| `getting-started/first-sync.md` | P2 | Mentions the desk and Paper trail after the first sync |
| `getting-started/gmail-setup.md` | P2 | Mentions the web app opening on the desk |
| `getting-started/quick-start.md` | P2 | Demo tour starts on the desk |
| `guides/activity-log.md` | P1 | New activity kinds per mode verb (to-do done, mode done, let go), ids and counts only |
| `guides/agent-skill.md` | P1 | Quick reference gains `mxr todo`, then each mode's command as it ships |
| `guides/architecture.md` | P2 | Computed mode membership lives in the daemon; principle 7 (rules first) gains "then the user's model" at P7 |
| `guides/archive-intelligence.md` | P6 | `mxr ask` becomes Archive's fallback after a record-field answer |
| `guides/automated-followups.md` | P3 | Waiting on becomes a Messages filter |
| `guides/automation-contract.md` | P1 | New dry-run mutations: `SetTodoDone`, then `SetModeDone`, digest let go, `FileAsRecord`, exports |
| `guides/briefings-and-loop-in.md` | P3 | The person page (`p`) is where a recipient briefing shows |
| `guides/calendar-invites.md` | P1 | Unanswered invites become to-dos; P6: answered invites file in Archive |
| `guides/deliveries.md` | P4 | Deliveries become Updates trackers; P6: delivered orders become Archive records |
| `guides/focus-and-reply.md` | P3 | Focus & reply becomes `g F` inside Messages (and To do from P1) |
| `guides/for-agents.md` | P1 | Worked examples gain `mxr todo`; "What stays local" also needs the background model work (see "Already wrong") |
| `guides/forgotten-work.md` | P1 | Promises become To do rows (`kind = promise`); P3: owed replies become Messages' Your turn |
| `guides/mailbox.md` | P2 | The TUI sidebar becomes the modes rail; Owed moves into Messages |
| `guides/no-native-desktop-app.md` | P2 | Says the web app opens on the desk |
| `guides/recipes.md` | P2 | Owed and Paper trail recipes; P1 adds To do recipes |
| `guides/rules.md` | P7 | Explain how deterministic rules relate to mode placement and corrections |
| `guides/search.md` | P2 | Search gains a mode filter; P6: Archive's answer box |
| `guides/semantic-search.md` | P1 | Recipe table, chunk mode tags, text-hash embedding keys; P2: baseline at sync and stale-only reindex; each mode phase adds its recipe |
| `guides/sender-view.md` | P3 | Becomes the person page in Messages |
| `guides/sound-hints-and-touch.md` | P2 | Low tide and desk swipes move to Now |
| `guides/unsubscribe.md` | P5 | `D` unsubscribes with evidence ("you opened 0 of the last 11") |
| `guides/web-app.md` | P1 | To do rail entry; P2: the rail is the modes; every phase adds its view |
| `reference/bridge.md` | P1 | New routes per phase (todos, modes, now, updates, reading, records) |
| `reference/cli/concepts.md` | P2 | IPC buckets gain the modes requests; search mode filter |
| `reference/config.md` | P1 | `llm.tiers.fast`, `llm.tiers.smart`, `escalate`, lead times; P2 `modes.archive_on_last_done`; P4 digest cuts; P7 `allow_cloud_background_classification` |
| `reference/json-output.md` | P1 | `mxr todo --format json`, then each mode's JSON |
| `reference/keybindings.md` | P1 | `t` makes a to-do; P2: `g` keys per mode, `K`, `X`, `T`, `e` as done here, `g u` moves to Updates |
| `reference/mcp.md` | P1 | Each phase ships its MCP tools after the CLI |
| `reference/time-phrases.md` | P1 | `mxr todo add --due PHRASE` and `mxr todo schedule` |
| `reference/tui.md` | P1 | To do lens; P2: the sidebar becomes the rail; each mode adds its lens |
| `guides/reading-and-paper-trail.md` | O | P4: Paper trail's notifications move to `guides/updates.md`; P5: Reading moves to `guides/reading.md`; P6: records move to `guides/archive.md`, and the page becomes a redirect. P6 shipped `guides/archive.md` before P4 and P5 moved their halves, so the page stays, pointing records at Archive, until they do |
| `reference/desk-and-places.md` | O | P2: replaced by `reference/now-and-modes.md` (Now sections, membership layers, per-mode done, the `mxr why` JSON with every mode) |
| `guides/triage-flow.md` | O | P2: the Screener stops being a destination (an inline question on a first-time sender's row, `/screener` as history); reply later becomes `b` Later in Messages and Reading; P7 retires `mxr triage` |
| `getting-started/imap-smtp-setup.md` | U | |
| `getting-started/install.md` | U | |
| `guides/accounts.md` | U | |
| `guides/adapter-development.md` | U | |
| `guides/analytics.md` | U | |
| `guides/compose.md` | U | Got it (P3) sends through the existing send path; revisit only if it gains a compose surface |
| `guides/crash-safe-drafts.md` | U | |
| `guides/labels-and-saved-searches.md` | U | |
| `guides/linked-drafts.md` | U | |
| `guides/mail-merge.md` | U | |
| `guides/observability.md` | U | |
| `guides/pre-send-safety.md` | U | |
| `guides/public-rust-crates.md` | U | `mxr-reader` (where quote stripping lands in P3) is not a public crate |
| `guides/snippets.md` | U | |
| `guides/timing-and-cadence.md` | U | |
| `reference/adapters.md` | U | |
| `reference/bug-report.md` | U | |
| `reference/conformance.md` | U | |
| `troubleshooting.md` | U | |
| `videos.mdx` | U | The recordings show the v0.6.47 desk; re-record after P2 if they are kept on the site |

`guides/desk.md` is counted as R; it also becomes obsolete at P2.

## The sidebar today and by mode

`site/astro.config.mjs` holds the sidebar. Phase 0 adds "Email Is Five
Apps" under Concepts and nothing else, because every other group still
describes shipped behaviour.

When phase 2 ships (Now, the rail, membership), reorganise by mode, adding
each mode's group in the phase that ships it:

```text
Start Here          Installation, Quick Start, Examples, Video Gallery,
                    Gmail Setup, IMAP / SMTP Setup, First Sync
How mxr Works       Email Is Five Apps, Why mxr, Glossary
Now                 Work Through Your Day (P2), Now (P2, was Clear the Desk)
Messages            Messages (P3), Reply to Everyone You Owe (P3, as g F),
                    Sender View (P3, the person page), Compose,
                    Linked Gmail Drafts, Pre-send Safety, Snippets, Mail Merge
To do               To do (P1), Promises (P1, from Forgotten Work),
                    Calendar Invites (P1), Time Phrases
Updates             Updates (P4), Deliveries (P4), Rules
Reading             Reading (P5), Unsubscribe (P5)
Archive             Archive (P6), Search Workflow, Semantic Search,
                    Archive Intelligence (P6, as Archive's fallback)
Everything          Inbox / Mailbox Workflow, Labels and Saved Searches,
                    Triage Flow (until P2 retires its Screener section)
Models and Privacy  LLM Features, Security & Privacy, Activity Log
Power Features      Automated Follow-ups, Timing and Cadence,
                    Briefings and Loop-in, Analytics, Crash-Safe Drafts,
                    Accounts
Concepts            Architecture, For Agents, Automation Contract,
                    Observability
Building on mxr     Adapter Development, AI Agent Skill, Public Rust crates
Reference           CLI, TUI, Keybindings, Now and modes (P2, was Desk and
                    places), Config, JSON output schemas, HTTP Bridge,
                    MCP Server, API route inventory, Bug Reports, Adapters,
                    Conformance Tests
Help                Troubleshooting
```

A mode group appears only when its phase ships. Until then its pages stay
where they are today.

## Repo docs and agent files

| File | Category | What changes, and when |
|---|---|---|
| `README.md` | R | Phase 0: a paragraph on sorting by what you do with mail and the modes as the plan; model features described as off by default, local or your own key. P1 and P2: the commands shown; P6: the attachment download line; P7: "What is local" |
| `AGENTS.md` | R | Phase 0: a product-model line pointing at 22 and the docs rule |
| `.agents/skills/mxr/SKILL.md` | R | Phase 0: today's sorting commands, and a warning not to call unshipped mode commands. Each phase adds its commands |
| `.agents/skills/mxr-development/SKILL.md` | R | Phase 0: a product-shape line naming the modes, the phase order and the Docs line rule |
| `docs/README.md` | R | Phase 0: rows for 22, the research and this inventory |
| `docs/vision.md` | R | Phase 0: a section saying the thesis moved to the modes, with what has and hasn't shipped |
| `docs/blueprint/14-roadmap.md` | R | Phase 0: the modes checklist matches 22's phases, with phase 0 and the docs gate |
| `docs/web-app-experience-rubric.md` | R | Phase 0: criterion X11, docs and READMEs match what ships |
| `PRODUCT.md` | R (not done) | States the brand purpose and "four superpowers" without the modes. Positioning, so it should change, but it is outside the phase 0 rewrite list; BK to decide |
| `docs/blueprint/00-overview.md` | P2 | The original one-line pitch; add a pointer to 22 when the rail ships |
| `docs/blueprint/README.md` | U | Already indexes 22 and its research |
| `docs/blueprint/02-data-model.md` | P1 | `todos`, then `mode_aspects`, `mode_corrections`, `mode_done` and the per-mode stores |
| `docs/blueprint/05-search.md` | P1 | Per-mode index recipes and the mode filter |
| `docs/blueprint/08-tui.md` | P2 | The sidebar becomes the modes rail |
| `docs/blueprint/09-cli.md` | P1 | `mxr todo`, then `mxr modes`, `mxr now`, `mxr messages`, `mxr updates`, `mxr reading`, `mxr records` |
| `docs/blueprint/12-config.md` | P1 | `llm.tiers`, lead times, `modes.*` |
| `docs/blueprint/21-web-experience.md` | O | P2: its desk-and-places rail is superseded by D109; D111 already amends its "no automatic LLM classification" line. Add a superseded note when the rail ships |
| `docs/web-app.md` | P1 | To do route; P2: the rail and routes by mode |
| `docs/web-app-controls.md` | P2 | Persistent-controls inventory for the modes rail (rubric X4) |
| `docs/activity-log.md` | P1 | New activity kinds, ids and counts only |
| `docs/security-audit-rubric.md` | P1 | Cloud smart tier egress; P7: background classification egress |
| `docs/reference/ai-email.md` | P1 | Principles gain the tiers and the verbatim check for extracted amounts and dates |
| `docs/reference/tui-keymap.json` | P2 | Generated by the TUI keymap test; changes with the mode scopes |
| `docs/calendar-email/` | P1 | Invites as to-dos while unanswered |
| `PRIVACY.md` | P1 | The cloud smart tier, then P7's background classification opt-in. Also already wrong on credentials (see below) |
| `ARCHITECTURE.md` | P2 | Computed membership in the daemon |
| `CONTRIBUTING.md` | U | |
| `CLAUDE.md` | U | Not in the repo; `AGENTS.md` is the agent context |
| `DESIGN.md` | U | Visual system for the site |
| `SECURITY.md` | U | |
| `TERMS.md` | U | |
| `TODO.md` | U | |
| `CHANGELOG.md` | U | Owned by release-please |
| `CODE_OF_CONDUCT.md` | U | |
| `crates/core/README.md` | U | Provider traits only |
| `docs/web-app-rubric.md` | U | The parity floor |
| `docs/dogfooding-log.md` | U | A log; phases add entries, not rewrites |
| `docs/implementation-journey.md` | U | History |
| `docs/idiomatic-rust.md`, `docs/idiomatic-rust-tests.md` | U | |
| `docs/triage-session-feedback-2026-06-03.md`, `docs/web-visual-review-2026-10-01.md` | U | Dated records |
| `docs/guides/writing-docs.md`, `docs/guides/http-bridge.md` | U | |
| `docs/articles/` (2 essays) | U | Dated essays |
| `docs/blueprint/` chapters 01, 03, 04, 06, 07, 10, 11, 13, 16 to 20, and the audits | U | |
| `docs/blueprint/15-decision-log.md` | U | Already holds D107 to D114 |
| `docs/research/` | U | Inputs to 22 |
| `docs/reference/` other files | U | |

## Marketing (listed, not rewritten)

| File | Category | Note |
|---|---|---|
| `marketing/launch-thread/thread.md` | P2 | States the philosophy as local history plus agents, with no modes; revisit once a mode view ships worth showing |
| `marketing/launch-thread/concept.md` | P2 | Same framing as the thread |
| `marketing/meetup-deck/concept.md` | P2 | Positions mxr as local history for agents |
| `marketing/meetup-deck/speaker-notes.md` | P2 | Same; mentions model features |
| `marketing/launch-thread/QA.md`, `marketing/meetup-deck/QA.md` | U | Review notes |
| `marketing/video-library/publishing-log.md` and `campaign/*.md` (11 files) | U | Post copy for recorded clips of shipped features |

None of these describe unbuilt behaviour as shipped. They predate the
modes, so the change is an addition, not a correction.

## Already wrong, regardless of the modes

Found during the audit; the first four are fixed in phase 0 because they
sit on pages it rewrites.

- `guides/glossary.md` said the decision log runs D001 to D048. It runs to
  D114. Fixed.
- `reference/config.md` listed 8 of the 14 `llm.overrides` keys
  (`crates/config/src/types.rs`, `LlmOverrides`). Fixed.
- `reference/config.md`'s override example points `answer_coverage` at
  OpenAI, but the daemon refuses relationship-data features on a cloud
  endpoint unless `allow_cloud_relationship_data = true`
  (`relationship_data_block_reason`, `crates/daemon/src/state.rs`). Fixed
  with a note; the TOML example block near the top of the page still shows
  the same override without the setting.
- `guides/llm-features.md` opened with "This page covers", and its feature
  table left out row gists and `mxr triage`, both of which call the model
  through the `summarize` feature. Fixed.
- `guides/for-agents.md`, "What stays local, what doesn't", names only
  `mxr summarize` and `mxr draft-assist` as model calls. Background row
  gists (`handler/thread_gists.rs`) and delivery confirmation after sync
  (`handler/deliveries.rs`, called from `post_sync_fanout`) also reach the
  configured endpoint. Not fixed (P1 page).
- `guides/security-and-privacy.md` said "the network is for talking to your
  provider", leaving out the model endpoint, model downloads, remote images
  and unsubscribe endpoints. Fixed.
- `PRIVACY.md` says IMAP and SMTP passwords are stored in the OS secret
  store. Since the disk-first change they live in `secrets.toml` (mode
  `0600`) with the keychain as an optional mirror, as
  `guides/security-and-privacy.md` says. Not fixed: it is the published
  privacy policy, and BK should approve its wording.
- `PRIVACY.md` "Data Deletion" implies removing the directories is the
  only path; provider deletes already cascade most derived data (see the
  security page). Not fixed, same reason.
- `guides/why-mxr.md` introduced its non-goals with "Use something else
  when you need:" followed by items starting "You want" and "You need".
  Fixed.
