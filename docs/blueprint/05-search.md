# mxr — Search

## Philosophy

Search is navigation, not a mailbox filter bolted on later.

- lexical search stays exact and fast
- semantic retrieval is optional recall expansion
- hybrid search should help without blurring away literal intent

Semantic search is an `mxr-platform` feature layered on top of the mail runtime. Mail sync, read, labels, drafts, send, and export still fundamentally work without it.

## Search modes

mxr has three search modes:

- `lexical`: Tantivy BM25 only
- `hybrid`: Tantivy BM25 + dense retrieval + Reciprocal Rank Fusion (RRF)
- `semantic`: dense retrieval only, with the same structured filters applied afterward

`lexical` remains the default until semantic search is explicitly enabled.

## Architecture

### Lexical engine

Tantivy remains the exact-search engine:

- BM25 ranking
- field boosts
- phrase and Boolean queries
- attachment filename lookup
- rebuildable on-disk index

The Tantivy index is rebuildable from SQLite.

### Semantic engine

Semantic retrieval lives in `mxr-semantic` and stays local:

- embeddings are generated locally
- vectors and chunk metadata are stored in SQLite
- the in-memory dense ANN index is rebuilt from SQLite
- model weights are cached locally under the mxr data dir

Dense retrieval is derived state, not a new source of truth.

## Semantic storage

Semantic search extends the canonical SQLite store with:

- `semantic_profiles`
- `semantic_chunks`
- `semantic_embeddings`

Important boundary:

- `semantic_chunks` is semantic-ready text storage
- `semantic_embeddings` is profile-specific derived data

Chunk ids stay stable per `message/source_kind/ordinal`. Embeddings are keyed by `chunk_id + profile_id`.

Replacing chunks for a message deletes the old chunks and cascades old embeddings for that message, so stale vectors do not survive content changes.

## What gets indexed

### Lexical

Tantivy indexes:

- subject
- sender name/email
- recipient email
- snippet
- cleaned body text
- attachment filenames
- labels
- flags
- date
- attachment presence

### Semantic

Dense retrieval indexes chunks derived from:

- header summary: subject + participants + snippet
- cleaned message body text
- attachment filename + mime summary
- extracted attachment text when local real-text extraction succeeds

Current semantic chunk source kinds:

- `Header`
- `Body`
- `AttachmentSummary`
- `AttachmentText`

### Attachment extraction scope

Semantic attachment extraction uses real text only:

- plain text attachments
- HTML attachments normalized to text
- Office docs
- spreadsheets
- PDFs with extractable text

Semantic indexing does **not** use OCR.

Skipped:

- image attachments
- scanned/image-only PDFs
- any `tesseract` / `pdftoppm` fallback path

If PDF text extraction fails, mxr skips semantic text extraction for that PDF.

## Query semantics

The user-facing query language does not change when semantic search is enabled.

Examples:

```text
invoice
"deployment plan"
subject:quarterly report
body:house of cards
filename:roadmap
from:alice subject:invoice is:unread
from:alice -subject:spam
```

### Lexical semantics

Lexical search stays literal and field-aware through Tantivy.

Examples:

- `body:house of cards` means body-field lexical retrieval
- `subject:house of cards` means subject-field lexical retrieval
- `filename:house of cards` means attachment filename lexical retrieval

### Dense semantics

Dense retrieval now respects field intent where possible by constraining chunk source kinds:

- unfielded text / phrase: all chunk kinds
- `subject:`: `Header`
- `body:`: `Body`
- `filename:`: `AttachmentSummary` + `AttachmentText`

Examples:

- `body:house of cards`
  - lexical side searches body text literally
  - dense side searches body chunks only
- `subject:house of cards`
  - dense side searches header chunks only
- `filename:house of cards`
  - dense side searches attachment-origin chunks only

This is intentional recall control, not a promise that dense retrieval is exact Boolean field matching.

### Hybrid semantics

Hybrid mode keeps both paths:

