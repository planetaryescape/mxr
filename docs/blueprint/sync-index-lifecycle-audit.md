# Sync / Index / Semantic Lifecycle Audit

This document is the code-truth audit for the mail sync -> SQLite -> lexical index -> semantic runtime lifecycle after the lifecycle-hardening pass.

Primary code paths audited:

- `crates/sync/src/engine.rs`
- `crates/daemon/src/loops.rs`
- `crates/daemon/src/handler/diagnostics/mod.rs`
- `crates/daemon/src/server.rs`
- `crates/semantic/src/lib.rs`
- `crates/store/src/semantic.rs`
- `crates/daemon/src/handler/diagnostics/search_execute.rs`
- `crates/daemon/src/handler/diagnostics/search_filter.rs`

See also [semantic-search-audit.md](semantic-search-audit.md) for the semantic-only cleanup audit that sits underneath this broader lifecycle view.

## What sync guarantees now

For each upserted message in a sync batch, mxr currently guarantees:

1. envelope is written to SQLite
2. body is written to SQLite
3. label associations are updated in `message_labels`
4. Tantivy is updated with body-aware lexical content for that message
5. the lexical batch is committed before sync finishes
6. label counts are recalculated for the account
7. accounts without stable native thread ids are rethreaded
8. the sync cursor is advanced

This is the immediate core-mail guarantee. Mail read/search correctness does not depend on semantic retrieval being enabled.

## What lexical indexing guarantees now

Lexical search is the immediate freshness path.

Current behavior:

- sync indexes body-aware search documents during the same sync batch
- sync commits the Tantivy writer at the end of that batch
- after the batch commit, lexical search sees the new/changed mail
- daemon startup can repair/rebuild the lexical index from SQLite if Tantivy is partial or stale

Lexical freshness is therefore:

- immediate after a completed sync batch
- repairable from canonical SQLite state

## What semantic indexing guarantees now

Semantic lifecycle is intentionally split into four stages:

1. chunk extraction / normalization
2. chunk persistence
3. embedding generation
4. embedding persistence + ANN rebuild/use

Current behavior after sync:

- the daemon takes `upserted_message_ids` from sync
- semantic ingest prepares normalized chunks for those messages
- semantic ingest persists `semantic_chunks` even if semantic retrieval is disabled
- if semantic retrieval is disabled, ingest stops there
- if semantic retrieval is enabled, ingest also:
  - installs/uses the active local profile
  - generates embeddings from the stored chunks
  - persists `semantic_embeddings`
  - refreshes the active in-memory ANN index

This means semantic-ready text storage is warmed continuously, while embeddings remain feature-gated derived state.

## What repair and recovery behavior exists

Current repair/recovery behavior:

- Gmail cursor recovery
  - if a Gmail cursor is invalid/not-found, sync resets to `Initial` once and retries
- Junction-table corruption recovery
  - if a label-capable provider has messages but `message_labels` is empty, sync resets cursor and re-runs a full rebuild path
- Lexical startup repair
  - on daemon startup, if Tantivy doc count does not match SQLite message count, mxr rebuilds the lexical index from SQLite
- Semantic later-enable backfill
  - when a profile is enabled/used later, mxr backfills missing chunks only for messages that do not already have them

What does **not** currently happen:

- no startup semantic repair pass
- no OCR fallback repair path
- no cloud embedding recovery path

## The gap this task closed

Before this pass, the intended lifecycle story and the runtime naming were out of sync:

- sync-time semantic work conceptually behaved like ingest, but the public API name suggested full reindex
- docs described the lifecycle across multiple files, but not in one clear sync-to-search audit
- some top-level docs did not state plainly when lexical search is fresh vs when semantic search is merely prepared

This pass keeps the implementation shape mostly intact, but makes the lifecycle explicit:

- sync-time semantic work is now named as ingest
- daemon call sites use the ingest path directly
- comments now fence core sync guarantees vs optional semantic work
- docs now describe one consistent lifecycle story

## Final lifecycle after this task

When new mail arrives:

1. envelope + body are stored in SQLite immediately
2. Tantivy is updated immediately for lexical search and committed per sync batch
3. labels, label counts, threading, and cursor updates complete as usual
4. the daemon ingests semantic chunks for newly upserted messages and persists `semantic_chunks`
5. if semantic is disabled, mxr stops there
6. if semantic is enabled, mxr generates embeddings from the stored chunks, persists them, and refreshes the ANN index
7. if semantic is enabled later, mxr reuses stored chunks and only backfills missing ones before embedding

Net effect:

- lexical search stays immediately fresh after sync
- semantic readiness is optional/platform-level
- later semantic enablement is cheaper because chunk prep is already done
- embeddings remain local
- OCR is not part of active semantic indexing

## Practical examples

### Normal sync, semantic disabled

- `mxr sync`
- SQLite has the new envelope/body
- Tantivy can find the message immediately
- `semantic_chunks` are present
- `semantic_embeddings` are absent

### Normal sync, semantic enabled

- `mxr sync`
- SQLite and Tantivy update as above
- semantic ingest persists chunks
- active-profile embeddings are generated from those chunks
- hybrid/semantic search can use the refreshed ANN index

### Enable semantic later

- sync mail for days/weeks with `enabled = false`
- later run `mxr semantic profile use bge-small-en-v1.5`
- mxr reuses stored chunks, backfills only missing ones, generates embeddings, and rebuilds the active ANN index

### Reindex

Use:

- `mxr semantic reindex`
- `mxr doctor --reindex-semantic`

when:

- chunk extraction logic changed
- attachment text extraction behavior changed
- you want a full correctness rebuild for the active profile

Reindex rebuilds chunks from message content, then regenerates embeddings.
