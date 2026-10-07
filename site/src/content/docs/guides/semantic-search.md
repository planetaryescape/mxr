---
title: Semantic search
description: Operate mxr's local, opportunistic semantic search layer.
---

## What it is

mxr supports three search modes:

- `lexical`
- `hybrid`
- `semantic`

Semantic search is an `mxr-platform` feature layered on top of the core mail runtime. The default config enables semantic work opportunistically, but `lexical` remains the default search mode and the immediate correctness path.

- mail still works without it
- embeddings stay local
- hybrid keeps lexical BM25 and adds dense recall with RRF
- OCR is not used for semantic indexing
- dense retrieval falls back to lexical ranking when unavailable or unhealthy

## What gets indexed semantically

mxr prepares semantic chunks from:

- subject/participants/snippet
- cleaned message body text
- plain text attachments
- HTML attachments normalized to text
- Office docs
- spreadsheets
- PDFs with extractable text
- passages you [highlighted in Reading](/guides/reading/#highlight-what-you-want-to-keep), one chunk each with its note, so a search finds the passage you kept rather than the window it fell in

The cleaned body now also drops quoted history marked by Gmail, Apple Mail
and Outlook in HTML mail, so a reply's chunks hold what it said rather than
the message it quoted.

Per-mode index recipes are on the way. The Messages recipe (each message's
new text prefixed with the person and topic, "Samir Patel · Contract
renewal", plus the conversation's gist) is defined in code as version 1;
search keeps today's chunks until a retrieval eval on real mail shows the
recipe finds more.

mxr does **not** use OCR for:

- image attachments
- scanned/image-only PDFs

If PDF text extraction fails, that PDF is skipped for semantic text extraction.

Attachments are indexed once they are on disk, which for most mail means
once you open them. Archive is the exception: mxr downloads the PDFs of
emails filed as [records](/guides/archive/) ahead of time, within
`records.pdf_budget_mb` (512 MB by default), so their text is indexed.

### Archive's emails get one more chunk

An email that is a record's source gets a field chunk beside its header:
the record's kind, issuer, what it is, its reference and its place
("record booking TAP Air Portugal LHR -> LIS TP1357 K7QX2M Lisbon"). When
new mail is filed, its emails are ingested again so the chunk lands even if
the semantic worker read them first, and a prefetched PDF triggers the same;
chunks that didn't change are not embedded again. Records filed from older
mail get their chunk at the next reindex (`mxr semantic reindex`).
Reference numbers also stay in the keyword index, so exact matches like
`402-118` keep working in lexical and hybrid search.

## Default config

```toml
[search]
default_mode = "lexical"

[search.semantic]
enabled = true
auto_download_models = true
active_profile = "bge-small-en-v1.5"
```

Use hybrid or semantic mode explicitly when you want dense recall:

```bash
mxr search "house of cards" --mode hybrid
mxr search "house of cards" --mode semantic
```

Check runtime readiness with:

```bash
mxr semantic status
```

## First profile activation expectations

On first profile activation, mxr may:

1. install/download the selected local model
2. backfill missing semantic chunks
3. generate embeddings from stored chunks
4. rebuild the dense ANN index

This can take longer than a normal search. After that, sync keeps semantic chunk prep warm for changed messages. If model/profile work fails, lexical search still works.

## When semantic search is ready

Lexical search freshness and semantic readiness are different things:

- sync writes mail to SQLite immediately
- lexical search is fresh after the sync batch commit
- semantic chunks are also persisted after sync
- semantic search becomes ready when the active profile has embeddings + ANN state
- hybrid/semantic search falls back to lexical ranking when dense retrieval is unavailable

Use:

```bash
mxr semantic status
mxr doctor --semantic-status
```

to see whether the active profile is actually ready.

## What explicit `enabled = false` means

`enabled = false` does **not** mean semantic-ready data is absent.

Current behavior:

- sync still prepares semantic chunks for changed messages
- embeddings are not generated
- dense retrieval is off
- lexical search still works normally

That makes later enablement cheaper.

## Turn semantic back on later

Typical flow:

1. explicitly run with `enabled = false` for normal sync/read/search
2. later enable or `mxr semantic profile use ...`
3. mxr reuses stored chunks, backfills only missing ones, then builds embeddings

Use `mxr semantic reindex` only when chunk extraction behavior changed or you want a full correctness rebuild; it skips whatever is already embedded, so it is cheap to re-run.

## Status, profiles, and reindex

Inspect current status:

```bash
mxr semantic status
mxr doctor --semantic-status
```

Install a profile without switching:

```bash
mxr semantic profile install multilingual-e5-small
```

Switch profiles:

```bash
mxr semantic profile use multilingual-e5-small
```

Rebuild:

```bash
mxr semantic reindex           # resumable; skips messages already embedded for this profile
mxr semantic reindex --force   # re-embed everything, current vectors included
mxr doctor --reindex-semantic
```

An ordinary `mxr semantic reindex` is idempotent and stepped: it walks the corpus, skips messages whose chunks and embeddings already match the active profile, and persists its position as it goes, so re-running one that was interrupted picks up where it stopped rather than starting over. Only one index pass runs at a time — a second request joins the pass in flight.

Reach for `--force` only to recover a corrupt or half-migrated index, or after a change to chunk extraction that makes the stored vectors wrong rather than missing. On a large mailbox it re-embeds every message, which is the slowest thing mxr does.

Watch it from another shell:

```bash
mxr semantic status --format json \
  | jq -r '.profiles[] | "\(.profile) \(.status) \(.progress_completed)/\(.progress_total)"'
```

What you get: one line per installed profile with its status and how far its index pass has got.

## Query examples

```bash
mxr search "house of cards" --mode hybrid
mxr search "body:house of cards" --mode hybrid --explain
mxr search "subject:house of cards" --mode hybrid --explain
mxr search "filename:house of cards" --mode hybrid --explain
```

Current dense source intent:

- unfielded text: all chunk kinds
- `subject:`: header chunks
- `body:`: body chunks and Reading highlights
- `filename:`: attachment-origin chunks

Lexical search still handles literal field matching. Dense retrieval broadens recall inside the intended source area.

## Fallback behavior

mxr falls back to lexical behavior when:

- semantic support is unavailable in the binary
- semantic is disabled
- dense retrieval errors
- the query has no semantic text terms
- the query negates semantic text terms
- dense retrieval returns no candidates

Use `--explain` to see the requested mode, executed mode, dense/lexical candidate counts, and fallback notes.
