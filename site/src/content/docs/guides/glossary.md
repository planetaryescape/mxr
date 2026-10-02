---
title: Glossary
description: Plain-language definitions for the terms scattered across mxr docs and code. One paragraph each, plus a link to where the concept lives.
---

mxr borrows vocabulary from email standards, Gmail, Hey, and IMAP. This page reconciles them so you don't have to guess.

## Architecture

**Daemon**: the background process that owns the SQLite store, the search index, the network connections, and the IPC socket. The TUI, the CLI, and the HTTP bridge are all clients of the same daemon. `mxr` autostarts it; `mxr daemon` runs it explicitly. See the [architecture guide](/guides/architecture/).

**Adapter**: the per-provider code that translates mxr's internal model to and from a provider's API or wire protocol. Today: `provider-gmail`, `provider-outlook`, `provider-imap`, `provider-smtp`, `provider-fake`. New adapters live in their own crate and pass a [conformance test suite](/reference/conformance/).

**Internal model**: mxr's provider-agnostic types: `Envelope`, `Thread`, `Account`, `Address`, `Label`. Adapters map _into_ this model so application code never speaks Gmail-specific or IMAP-specific dialects.

## IPC buckets

Every IPC request the daemon serves falls into one of four buckets. The buckets are conceptual, not separate sockets, but they shape what's stable, what's user-facing, and what's an internal hatch.

- **`core-mail`**: read mail, send mail, mutate mail. The settled, stable surface.
- **`mxr-platform`**: accounts, rules, saved searches, subscriptions, semantic runtime. mxr-product features that aren't strictly _mail_.
- **`admin-maintenance`**: status, events, logs, doctor, bug reports, local reset, repair. Diagnostic and operational.
- **`client-specific`**: pane-shape, view-model, screen-state. Not part of the daemon contract; clients (TUI and web) shape these themselves.

See [Architecture](/guides/architecture/) for why this split matters.

## Account state

**Config-backed account**: an entry in `config.toml` under `[accounts.<key>]`. IMAP/SMTP accounts and Gmail-with-BYOC live here. Editable from the TUI's Accounts page or by hand.

