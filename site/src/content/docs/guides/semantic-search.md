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

mxr does **not** use OCR for:

- image attachments
- scanned/image-only PDFs

If PDF text extraction fails, that PDF is skipped for semantic text extraction.

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

Use `mxr semantic reindex` only when chunk extraction behavior changed or you want a full correctness rebuild.

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

Full rebuild:

```bash
mxr semantic reindex
mxr doctor --reindex-semantic
```

Use reindex when chunk extraction behavior changed or when you want a full correctness rebuild.

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
- `body:`: body chunks
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
