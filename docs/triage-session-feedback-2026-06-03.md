# Triage-session field report — CLI gaps & summariser triage signal (2026-06-03)

Field report from an agent driving `mxr` through a full inbox triage (1080 → 880 inbox;
`Follow Up` 122 → 37; `Receipts` 13 → 38; ~13 senders unsubscribed; 4 auto-archive rules
added). Captures (1) CLI feature gaps that cost real time during the session and (2) a
summariser-prompt change to surface actionability up front. Grounded in commands actually
run, with evidence captured inline.

Source: live triage session, 2026-06-03. Verify against current `--help` before scoping —
the CLI moves fast.

- [Already exists (discoverability, not gaps)](#already-exists)
- [Part 1 — CLI gaps, prioritised](#part-1)
- [Part 2 — Summariser triage-signal prompt](#part-2)

## Already exists {#already-exists}

Confirmed present while writing this — do **not** rebuild:

- **`mxr unsubscribe <address> [--search …]`** — unsubscribe by email address (rewrites to
  `from:<addr>`, acts on the most recent match), scopable with `--search`. The agent did a
  manual `search --format ids | head | unsubscribe` dance unnecessarily. Gap is only the
  *footprint clear* (see P0-4), not address-based unsubscribe.
- **`mxr storage --by sender`** — a real per-sender aggregation, but disk-ranked and not
  scoped to an arbitrary query. The building block for P0-2 exists; it just isn't
  generalised to triage.
- **`mxr subscriptions --rank`** — open-rate and archived-unread ranking plus the
  resolved unsubscribe method per sender, in one place. The single most useful command of
  the session. (`replied_count` is present in JSON but currently stable-zero; one caveat —
  see P2-11.)

## Part 1 — CLI gaps, prioritised {#part-1}

### P0 — biggest time sinks

**P0-1. Surface a triage signal in list/search output (cached).**
Today `summarize` is **thread-only**, one LLM call each (`--limit` warns each match is a
metered call), and its output is separate from `search`. There is no *read-the-signal-first*
path, so the agent read full bodies just to classify routine vs actionable. This was the
core inefficiency of the whole session.
- Want: `mxr triage --search "label:inbox"` → one line per message:
  `ACTION|FYI|ROUTINE · date · sender · subject · one-clause why`, **cached** so re-runs are
  free and so other surfaces (TUI/web) can reuse it.
- Equivalent: a `--summarize`/`--triage` column on `search`.
- Directly powered by Part 2 — the summariser already emits the verdict; this just exposes
  and caches it. Turns "read 200 bodies" into "scan 200 lines."

**P0-2. Query-scoped sender aggregation (`--group-by`).**
The agent hand-rolled Python sender tallies ~4× (survey, re-survey after each wave). "Survey,
don't read in order" depends on this and the CLI can't do it for an arbitrary query.
- Want: `mxr search "label:inbox" --group-by from` (or `count --group-by from`) →
  sender, count, unread, oldest/newest.
- Engine already exists in `subscriptions --rank` and `storage --by sender`; generalise it
  to any query + grouping field (`from`, `list`, `category`).

**P0-3. Readability pass in reader view for HTML-only mail.**
Verified this session: `mxr cat <id>` (default reader view) on a message with
`text_plain: null` returns **raw HTML**, not rendered text.
- Evidence: Tesco order email → 50,309 chars of raw HTML; Numan treatment emails → 209 KB.
- Want: reader view runs an HTML→text readability pass when `text_plain` is absent; reserve
  raw markup for `--view html`/`--raw`. (AGENTS.md already states "rendering is plain-text
  reader-first" — this is a correctness gap against that invariant.)
- Without it, "check contents" on any modern marketing/transactional email is unusable.

**P0-4. One-shot unsubscribe + footprint clear.**
`unsubscribe <addr>` exists, but *cutting* a sender is still two commands: unsubscribe, then
`read-archive --search "from:<addr>"`. This was the most-repeated sequence of the session
(cut-list ×9, Simply Wall St, France Culture, The Moth, Tailscale, …).
- Want: `mxr unsubscribe <addr> --purge` (or `--archive-all`) → unsubscribe **and**
  read-archive the whole footprint in one mutation, returning a single undo id.

### P1 — notable friction

**P1-5. Multi-action rules + a queue `route` verb.**
Rules accept exactly one action — `--then 'archive;mark-read'` is rejected
(`Unsupported action`). So auto-archive rules can't also mark read (leaves unread in the
archive, against the user's stated preference). Queue routing (`home`/`Notto`/`Follow Up`)
was always three calls: `label` + `unlabel` + `read-archive`.
- Want: chained actions (`--then 'mark-read,archive'`) **and**
  `mxr route --to <label> --from-queue <label> [--archive]` as one atomic mutation.
- Confirmed valid single actions: `archive`, `mark-read`, `trash`.

**P1-6. Large-batch mutations time out.**
Footprint archives of 400–500 messages (e.g. `theaibreak@substack.com` 445,
`franceculture` 482) hit `IPC request timed out after 120 seconds`, leaving ambiguous state
and no surfaced undo id.
- Want: server-side chunking with progress, or `--async` returning a job id (+ `mxr jobs`),
  so large sweeps don't block or half-apply.

**P1-7. Search result ceiling / no pagination.**
`search --limit 1080` returned ~755 of 1080. There is no `--offset`/cursor, so full-inbox
*discovery* silently truncates. (Mutations via `--search` are server-side and unaffected —
this is a discovery/tallying problem only.)
- Want: honor large `--limit`, or add `--offset`/cursor, or document the cap explicitly.

### P2 — polish

- **P2-8. `count --format plain`** (bare integer). The agent grepped `"count":N` on every
  call; `count` only emits `table/json/jsonl/csv/ids`.
- **P2-9. dry-run vs apply parity.** A Lovevery sweep dry-ran 7, applied 5 (thread
  collapse). Output should state "N threads / M messages affected" so delta-checks aren't
  confusing.
- **P2-10. `unsubscribe --dry-run <addr>`** that reports the method or `None` up front, so
  dead-ends (MedExpress, blockchain.com — no `List-Unsubscribe`) are known *before* acting.
- **P2-11. Clarify `subscriptions --rank` `opened_count`.** For several senders it equalled
  `message_count`, which made engagement calls fuzzy. Document the semantics (proxied pixel
  opens? distinct opens?).

## Part 2 — Summariser triage-signal prompt {#part-2}

Append to the existing summariser prompt. Forces an unambiguous classification as the literal
first line, before any topic description. The single-token first line is deliberately the
same shape a `mxr triage` column (P0-1) would parse.

```
OUTPUT FORMAT — STRICT:
The FIRST line must be the triage verdict and nothing else. It must begin with
exactly one of these three tokens, verbatim:

  ACTION REQUIRED — <the specific reason, + any deadline as (by YYYY-MM-DD)>
  FYI — <why it's legitimate but needs nothing from the reader>
  ROUTINE — <marketing / notification / automated / low-signal>

Then a blank line, then your normal summary.

CLASSIFY BY WHAT THE EMAIL ASKS OF THE READER, not by topic, sender prestige,
or length:

- ACTION REQUIRED: the reader must reply, decide, pay, sign, submit, renew,
  confirm, or show up — OR money/security/legal/health consequences follow from
  inaction. Recurring failures (e.g. a payment that keeps bouncing) are ACTION.
  Always name the reason; surface any date as "(by YYYY-MM-DD)".
- FYI: legitimate and possibly worth knowing, but nothing is owed and no choice
  is pending (a shipped order, a published statement, a routine receipt to file).
- ROUTINE: marketing, promotions, digests, social/app notifications, and
  automated noise the reader can safely skip.

TIE-BREAKERS:
- Unsure between ACTION and FYI → choose ACTION.
- Unsure between FYI and ROUTINE → choose FYI.
- A deadline, an unpaid balance, a security/login alert, or someone explicitly
  waiting on the reader → ACTION, regardless of how routine the sender looks.
- "Action" verbs in marketing copy ("Act now!", "Don't miss out") are ROUTINE,
  not ACTION — judge real consequence, not tone.

The verdict line must stand alone and be machine-parseable: one token, one em
dash, one clause. No hedging ("possibly", "might be"), no second sentence.
```

### Calibration examples (from the source triage session)

| Expected first line | Sender |
|---|---|
| `ACTION REQUIRED — recurring scheduled-payment failure, 5× since Apr 1` | Starling |
| `ACTION REQUIRED — Azure payment overdue, card declined (by 2026-05-25)` | Microsoft |
| `FYI — new prescription issued; Numan auto-ships, no action` | Numan |
| `FYI — parcel delivered to your Safeplace` | Royal Mail |
| `ROUTINE — AI-news digest, aggregator` | The AI Break |
| `ROUTINE — Business Manager partner-request notification` | Facebook |

### Design note

Part 2 and P0-1 reinforce each other: if the summariser emits this strict verdict line, a
`mxr triage` view can parse/sort/colour by verdict for free (`grep '^ACTION'`, sort inbox by
triage key, etc.). Build the prompt change first (cheap, no code), then the cached `triage`
surface on top.
