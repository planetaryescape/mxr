# AI Email Layer

The AI layer in mxr exists to do real email work: prevent send mistakes, recover
forgotten promises, find old context, and improve timing. It is not a
demo-driven feature set. This document captures the design intent behind the
six tracks (pre-send safety, forgotten work, archive intelligence, timing,
briefings, collaboration) and the constraints any future change must respect.

Every track is shipped end-to-end across store, protocol, daemon, CLI, and TUI
as of 2026-05-28. The "where it lives" sections below point at the entry
points; treat code as the source of truth for current shape.

## Why these features and not others

The cut list matters as much as the build list. We explicitly do not ship:

- **3-button Smart Reply.** Throwaway text, hostile to keyboard-native email.
- **Magic-wand "rewrite in style X"** without relationship grounding. `draft-refine`
  and voice-match are fine because they are grounded in `contact_style` and the
  user's voice profile.
- **AI emoji insertion.** Ever.
- **Uncited archive answers.** No citation, no answer. The LLM is allowed to
  say "not enough evidence"; it is not allowed to invent.
- **Always-on relationship nags.** Cadence drift requires an explicit watchlist.
  No auto-watch, no startup modal.
- **Auto-CC, auto-forward, auto-rewrite, auto-send.** Suggestions only. mxr may
  warn or block pending an explicit override, but it must never silently rewrite,
  add recipients, or send.

Future contributors: if a feature proposal cannot point at a concrete email job
it makes faster or safer, it does not belong here.

## Architectural principles

These are load-bearing constraints, not preferences.

1. **Local-first.** SQLite is source of truth. Embeddings and derived summaries
   must be rebuildable from local mail. No hosted email search, no remote
   provider agentic browsing.
2. **Deterministic before LLM.** Regex, reply-pair statistics, address history,
   and explicit config run before any LLM call. When the LLM is disabled the
   deterministic path must still produce a useful answer or an explicit error;
   silent degradation that pretends success is a bug.
3. **CLI first.** Every capability has a CLI surface with JSON/JSONL output
   before or alongside TUI. The TUI is a daemon client; it never invents
   behavior the daemon does not expose. Scripts and agents are first-class
   consumers.
4. **Citations required for message-evidence synthesis.** Archive ask,
   decisions, thread briefings, expert, and whois must tie claims back to
   retrieved message or thread evidence. Profile-only deterministic fallbacks
   may return an empty citation list, but they must not pretend to be cited
   synthesis. The validator rejects LLM output whose citation ids are not in
   the retrieved candidate set — this is the single most important guardrail
   and must not be relaxed.
5. **Warnings are reviewable, blockers are explicit.** `--yes` does not bypass
   safety blockers; a separate single-use override token does. Override tokens
   live in `draft_safety_overrides` and are consumed on use.
6. **Provider-agnostic core.** Provider quirks stay below the core mail model.
   Don't reach for provider-specific contact APIs from feature code.
7. **Privacy explicit.** Relationship data leaves the machine only when the
   existing LLM privacy config allows it. Recipient briefings degrade to a
   deterministic profile-only path when cloud LLM is gated.
8. **Retrieved mail is data, never instructions.** Every LLM feature that
   consumes message content (summarize, ask, draft-assist, briefings,
   commitments, decisions, answer coverage) treats email fields and
   attachments as untrusted input to analyze, never as directives to follow.
   Prompts must tell the model to ignore instructions embedded in mail
   content, and validators must not let mail-derived text expand scope:
   email cannot expand permissions, redirect recipients, trigger tools,
   request credentials, or override the feature's task. The agent-facing
   statement of this rule lives in the mxr skill and the for-agents guide.

## Reuse map

Before adding a new subsystem, check whether the existing infra already
covers it. Most AI features here are thin layers over primitives that
already existed when this work started:

