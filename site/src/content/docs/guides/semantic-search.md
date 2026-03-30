---
title: Semantic search
description: Enable and operate local semantic search in mxr.
---

## What it is

mxr supports three search modes:

- `lexical`
- `hybrid`
- `semantic`

Semantic search is an optional `mxr-platform` feature layered on top of the core mail runtime.

- mail still works without it
- embeddings stay local
- hybrid keeps lexical BM25 and adds dense recall with RRF
- OCR is not used for semantic indexing

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

## Enable it

```toml
[search]
default_mode = "hybrid"

[search.semantic]
enabled = true
auto_download_models = true
active_profile = "bge-small-en-v1.5"
```

Then:

```bash
mxr semantic status
```

## First enable expectations

On first enable, mxr may:

1. install/download the selected local model
2. backfill missing semantic chunks
3. generate embeddings from stored chunks
4. rebuild the dense ANN index

This can take longer than a normal search. After that, sync keeps semantic chunk prep warm for changed messages.

## What `enabled = false` means

`enabled = false` does **not** mean semantic-ready data is absent.

Current behavior:

- sync still prepares semantic chunks for changed messages
- embeddings are not generated
- dense retrieval is off
- lexical search still works normally

That makes later enablement cheaper.

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
- the query has no semantic text terms
- the query negates semantic text terms
- dense retrieval returns no candidates

Use `--explain` to see the requested mode, executed mode, dense/lexical candidate counts, and fallback notes.
