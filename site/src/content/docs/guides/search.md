---
title: Search workflow
description: Use search as the primary navigation model in mxr.
---

## Core idea

mxr treats search as navigation, not a bolt-on filter. Search results drive the TUI mail list, saved searches, exports, and batch mutations.

## Common patterns

```bash
mxr search "from:alice unread"
mxr search "label:work has:attachment"
mxr search "subject:\"quarterly review\" after:2026-01-01"
mxr search "unsubscribe"
mxr search "label:inbox" --format ids
mxr search "adrian in:inbox" --limit 1000
mxr search "{from:amy from:david} subject:(dinner movie)"
mxr search "holiday AROUND 10 vacation"
mxr search "body:house of cards" --mode hybrid --explain
mxr search "is:owed-reply"
mxr search "has:calendar newer_than:30d"
```

The `is:` filter has the usual suspects (`unread`, `read`, `starred`,
`draft`, `sent`, `trash`, `spam`, `answered`, `inbox`, `archived`,
`reply-later`) and one computed lens worth knowing about:

- **`is:owed-reply`** (alias `is:owed`) — threads where the latest
  message is inbound, no later outbound exists, the sender isn't a
  newsletter/list, and the screener hasn't denied them. Same result
  set as `mxr owed`, so the TUI sidebar can show it as a saved search.

Gmail-style operators are parsed by the `mail-query` crate, then executed
against mxr's local index. The important practical consequence: syntax
parity is separate from storage parity. For example, Gmail's color-star
operators parse, but mxr currently stores starred as a boolean, so
`has:yellow-star` and `has:purple-question` both mean "starred" locally.
Likewise `+word` is preserved in the AST as a no-stemming hint, but the
current Tantivy schema executes it like a normal text term until mxr grows
a non-stemmed mirror field.

## Search modes

- `lexical`: exact BM25/Tantivy retrieval
- `hybrid`: lexical + dense retrieval + RRF
- `semantic`: dense retrieval only

Semantic search is optional. It is an `mxr-platform` feature layered on top of the mail runtime, not a requirement for normal mail sync/read/send.

Embeddings stay local. OCR is not used for semantic indexing.

## Dedicated search page

The TUI has two distinct search tools:

- `/` from Mailbox opens Search and hits the full local index
- `Ctrl-f` from Mailbox filters only the current mailbox view

The Search page gives you:

- Query input
- Result list
- Preview pane
- Lexical / hybrid / semantic modes
- Normal open flow into mailbox/thread interaction once a result is previewed

## Useful combinations

- Use `mxr search ... --format ids | xargs ...` for shell pipelines.
- Save high-value searches in the TUI sidebar for recurring workflows.
- Use `mxr count QUERY` for quick status-bar or script integration.
- Use `mxr export --search QUERY --format mbox` to archive slices of mail.
- Use `--explain` when debugging hybrid/semantic fallback or dense contribution.

## Fielded hybrid behavior

Examples:

```bash
mxr search "body:house of cards" --mode hybrid --explain
mxr search "subject:house of cards" --mode hybrid --explain
mxr search "filename:house of cards" --mode hybrid --explain
```

Current intent:

- lexical side stays literal and field-aware
- dense side respects chunk source kinds
- `body:` searches body chunks
- `subject:` searches header chunks
- `filename:` searches attachment-origin chunks

Literal lexical matches should usually remain stronger than merely related semantic matches.

## When search is fresh

- lexical search is fresh right after the sync batch commits
- hybrid search can use lexical results immediately, even if semantic retrieval is disabled
- semantic-only readiness depends on the active profile being built

Check semantic readiness with:

```bash
mxr semantic status
mxr doctor --semantic-status
```

## TUI flow

1. Press `/` in Mailbox to jump into Search.
2. Start typing. The Search page runs live after a short debounce.
3. Press `Enter` to run immediately when you do not want to wait for debounce.
4. Use `Tab` to change lexical / hybrid / semantic mode while editing, or to switch results and preview when not editing.
5. Use `j` / `k` to move the result cursor.
6. Use `Enter`, `o`, or `l` to open the selected result in preview.
7. Use `Esc` to move preview -> results -> mailbox.

Search is full-corpus by design. It searches beyond the currently loaded mailbox slice.

## Saved searches

Saved searches are not a secondary convenience. They are persistent inbox lenses and appear in:

- The sidebar
- Command palette
- CLI via `mxr saved`

Common flow:

```bash
mxr saved add urgent "label:inbox unread from:boss@example.com"
mxr saved list
mxr saved run urgent
```

## In real life

- **Quick "did I miss anything important today":** `mxr search 'is:unread label:inbox newer_than:1d' --format json | jq 'group_by(.from) | map({sender:.[0].from, count:length})'`
- **Find that one PDF you were sent last quarter:** `mxr search 'has:attachment filename:pricing.pdf older_than:90d' --mode hybrid`
- **Bulk-archive every receipt 30 days old:** `mxr search 'label:receipts older_than:30d' --format ids | mxr archive --yes`
- **Build a digest before a 1:1:** `mxr search 'from:sarah newer_than:7d' --format json | jq -r '.[].subject'`
- **Pull only real conversations, hide newsletters:** `mxr search 'is:unread has:link-none'` — strips link-heavy promotional/transactional mail in one filter.
- **Triage the newsletter pile in bulk:** `mxr search 'has:link-heavy older_than:7d' --format ids | mxr archive --yes` — auto-archives newsletter-shaped mail older than a week.
- **Find a shared doc someone sent you:** `mxr search 'from:alice has:link newer_than:14d'` — `has:link` excludes trackers/unsubscribe URLs, so it surfaces real link-bearing replies.
- **Work the owed-reply backlog:** `mxr search 'is:owed-reply' --format ids | mxr remind --when 'friday 16:00'` — pin a Friday-afternoon nudge on every thread you owe a reply on.
- **Owed within the last two weeks only:** `mxr search 'is:owed-reply newer_than:14d'` — skip ancient threads you've implicitly already ignored.
- **Review recent meeting invites:** `mxr search 'has:calendar newer_than:30d' --format ids | xargs -I{} mxr invite show {}` — inspect invite details before deciding whether to RSVP.

## Agent prompts that work

```text
"Find every email mentioning 'launch checklist' since last Monday and
summarise what's still open. Use `mxr search 'launch checklist
newer_than:7d' --format json` and `mxr summarize` for any thread with
4+ messages."
```

```text
"What did Bob email me about pricing in Q1? Use hybrid search and
`--explain` so I can see why each result matched: `mxr search
'from:bob pricing after:2026-01-01 before:2026-04-01' --mode hybrid
--explain`."
```

## See also

- [Labels and saved searches](/guides/labels-and-saved-searches/)
- [Semantic search](/guides/semantic-search/)
- [Recipes — fzf / jq](/guides/recipes/)
- [CLI — Mail retrieval](/reference/cli/#mail-retrieval-and-inspection)