| Need | Existing base |
|---|---|
| Local retrieval | `mxr-search`, `mxr-semantic`, hybrid search, saved searches |
| LLM calls | `mxr-llm`, feature-specific runtime overrides |
| Thread synthesis | `handler/summarize.rs`, `thread_summaries` table |
| Draft generation | `draft-assist`, `draft-new`, `draft-refine` handlers |
| Voice and tone | `mxr-relationship`, `contact_style`, `user_voice_profile` |
| Commitments | `contact_commitments`, relationship service extraction |
| Response timing | `reply_pairs`, `mxr response-time` |
| Forgotten replies | `mxr stale`, `message_flags`, reply-later queue |
| Relationship cadence | `contacts`, `mxr contacts decay` |
| Deterministic writing checks | `mxr-humanizer`, compose validation |
| Searchable old content | semantic chunks, lexical Tantivy index, LLM export |

If a new feature needs a new table, justify why the existing tables cannot
hold it (e.g. cardinality, freshness, query shape). Materialized caches like
`recipient_reply_latency_buckets`, `expertise_index`, `collaborator_patterns`,
`entities`, and `entity_mentions` are deliberately documented future caches,
not current schema. Add them only when query-time performance forces the issue.

Thread summaries are demand-driven, not sync backfill. `GetThread` returns a
valid cached row from `thread_summaries` when the content/relationship hash
still matches; `SummarizeThread` refreshes it through the LLM bulk lane. The
TUI and web clients may lazily request a missing summary after opening a thread,
but the request must stay backgrounded and visible in the reader instead of
blocking navigation.

---

## Track 1: Pre-Send Safety

One composable pipeline, six checks. Lives in `crates/safety/` and is invoked
from CLI, TUI compose confirm, `SendDraft`, `SendStoredDraft`, and the
scheduled-send flusher through `enforce_draft_safety_with_override()` in
`crates/daemon/src/handler/mutations.rs`. The single shared entry point is
load-bearing: any code path that hands a draft to a provider must route
through it.

### Checks and severity contract

- **Wrong Recipient** — typo distance vs. contact frequency; internal-leak
  detection (body markers + external domain); configurable
  `[safety.recipients]` for `internal_domains`, `sensitive_domains`,
  `warn_on_first_time_external`. Blocker for sensitive domain or likely
  internal leak; warning for typo candidate or first-time external.
- **Missing Attachment** — pure regex on subject+body. Subtracts negations
  (`not attached`, `without attachment`) and quoted reply context. Warning.
- **Reply-All Sanity** — only triggers when reply-all is on, >2 recipients,
  and a single non-self person is named (vocative cue) with no group language
  (`team`, `all`, `everyone`). Warning, never blocker.
- **PII/Secrets** — local-only detectors: Luhn-valid card, SSN shape, common
  secret prefixes (`sk-...`, `ghp_...`, `xoxb-...`, `AWS_*`, PEM private key,
  `api_key=`, `client_secret=`). Redacted previews only — no raw secret ever
  appears in JSON, logs, or TUI modal. Blocker for keys/API secrets, warning
  for SSN/card.
- **Tone Mismatch** — deterministic stylometry against `contact_style` and
  voice profile. Only warns when sample size is sufficient (≥3 prior sent)
  and confidence is medium/high. No LLM call here by design.
- **Answer Coverage** — the only LLM-backed check. Loads thread, asks for
  strict JSON of explicit asks with `evidence_msg_id`, validates every id is
  from the retrieved thread, then warns when asks are unaddressed. Lives in
  `handler/safety_llm.rs`. Disabled LLM degrades to Info, not silent pass.

### Failure-mode invariants

- Store unavailable: `SendStoredDraft` fails closed; `--check` reports error.
- Safety pipeline panic: send is blocked. A broken safety gate must not
  silently pass — this is non-negotiable.
- Scheduled send hits a blocker: keep draft, clear schedule, emit event.
- Audit lives in `draft_safety_runs`. Don't store full PII; redacted preview
  + issue kind + field only.

