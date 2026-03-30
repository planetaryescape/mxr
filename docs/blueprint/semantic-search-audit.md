# Semantic Search Audit

This document is the code-truth audit for mxr semantic search as of this cleanup pass.

For the full sync -> SQLite -> lexical -> semantic lifecycle story, see [sync-index-lifecycle-audit.md](sync-index-lifecycle-audit.md).

Primary code paths audited:

- `crates/semantic/src/lib.rs`
- `crates/store/src/semantic.rs`
- `crates/daemon/src/loops.rs`
- `crates/daemon/src/handler/diagnostics/mod.rs`
- `crates/daemon/src/handler/diagnostics/search_execute.rs`
- `crates/daemon/src/handler/diagnostics/search_filter.rs`
- `crates/search/`

## What already existed

Before this pass, mxr already had a substantial semantic stack:

- local semantic profiles
- chunk records in SQLite
- embedding rows in SQLite
- local embedding generation with FastEmbed-backed models
- in-memory ANN index rebuild from SQLite
- `lexical`, `hybrid`, and `semantic` search modes
- RRF-based hybrid ranking
- CLI/TUI profile install, status, enable, disable, and reindex flows

This was already real product/runtime code, not aspirational scaffolding.

## What was already solid

- Semantic search was already local-first. Embeddings stayed on-device and model weights were cached locally.
- Hybrid search already kept Tantivy BM25 as the lexical engine and fused lexical + dense results with RRF instead of pretending the two score spaces were directly comparable.
- Structured filters already stayed authoritative. Dense retrieval never replaced `label:`, `is:`, date, size, or other filter logic.
- Semantic chunks already had useful source kinds: `Header`, `Body`, `AttachmentSummary`, `AttachmentText`.
- Profiles, embeddings, and ANN rebuilds were already modeled as explicit runtime state instead of hidden magic.

## Current ingestion and indexing model

After this pass, the semantic lifecycle is intentionally split into four stages:

1. chunk extraction
2. chunk persistence
3. embedding generation
4. embedding persistence + ANN refresh

Current behavior:

- Sync still writes envelopes/bodies and updates Tantivy immediately.
- Sync now also prepares and persists semantic chunk data for changed messages even when semantic retrieval is disabled.
- `enabled = false` now means:
  - no embedding generation
  - no dense retrieval
  - no active ANN rebuild
  - chunk preparation still happens
- `enabled = true` means:
  - use the stored chunks
  - generate embeddings for the active local profile
  - persist embedding rows
  - rebuild/use the dense ANN index

Store shape:

- `semantic_chunks` is canonical semantic-ready text storage per message/source/ordinal.
- `semantic_embeddings` is profile-specific derived data keyed by `chunk_id + profile_id`.
- Replacing chunks for a message deletes old chunks and cascades old embeddings for that message, so stale vectors do not survive content changes.

## Current profile lifecycle

Current behavior after the cleanup:

- `mxr semantic enable`
  - installs the active local profile if needed
  - backfills missing chunks for messages that do not already have them
  - generates embeddings from stored chunks
  - rebuilds the active ANN index
- `mxr semantic profile use ...`
  - same lifecycle as enable, but for the selected profile
- `mxr semantic reindex` / `mxr doctor --reindex-semantic`
  - full correctness path
  - rebuilds chunks from message content
  - regenerates embeddings for the active profile
  - rebuilds the ANN index

This keeps later enablement cheaper without removing the existing full rebuild path.

## Current hybrid query semantics

Lexical and dense retrieval now have a clearer division of labor:

- lexical search stays literal and field-aware through Tantivy
- dense search broadens recall, but it now respects field intent where possible
- hybrid search still merges lexical + dense with RRF

Current dense source-kind mapping:

- unfielded text / phrase: all chunk kinds
- `subject:`: `Header`
- `body:`: `Body`
- `filename:`: `AttachmentSummary` + `AttachmentText`

Examples:

- `body:house of cards`
  - lexical side searches Tantivy body text
  - dense side searches only body-derived chunks
  - hybrid fuses both
- `subject:house of cards`
  - dense side searches only header chunks
- `filename:house of cards`
  - dense side searches only attachment-origin chunks

This is still pragmatic, not magical. Dense field awareness is approximate recall control, not a new exact Boolean language. Literal lexical matches should still usually win when the words actually match.

## Current OCR stance

Before this pass, the semantic attachment path still had OCR fallbacks:

- image attachments could go through `tesseract`
- PDFs could fall back to `pdftoppm` + OCR when text extraction failed

That is no longer active.

Current extraction scope is real text only:

- header/snippet text
- cleaned message body text
- plain text attachments
- HTML attachments normalized to text
- Office docs
- spreadsheets
- PDFs with extractable text

Current non-scope:

- image attachment OCR
- scanned/image-only PDF OCR
- any `tesseract` / `pdftoppm` semantic indexing path

If PDF text extraction fails, semantic extraction skips that PDF.

## What changed in this pass

- Removed the sync-time `semantic.enabled` gate from chunk preparation.
- Split chunk writes from embedding writes in the store API.
- Reworked semantic enable/profile-use flows to generate embeddings from stored chunks instead of forcing chunk rebuild first.
- Kept full semantic reindex for correctness/profile changes.
- Removed OCR from active semantic attachment extraction.
- Tightened dense query planning so fielded semantic terms constrain dense retrieval by chunk source kind.
- Removed dead duplicate semantic source files that no longer backed the active crate implementation.
- Updated docs so semantic search is documented as a real `mxr-platform` feature, not a vague future capability.

## Why this fits mxr architecture

This cleanup aligns semantic search with mxr's actual architecture:

- Semantic search is useful, but mail must still fundamentally work without it.
- The canonical store remains SQLite.
- Dense indexing remains derived and rebuildable.
- Embeddings remain local, preserving privacy/offline/local-first behavior.
- Exact lexical retrieval remains first-class.
- Semantic retrieval stays a platform/runtime layer on top of the core mail system, not a redefinition of the mail model itself.

That is the intended shape:

- core mail runtime stays boring and reliable
- semantic retrieval stays optional but deeply integrated
- the platform gets smarter without making the fundamentals magical