**Runtime account**: what the daemon actually has connected. May include accounts that are config-backed (the common case) plus runtime-only entries (e.g. browser-auth Gmail sessions that don't have a TOML row). `mxr accounts` shows runtime; `mxr config show` shows config.

**Owned address**: an address registered locally against an mxr account, used for direction inference and per-message From selection. Registering it in mxr does not create or authorise the address at the mail provider. Manage with `mxr accounts addresses`.

## Search

**Lexical search**: Tantivy BM25 only. Exact, fast, deterministic. The default mode.

**Semantic search**: dense retrieval using local embeddings. Useful when you don't know the keywords. Semantic work is enabled in the built-in config, but lexical search remains the default mode and mxr falls back to lexical when dense retrieval is unavailable. Runs entirely on-device.

**Hybrid search**: lexical + semantic, fused with reciprocal-rank fusion. Best recall; the keyword-aware default for "I want it to find what I mean."

**Saved search**: a named, queryable inbox lens. Lives in the sidebar. Run from the CLI with `mxr saved run <name>`.

## Mutation flow

**Mutation**: any operation that changes provider state (archive, trash, spam, label, snooze, send, unsend, unsubscribe). All support `--dry-run`; see the [automation contract](/guides/automation-contract/).

**Mutation ID**: a short token printed by archive/trash/spam/read/read-archive. Copy it and pass to `mxr undo MUTATION_ID` within ~60 seconds to revert.

**Reply-later** (a.k.a. **bookmark**): flag a thread to come back to. Hey's term is "Reply Later"; in the TUI the key is `b` for bookmark. The reply queue is browsed with `mxr replies` or `Ctrl-p → Reply Queue`.

**Snooze**: hide a thread until a time. Returns to inbox at the specified moment. Set with `mxr snooze --until '<time>'` (see [time phrases](/reference/time-phrases/)) or the `Z` key in the TUI.

**Screener**: Hey-borrowed term for triaging unknown senders into Allow / Deny / Feed / Paper Trail. Anyone you have written to is never in the queue. Allow makes a sender a person, not someone you owe: they land in New from people until you write to them. Feed sends their mail to Reading and Paper Trail to Paper trail, the same as a [sender kind](#the-desk-and-places). Local-only consent metadata; never round-trips to the provider. CLI: `mxr screener`.

**Owed reply**: a conversation in your inbox where someone you have written to wrote last and you have not replied: the desk's You owe lane. mxr ranks them by `waiting_days / expected_days`, where `expected_days` is your usual reply time to that person. Reachable via `mxr owed`, the desk, or the TUI sidebar (Owed). The raw set (every thread whose latest message is inbound with no later reply, automated and archived mail included) is `mxr owed --all` or `mxr search 'is:owed-reply'`. See the [search guide](/guides/search/#common-patterns).

**Commitment**: a promise mxr extracted from your sent mail (e.g. "I'll send the deck Friday"). Persisted to `contact_commitments` after every successful `mxr send`. Browse with `mxr commitments --status open`. See [Forgotten work](/guides/forgotten-work/). You can also keep one yourself with `mxr commitments add`, or from the offer the web app shows after a send. Open ones due within a week show under **Due** on the desk.

## The desk and places

**Desk**: the web app's home and a TUI lens: what needs you, not what arrived. Four lanes, each row with its reason and age. CLI: `mxr desk`. See [Clear the desk](/guides/desk/).

**Lane**: one section of the desk. **You owe** (someone you are in conversation with wrote last), **Due** (a promise you made is coming due), **Waiting on** (you wrote last and they have not answered), **New from people** (a person you have not written to before). The rules are in the [desk reference](/reference/desk-and-places/#lanes).

**Done**: puts a desk row away. You owe and New from people are archived and marked read; Waiting on is marked read and set aside; a Due promise is marked kept. The row stays away until someone writes in the conversation again. CLI: `mxr desk done`, previewed with `--dry-run`, reversed with `mxr undo`.

**Promise**: the desk's and web app's word for a commitment you made in mail you sent. When you send one with a date, mxr offers to remind you. See [Reply to everyone you owe](/guides/focus-and-reply/#keep-the-promises-you-make).

**Focus & reply**: a web app mode that shows everyone you owe a reply one conversation at a time, with the reply beside it. See [Reply to everyone you owe](/guides/focus-and-reply/).

**Place**: where mail that is not from people goes instead of the desk. There are two, **Reading** (newsletters and lists) and **Paper trail** (receipts and notifications). Both are views over the inbox. CLI: `mxr reading`, `mxr paper-trail`. See [Clear Reading and Paper trail](/guides/reading-and-paper-trail/). Under the [modes plan](#email-modes), Reading becomes a mode and Paper trail splits into Updates and Archive.

**Sender kind**: where a sender's mail goes, set once with `mxr sender kind` or Move sender: people, reading, paper-trail, screened-out, or auto (the rules decide). Stored as the sender's screener decision. `mxr why MESSAGE_ID` says which rule placed a message.

**Sweep**: archive everything unpinned in a place, or in one sender's bundle there, after a preview. CLI: `mxr sweep`, reversed with `mxr undo`.

**Pin**: keeps a message in place when you sweep. Local to this machine; not a provider star. CLI: `mxr pin`, `mxr unpin`.

**Low tide**: what the web app shows when the desk is clear: "Low tide. Nobody's waiting on you."

**Time phrase**: a time typed in words, such as `fri 3` or `in 2d`, accepted wherever mxr asks for a time. Preview one with `mxr time`. See [time phrases](/reference/time-phrases/).

## Email modes

mxr's plan treats email as five apps sharing one inbox. See
[Email is five apps at once](/guides/email-modes/) for the model and what
ships today. Terms marked **(planned)** name parts of the plan that are not
in a release yet; the rest exist today.

**Mode** (planned): one of the five jobs email does, each with its own view,
unit and verbs: Messages, To do, Updates, Reading and Archive. One email
can be in several modes at once. Today the desk, Reading and Paper trail
cover parts of these jobs.

**Messages** (planned): the mode for people you are in conversation with,
one row per person with their conversations as topics inside. Today's
nearest features are the desk's You owe and Waiting on lanes, `mxr owed`
and Focus & reply.

**To do** (planned): the mode for things you must act on, each titled as
an instruction ("Pay council tax") with a due date and an act-by date.
Today's nearest features are promises (`mxr commitments`) on the desk's
Due lane and calendar invites.

**Updates** (planned): the mode for notifications, shown as a briefing by
source in fixed digests and let go in one key. Today Paper trail and
deliveries hold this mail.

**Reading** (mode): the planned mode for newsletters and posts you chose,
with a Later shelf. It grows out of today's Reading place, which exists.

**Archive** (mode, planned): records such as receipts, orders and bookings,
with an answer box that returns the field you asked for. Not the same as
the archive action, which removes mail from your provider's inbox.

**Now** (planned): the front page across the modes, at most ten items in
four fixed sections. The desk is its first version.

**Inbox**: everything, in arrival order. Under the modes plan it stays as
a lens over all mail, not a mode.

**Handoff** (planned): passing an item from one mode to the next, such as
making a to-do from a message (`t`) or letting a delivered parcel's order
file itself in Archive. The toast names where the item went.

**Done here** (planned): finishing an item in one mode without clearing it
from the others. The provider archive happens when the last mode lets go.
Today's **Done** on the desk archives at once.

**Reason**: the line that says why an item is where it is. Today
`mxr why MESSAGE_ID` prints the place and the rule. Under the plan every
item in every mode carries one, naming the rule, your decision or the
model.

**Correction**: telling mxr an item is in the wrong place. Today you
correct a sender with `mxr sender kind`. Corrections per email are
planned.

**Fast tier** (planned): model work that touches every incoming message,
such as deciding its mode. Runs on your local model by default; a cloud
model only with your explicit opt-in and your own API key.

**Smart tier** (planned): model work that needs care, such as pulling the
amount and deadline from a bill. It sees only mail already placed in To
do, Archive or Messages. Can step up (**escalate**) to a stronger model
when its answer fails the check that amounts and dates appear verbatim.

**Index recipe** (planned): what each mode indexes for semantic search,
such as one fact per Updates message or each message's new text in
Messages. Today one recipe covers all mail.

**Let go** (planned): Updates' and Reading's word for done. Letting go of
a digest acts on exactly the set the preview listed.

**Act-by date** (planned): the last day you can act and still meet a due
date, such as three working days before a bill paid by bank transfer.
To do surfaces an item ahead of its act-by date, not its due date.

**Record** (planned): Archive's unit, built from one or more emails: an
order's confirmation, dispatch and delivery are one record.

## Pre-send safety

mxr gates every send through a [six-check pipeline](/guides/pre-send-safety/) before any provider call.

**Verdict**: the rollup decision from the safety pipeline: `Safe` (no issues), `Warn` (one or more Warnings, no Blockers), or `Blocked` (at least one Blocker). The CLI `mxr send --check` exits 0 on Safe/Warn and exit code 2 on Blocked.

**Issue**: a single finding from one check. Has a code (`WrongRecipient`, `MissingAttachment`, `ReplyAll`, `PiiSecret`, `ToneMismatch`, `AnswerCoverage`, plus structural codes like `NoRecipients`), a severity (`Info`, `Warning`, `Blocker`), a message, optional detail/citations, and an `override_token` for Blockers.

**Override token**: a single-use, draft-scoped token that bypasses Blocker issues for one send. Minted by `mxr send --check` when a Blocker is present. Consumed atomically on the next `mxr send DRAFT_ID --override-safety TOKEN`. Subsequent attempts with the same token fail. Editing the draft and adding a new Blocker kind invalidates the token.

**Safety audit**: every safety run is persisted to `draft_safety_runs` (verdict + redacted issues). Every override consumption is persisted to `draft_safety_overrides`. PII is never persisted raw, only redacted previews.

## AI features

mxr ships AI features that run above the core mail model. None of them mutate
anything; synthesis from message evidence is citation-backed, while
deterministic profile fallbacks may return no citations. See [archive intelligence](/guides/archive-intelligence/), [briefings and loop-in](/guides/briefings-and-loop-in/), and [timing and cadence](/guides/timing-and-cadence/).

**Archive ask**: `mxr ask "<question>"` runs a hybrid (or lexical, or semantic) retrieval over your local mail and synthesizes a Markdown answer with cited message ids. The daemon rejects any LLM citation that points outside the retrieved set; "not enough evidence" is a valid answer.

**Citation**: a structured reference to source evidence. Briefing citations use `{ message_id?, thread_id?, field, quote }`; whois and archive-answer surfaces carry their own cited evidence shapes. LLM-backed synthesis validates citations against the candidate set before trusting them.

**Decision log**: `mxr decisions` is a queryable ledger of explicit decisions extracted from threads ("we agreed on Postgres"). Rows are stable across rebuilds: `id = hash(account, thread, normalized_decision, evidence_ids)`. Rebuild with `mxr decisions rebuild --since N`, idempotent on unchanged threads.

**Thread briefing**: `mxr briefing thread <id>` is a cached, citation-backed recap of a dormant thread (default threshold: 30 days). Synthesized from the local thread transcript, cached in `context_briefings`, and invalidated when the thread's message ids/dates change. Force regenerate with `--refresh`.

**Recipient briefing**: `mxr briefing recipient <email>` is the same cached briefing primitive scoped to a person. Its deterministic baseline includes known interaction counts and last inbound/outbound dates; the LLM can turn that baseline into prose when enabled. The TUI compose flow surfaces it as a quiet hint after a long gap; never auto-inserts into the draft.

**Cadence watchlist**: `relationship_watchlist` is an explicit, account-scoped table of "relationships I chose to maintain". `mxr cadence watch <email> --every <N>d` adds a row; `mxr cadence drift` lists watched contacts whose interval has drifted past expected. mxr never auto-watches contacts.

**Send-time recommendation**: `mxr send-time <recipient>` computes the recipient's reply-latency bucket from local `reply_pairs`. Statistical, no LLM. Fires as a non-blocking hint inside `mxr send --check` when the proposed slot is at least 2× slower than the best window and confidence is medium or high.

**Maybe include**: `mxr suggest-recipients --draft <id>` proposes Cc candidates who co-participate on similar prior threads. Hard rules: minimum support of 3 distinct threads, no self-suggestions, Bcc is never leaked, existing recipients excluded.

**Expert**: `mxr expert <message-id>` or `--query "<text>"` ranks locally cited answerers of similar questions, not askers. Citations point at *answer* messages: replies that follow a question, contain explanatory content, and are followed by thanks or no further unresolved ask.

**Whois entity**: `mxr whois <name|email|term>` looks up a person or free-text term using only locally cited mail evidence. Project names currently use the free-text `term` / `ambiguous` path. Query-time only: there is no persisted `entities` table in v1. Ambiguous queries return `candidates`, not a synthesized definition.

**Delivery**: a tracked package distilled from shipping mail. A local heuristic (carrier/merchant senders, shipping subjects, schema.org markup, checksum-valid tracking numbers) shortlists candidates; an optional LLM step confirms and extracts merchant/carrier/items/ETA; a lifecycle layer collapses an order's many emails into one row whose `status` only advances and resolves on "delivered". Browse with `mxr deliveries` or TUI tab `7`. `source` is `schema`, `llm`, or `heuristic`. See the [Deliveries guide](/guides/deliveries/).

## Display

**Reader mode**: strips signatures, quoted text, tracking pixels, and remote-image references for distraction-free reading. Toggle with `R`.

**Plain text first**: mxr renders text/plain bodies if they exist, falling back to HTML→text only when needed. HTML remote images are controlled by `render.html_remote_content`; with it on, the TUI and CLI fetch every image the HTML points at, tracking pixels included. Only the web app filters: it drops images that declare a width or height of two pixels or less, plus images from known tracker hosts.

## Provider quirks (the seam)

**Label**: Gmail's primary classification primitive. A message can have many labels.

**Folder**: IMAP's primary classification primitive. A message lives in exactly one folder. mxr's `Label` type carries a `LabelKind::Folder` variant so IMAP folders don't get flattened into Gmail-style labels, preserving the semantic difference is important. `mxr move` is the verb that operates on folder semantics; `mxr label` is for label-style multi-membership.

**Provider ID**: the provider's own message identifier. Gmail's is stable; IMAP's is mailbox-scoped and may change across moves/copies. mxr stores it on `Envelope.provider_id` for round-trips back to the provider.

## Process state

**Sync**: pulling new mail and applying remote changes locally. Triggered automatically every `[general] sync_interval` seconds, or on demand with `mxr sync`.

**Indexing**: populating Tantivy from new SQLite records. Always runs as part of sync; lexical search is fresh as soon as a sync batch commits. Semantic chunks are persisted but not embedded unless `[search.semantic] enabled = true`.

**Reset / Burn**: `mxr reset --hard` and its alias `mxr burn` destroy local runtime state (the daemon, the database, the search index). They preserve `config.toml` and credentials by default. Use `--including-config` to also drop the config; pair with `--dry-run` to preview.

## See also

- [Architecture](/guides/architecture/): the daemon-and-clients model in depth
- [CLI concepts](/reference/cli/concepts/): query operators, search modes, output formats
- [Automation contract](/guides/automation-contract/): the scriptable surface
- [Decision log](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/15-decision-log.md): D001 to D114, why mxr is the way it is