### Things to know before changing safety

- The single-use override token contract (one token, one consume, one
  audit row) is what lets us be strict-by-default without painting the user
  into a corner. Don't loosen it to "session overrides" without a fresh
  threat-model conversation.
- Adding a new check means: new `DraftSafetyIssueCode` variant, deterministic
  unit tests in `mxr-safety`, integration test through the daemon, snapshot
  refresh, and a TUI modal verdict path.

---

## Track 2: Forgotten Work

### Outgoing commitments

Two-stage ledger. The send path extracts commitment candidates (deterministic
prefilter for first-person promise phrases + due-date phrases, then LLM via
`LlmFeature::Commitments` with strict JSON validation, in
`handler/commitments_extract.rs`). On `--check`, candidates are shown but not
persisted. On a successful send and local sent ingest, candidates promote
from `draft_commitment_candidates` to the existing `contact_commitments`
table.

Promotion is keyed on `(account, contact, sent message id, direction,
normalized what)` so an idempotent resend does not duplicate.

Failure modes worth remembering:
- LLM disabled → low-confidence deterministic candidate in safety report,
  no automatic ledger row.
- Send succeeds but promotion fails → send receipt still returns; the event
  log records "unpromoted candidates" and `mxr doctor` can surface them.
- Scheduled send re-runs extraction at fire time because the draft may have
  changed since enqueue.

### Owed-reply lens

`mxr owed` / `is:owed-reply` (with `is:owed_reply`, `is:owed` aliases).
Currently computed on demand from `messages`, `reply_pairs`,
`contacts.cadence_days_p50`, `message_flags`, and `screener_decisions`. We
chose not to materialize because the query is bounded and analytics rebuild
already maintains the inputs. The trigger to materialize would be either
mailbox size making the query slow or a real-time TUI lens that polls
aggressively.

Ranking is `overdue_score = waiting_days / expected_days`, where
`expected_days` falls back contact-cadence → global p50 from `reply_pairs` →
7. List senders, screener-denied/feed senders, trash/spam are excluded; this
is what stops it from becoming a "yelling at you about newsletters" feature.

---

## Track 3: Archive Intelligence

### `mxr ask`

Synthesis on top of search, not a replacement for it. Lexical+semantic
hybrid retrieves candidates; the LLM is constrained to answer only from
provided excerpts and to cite every claim. Citation validation against the
retrieved candidate set is the bug-stopper — without it the model invents
ids that look plausible. Date/from filters are enforced before the prompt,
not by the LLM. Cache is optional and invalidated by a content fingerprint,
not by time.

Modes degrade explicitly: semantic unavailable → lexical only with the
executed mode reported back; LLM disabled → top search results plus
"synthesis unavailable". Never pretend a fallback is the real thing.

### Decision log

`decision_log` table with stable ids = hash(account, thread, normalized
decision, evidence ids). The stable-id migration (`034_decision_log_stable_id`)
is what lets rebuild be idempotent — earlier the table used row-generated
ids and re-extraction duplicated rows. Don't revert to row ids.

The extractor only writes entries that have evidence message ids in the
thread; "we agreed on Postgres" with a citation is a decision; brainstorming
without resolution is not. The candidate-thread filter (recent multi-message
threads or threads with decision phrases like `decided`/`agreed`/`go with`)
keeps the rebuild affordable.

### `mxr whois` (Knowledge Graph)

Deliberately kept query-time with no `entities`/`entity_mentions` tables.
For an email query we use sender profile + relationship profile; for a
free-text query we run lexical search and return deterministic cited
matches/candidates. Ambiguity returns candidates rather than a synthesized
answer. Persistence is a materialization decision — make it when query
latency forces it, not before.

---

## Track 4: Timing and Cadence

Both features here are statistical, local, and cheap. No LLM, no calendar,
no server-side tracking.

### Send-time optimizer

