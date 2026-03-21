# mxr — Search

## Philosophy

Search is a navigation primitive, not a mailbox add-on. The default path must stay instant and offline. Semantic retrieval is layered on top of that, not used as an excuse to weaken exact search.

## Retrieval layers

mxr now has three search modes:

- **`lexical`**: Tantivy BM25 only. Default behavior. Fastest path.
- **`hybrid`**: Tantivy BM25 + dense retrieval fused with Reciprocal Rank Fusion (RRF).
- **`semantic`**: Dense retrieval only, with the same structured filters applied afterward.

`lexical` remains the default until semantic search is explicitly enabled.

## Architecture

### Lexical engine: Tantivy

Tantivy remains the primary exact-search engine. It provides:

- BM25 ranking
- field boosts
- boolean queries
- phrase queries
- faceted filtering
- rebuildable on-disk index

The Tantivy index lives on disk alongside SQLite:

- Linux: `$XDG_DATA_HOME/mxr/search_index/`
- macOS: `~/Library/Application Support/mxr/search_index/`

The index is always rebuildable from SQLite. `mxr doctor --reindex` rebuilds it from scratch.

### Semantic engine: local embeddings + rebuildable ANN

Semantic search is local-first:

- vectors and chunk metadata are stored in SQLite
- the in-memory dense ANN index is rebuilt from SQLite at startup or after semantic reindex
- local model weights are cached on disk, not baked into the binary

Canonical storage stays in SQLite. Dense retrieval is an acceleration layer, not a new source of truth.

### SQLite semantic tables

Semantic search extends the canonical store with:

```sql
ALTER TABLE saved_searches
    ADD COLUMN search_mode TEXT NOT NULL DEFAULT '"lexical"';

CREATE TABLE semantic_profiles (
    id TEXT PRIMARY KEY,
    profile_name TEXT NOT NULL UNIQUE,
    backend TEXT NOT NULL,
    model_revision TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    status TEXT NOT NULL,
    installed_at INTEGER,
    activated_at INTEGER,
    last_indexed_at INTEGER,
    progress_completed INTEGER NOT NULL DEFAULT 0,
    progress_total INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE TABLE semantic_chunks (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    normalized TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE semantic_embeddings (
    chunk_id TEXT NOT NULL REFERENCES semantic_chunks(id) ON DELETE CASCADE,
    profile_id TEXT NOT NULL REFERENCES semantic_profiles(id) ON DELETE CASCADE,
    dimensions INTEGER NOT NULL,
    vector_blob BLOB NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (chunk_id, profile_id)
);
```

## Indexed content

### Lexical fields

Tantivy indexes:

- subject
- sender name/email
- recipient email
- snippet
- cleaned body text
- labels
- date
- flags
- attachment presence

Body text is indexed during sync, not lazily on first open.

### Semantic chunks

Dense retrieval indexes message chunks, not whole-message blobs. Chunks are built from:

- header summary: subject + participants + snippet
- cleaned body text
- attachment filename + mime summary
- extracted attachment text when available locally

Chunk ids are stable per message/source/ordinal. Embeddings are keyed by chunk id + profile id.

## Ranking

### Lexical ranking

Field boosts remain intentionally simple:

| Field | Boost | Why |
|---|---|---|
| subject | 3.0 | strongest exact signal |
| from_name | 2.0 | sender intent matters |
| from_email | 2.0 | exact sender matches are high-intent |
| snippet | 1.0 | useful but noisy |
| body_text | 0.5 | broad recall, lower precision |

### Hybrid ranking

Hybrid mode uses Reciprocal Rank Fusion with `k = 60`.

Candidate windows:

- BM25: `max(limit * 4, 100)`
- dense: `max(limit * 8, 200)`

Dense retrieval runs on chunks, then collapses to the best hit per message before fusion.

RRF is intentionally used instead of hand-tuned score normalization because BM25 and cosine-like dense scores are not naturally comparable.

### Structured filters stay authoritative

Structured filters are never delegated to embeddings. They apply after candidate generation in `hybrid` and `semantic` modes:

- `label:`
- `is:`
- `has:`
- `after:`
- `before:`
- `date:`
- `size:`
- account scoping

If a query is purely structured, mxr skips dense retrieval and uses the lexical/filter path only.

## Query syntax

The user-facing query language does not change when semantic search is enabled:

```text
invoice
"deployment plan"
from:alice@example.com
subject:quarterly report
from:alice AND subject:invoice
label:work
is:unread
has:attachment
after:2026-01-01
before:2026-03-15
date:today
from:alice subject:invoice after:2026-01-01 is:unread
from:alice -subject:spam
```

Semantic text is extracted from free text, phrases, and semantic-capable fields such as `subject:` and attachment filename/body text. Structured filters still behave exactly as structured filters.

## Saved searches

Saved searches remain live queries, but now persist their search mode:

```sql
CREATE TABLE saved_searches (
    id TEXT PRIMARY KEY,
    account_id TEXT,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    search_mode TEXT NOT NULL DEFAULT '"lexical"',
    sort_order TEXT NOT NULL DEFAULT 'date_desc',
    icon TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

This means "Unread invoices" can stay lexical while another saved search uses hybrid mode.

## Semantic profiles

### Default profile

Default local profile:

- `bge-small-en-v1.5`

Reason: smaller and faster for the majority-English default path.

### Opt-in multilingual profile

Standard multilingual profile:

- `multilingual-e5-small`

It is only downloaded if the user explicitly selects it.

### Optional advanced profile

Optional heavier profile:

- `bge-m3`

This is explicit-install only. It is not downloaded automatically.

### Model delivery

Model weights are cached under the mxr data directory:

- Linux: `$XDG_DATA_HOME/mxr/models/`
- macOS: `~/Library/Application Support/mxr/models/`

Rules:

- enabling semantic search downloads only the active profile if missing
- switching profiles downloads the new profile if needed, then rebuilds embeddings
- profile identity is stored with each embedding so model changes do not corrupt ranking

## Indexing lifecycle

1. **During sync**: envelopes and bodies are written to SQLite and indexed in Tantivy immediately.
2. **After sync**: changed message ids are queued for semantic reindex.
3. **During semantic reindex**: chunk text is normalized, embedded with the active profile, stored in SQLite, then loaded into the dense ANN index.
4. **During profile switch**: the new profile is installed if needed, embeddings are rebuilt, then the active profile flips once the new profile is ready.
5. **During repair**: `mxr doctor --reindex-semantic` rebuilds the active semantic profile from SQLite.

Label, read, and star changes do not trigger re-embedding. Content changes and profile changes do.

## Operator notes

- English semantic search is the default because it is smaller and faster.
- Multilingual support is first-class, but opt-in.
- Only configured profiles are downloaded.
- Local profiles keep message content on-device.
- A future cloud backend may exist behind the same profile abstraction, but local remains the default product story.

## Current rollout

Shipped baseline:

- Tantivy BM25 lexical search
- semantic profiles in SQLite
- English default profile with opt-in multilingual profiles
- hybrid search via RRF
- saved searches with per-search mode
- CLI/TUI mode selection and semantic status/reindex flows

Future refinement:

- richer attachment extraction (PDF/OCR/office docs)
- better semantic explain/debug output
- optional cloud embedding backends behind the same profile contract