1. lexical candidate generation
2. dense candidate generation
3. RRF merge

Literal lexical matches should usually remain stronger in the final ranking. Dense retrieval is there to broaden recall, not to overrule obvious exact hits.

### Structured filters

Structured filters stay authoritative and are never delegated to embeddings:

- `label:`
- `is:`
- `has:`
- `after:`
- `before:`
- `date:`
- `size:`
- account scoping

If a query has no semantic text terms, or negates semantic text terms, mxr falls back to the lexical/filter path.

## Ranking

### Lexical ranking

Field boosts stay intentionally simple:

| Field | Boost |
|---|---:|
| subject | 3.0 |
| from_name | 2.0 |
| from_email | 2.0 |
| snippet | 1.0 |
| attachment_filenames | 0.75 |
| body_text | 0.5 |

### Hybrid ranking

Hybrid search uses RRF with `k = 60`.

Candidate windows:

- lexical: `max(limit * 4, 100)` outside pure lexical mode
- dense: `max((limit + offset + 1) * 8, 200)`

Dense retrieval runs on chunks, then collapses to the best score per message before fusion.

## Indexing lifecycle

Semantic lifecycle is intentionally split into four stages:

1. chunk extraction
2. chunk persistence
3. embedding generation
4. embedding persistence + ANN rebuild/use

Current behavior:

1. During sync, envelopes/bodies are stored and Tantivy is updated.
2. During the same sync pass, semantic chunks are prepared and persisted for changed messages, even if semantic retrieval is disabled.
3. If semantic is disabled, mxr stops there.
4. If semantic is enabled, mxr generates embeddings for the active profile from stored chunks and refreshes the dense ANN index.

### `enabled = false`

`[search.semantic].enabled = false` means:

- no dense retrieval
- no embedding generation
- no active ANN rebuild
- chunk preparation still happens during sync

### Later enablement

When semantic is turned on later:

- mxr installs the active local profile if needed
- backfills missing chunks for messages that do not already have them
- generates embeddings from stored chunks
- rebuilds the active ANN index

This makes enablement cheaper than rebuilding chunk text from scratch for every message.

### Reindex

`mxr semantic reindex` and `mxr doctor --reindex-semantic` remain the full correctness path:

- rebuild chunks from message content
- regenerate embeddings for the active profile
- rebuild the ANN index

Use reindex when:

- chunk generation logic changes
- attachment extraction behavior changes
- profile content needs a full rebuild

## Profiles and local models

Current local profiles:

- `bge-small-en-v1.5`
- `multilingual-e5-small`
- `bge-m3`

Rules:

- embeddings remain local
- model weights are cached locally
- enabling semantic installs the active profile if missing
- switching profiles rebuilds embeddings for the new profile from stored chunks
- profile identity is stored with each embedding row

## Fallback behavior

Fallback behavior should stay boring and honest:

- if semantic support is unavailable in the binary, fallback is lexical
- if semantic is disabled in config, fallback is lexical
- if semantic query extraction yields no text, fallback is lexical
- if semantic query extraction hits negated semantic terms, fallback is lexical
- if dense retrieval returns no candidates in hybrid mode, lexical results still win
- if dense retrieval returns no candidates in semantic mode, fallback is lexical

`mxr search --explain` reports requested mode, executed mode, candidate counts, semantic query text when used, and fallback notes.

## Saved searches

Saved searches persist their search mode.

That means:

- one saved search can stay `lexical`
- another can opt into `hybrid`
- another can use `semantic`

Mode is part of the saved search behavior, not a global toggle.

## Operator notes

- Semantic search is a real mxr platform feature, not a required mail primitive.
- Embeddings are local by default and by design.
- OCR is intentionally out of scope for active semantic indexing.
- Hybrid search is pragmatic, not magical.
- When docs conflict with code, prefer code, then update docs.
- See [semantic-search-audit.md](semantic-search-audit.md) for the code-truth cleanup audit behind this document.