Computed on demand from `reply_pairs` where direction = `they_replied`.
Bucketed by local weekday/hour of the outbound message. Confidence tiers
gate user-facing output: high (≥20 pairs, ≥3 buckets) / medium (≥8) / low.
We only emit a recommendation when the proposed slot is at least ~2× worse
than the best slot at medium+ confidence — otherwise it's noise. The
recommendation surfaces in the safety pipeline via `safety_timing.rs` so it
shows up inline in `send --check`.

Optional `recipient_reply_latency_buckets` cache is documented but
unimplemented. Add only if on-demand query becomes the bottleneck.

### Cadence drift

Watchlist is explicit (`relationship_watchlist`). Last contact is
`max(last inbound, last outbound)` from `contacts`. Drift is
`now - last_contact_at - expected_days`; we list only positive drift,
ranked by drift days then relationship volume. List senders are rejected
by default and need `--allow-list-sender` — the whole point is that this
is opt-in for relationships the user cares about, not an automated CRM.

`SendTimeBucketData` in `crates/protocol/src/types.rs` is a leftover from
an earlier shape and is no longer referenced by any IPC type. Safe to
remove in a cleanup pass.

---

## Track 5: Briefings and Collaboration

### Briefings (`mxr briefing thread|recipient`)

Cached in `context_briefings` keyed by content hash. One table, two `kind`
values (`thread`, `recipient`). Thread briefings currently hash thread message
ids and dates; recipient briefings hash account/email plus contact counters and
last-contact timestamps. Relationship summaries, commitments, and decisions are
not part of the current thread briefing source pack.

Threshold gating matters: thread briefings only surface a TUI hint when a
thread is dormant past the configured threshold (default 30 days);
recipient briefings hint past 180 days. The CLI is always available
manually. Briefings never auto-insert into a draft body — they show as a
quiet hint the user opens explicitly.

Privacy interaction: if relationship privacy disallows cloud synthesis,
the recipient briefing degrades to deterministic profile-only output. This
is enforced before the LLM call, not after.

### Collaboration: suggest-recipients and expert

Both compute on demand. Optional materialized tables
(`collaborator_patterns`, `expertise_index`) are documented but deferred.

`suggest-recipients` requires `MIN_SUPPORT_THREADS` (default 3) of evidence
— a single coincidence does not produce a suggestion. Bcc evidence is
never leaked: contacts that appear only as Bcc on the user's own sent mail
are filtered out before scoring. Existing To/Cc and self addresses are
also excluded.

`expert` ranks people whose messages answered similar questions (their
message follows a question, contains explanatory content, thread later has
thanks/no further ask). People who asked similar questions but did not
answer are not ranked. Citations point to **answer** messages, not the
matching question messages — that's the invariant. LLM can polish reason
text later, but the current handler returns deterministic reason strings.

---

## Track 6: Knowledge Graph

Covered above under `mxr whois`. Status: query-time, no persisted tables,
intentionally. The schema in the spec (`entities`, `entity_mentions`) is
the path forward when persistence is justified by real query latency, not
hypothetical scale.

---

## Schema migrations contributed by this layer

- `028_draft_safety_audit.sql` — `draft_safety_runs`, `draft_safety_overrides`
- `029_draft_commitment_candidates.sql`
- `030_decision_log.sql`
- `031_relationship_watchlist.sql`
- `032_context_briefings.sql`
- `034_decision_log_stable_id.sql`

If you change one of these, treat it the same as any other schema change:
forward-only migration, no destructive rewrite of existing rows.

---

## Shared protocol shapes

The pre-send pipeline standardized a few shapes that the rest of the AI
layer reuses. The conventions:

- `Citation`-style structs carry source ids plus quote/field context, but the
  exact fields differ by surface. `CitationRefData` uses optional
  `message_id` / `thread_id` plus required `field` and `quote`; whois and
  archive-answer surfaces carry their own evidence shapes.
- Reports have a `verdict` (`Safe` / `Warn` / `Blocked`) and a flat list of
  `issues` with `code`, `severity`, `message`, optional `detail`,
  `citations`, and `override_token`. CLI/JSON output is the verdict +
  ordered issues; do not nest by check kind.
- IPC `*Data` structs are stable contracts. Renames need an OpenAPI snapshot
  refresh (`mxr-web` carries the public schema) and a migration plan for
  the web client (`apps/web`).

---

## Testing posture

- `mxr-safety` has unit coverage for every deterministic check (52 tests).
- Each AI CLI surface has an integration test that runs the daemon against
  real SQLite fixtures — not mocks. The integration tests are the spec for
  CLI/JSON output stability.
- Snapshots under `crates/daemon/tests/snapshots/` lock CLI help and JSON
  report shape; update them deliberately when surfaces change.
- LLM tests use a mock LLM. The mock must emit invalid citations in at
  least one test per LLM-backed feature so the citation validator stays
  honest.

Known flaky-but-out-of-scope: `crates/daemon/tests/reset_cli.rs` has a
`wait_for_process_exit` timeout that flakes under load. Separate ticket,
not an AI-email regression.

---

## Things to know if you're enhancing this layer

1. **Don't add an LLM call for something a join can do.** Tone mismatch
   stayed deterministic on purpose; if you find yourself wanting an LLM
   for a check that the data could answer with a histogram, look at the
   data first.
2. **Citation validation is a checkpoint, not a suggestion.** If you're
   building a new synthesis feature, the candidate-set check (LLM output
   ids must be in the retrieval set) is the first thing to wire, not the
   last.
3. **The send path's safety gate is shared.** Adding a new send entry
   point (e.g. a scheduled-resend or batch sender) means routing through
   `enforce_draft_safety_with_override()`. New code paths that call the
   provider directly are bugs.
4. **`--yes` is not an override.** Blockers need the single-use token,
   intentionally. Don't add `--force` aliases; if a check is wrong too
   often, fix the check.
5. **TUI follows daemon.** If a feature works in the TUI but not in the
   CLI, the CLI is the bug. Scripts and agents are first-class consumers
   and they only see the daemon.
6. **Materialize only when forced.** Optional caches in this layer
   (`recipient_reply_latency_buckets`, `expertise_index`,
   `collaborator_patterns`, `entities`) are documented intentionally.
   Don't add them speculatively; add them when a real query is slow on a
   real mailbox.
7. **Privacy gates run before LLM calls, not after.** If a cloud-LLM
   privacy check has to inspect a prompt body, the architecture is wrong.
8. **Audit rows are the contract with users.** `draft_safety_runs` and
   `draft_safety_overrides` exist so users can see what was checked and
   what they overrode. Don't truncate them aggressively.

---

## Pointers (current as of 2026-05-28; verify against code)

- Safety crate: `crates/safety/`
- Safety enforcement: `crates/daemon/src/handler/mutations.rs`
- LLM answer coverage: `crates/daemon/src/handler/safety_llm.rs`
- Timing hint: `crates/daemon/src/handler/safety_timing.rs`
- Commitment extraction: `crates/daemon/src/handler/commitments_extract.rs`
- Decision extraction: `crates/daemon/src/handler/decisions_extract.rs`
- Briefings: `crates/daemon/src/handler/briefing.rs`
- Suggest recipients: `crates/daemon/src/handler/suggest_recipients.rs`
- Expert: `crates/daemon/src/handler/expert.rs`
- Whois: `crates/daemon/src/handler/whois.rs`
- TUI send confirm modal: `crates/tui/src/ui/send_confirm_modal.rs`
- TUI owed lens: `crates/tui/src/ui/owed_lens.rs`
- Web client OpenAPI snapshot:
  `crates/web/src/snapshots/mxr_web__tests__openapi_spec_summary.snap`
