# mxr — Search

## Philosophy

Search is not an afterthought bolted onto a folder browser. It is a first-class navigation primitive. Power users don't browse folders — they search.

mxr's search should feel as fast and responsive as Spotlight or Alfred: type a query, get results instantly, navigate with keyboard.

## Architecture

### Primary engine: Tantivy

Tantivy is a Rust-native full-text search engine inspired by Apache Lucene. It provides:
- BM25 ranking (the standard relevance algorithm used by search engines)
- Field-level boosts (weight subject higher than body)
- Boolean queries (AND, OR, NOT)
- Phrase queries ("exact phrase matching")
- Faceted search (filter by label, date range, account)
- Sub-second results on large corpora (100k+ documents)

We considered SQLite FTS5 but rejected it as the primary engine because:
- FTS5's BM25 implementation is basic (no field boosts, limited query syntax)
- FTS5 gets slow on large mailboxes (100k+ messages)
- FTS5 can't do faceted filtering efficiently
- Tantivy is purpose-built for this; FTS5 is a SQLite feature

We keep FTS5 as a lightweight fallback (see data model doc) but Tantivy does all primary search work.

### Index structure

Tantivy index lives on disk alongside the SQLite database.

Location: `$XDG_DATA_HOME/mxr/search_index/` (Linux) or `~/Library/Application Support/mxr/search_index/` (macOS).

The index is always rebuildable from SQLite. If it gets corrupted or out of sync, `mxr doctor --reindex` rebuilds it from scratch.

### Indexed fields

```rust
pub fn build_schema() -> tantivy::schema::Schema {
    let mut builder = Schema::builder();

    // Stored fields (returned with results)
    builder.add_text_field("message_id", STRING | STORED);
    builder.add_text_field("account_id", STRING | STORED);
    builder.add_text_field("thread_id", STRING | STORED);

    // Searchable fields with BM25 ranking
    builder.add_text_field("subject", TEXT);       // High boost
    builder.add_text_field("from_name", TEXT);     // Medium boost
    builder.add_text_field("from_email", STRING);  // Exact match
    builder.add_text_field("to_email", STRING);    // Exact match
    builder.add_text_field("snippet", TEXT);        // Low boost
    builder.add_text_field("body_text", TEXT);     // Low boost (added when body is fetched)

    // Facet/filter fields
    builder.add_text_field("labels", STRING);       // Multi-valued
    builder.add_date_field("date", INDEXED | STORED);
    builder.add_u64_field("flags", INDEXED);
    builder.add_bool_field("has_attachments", INDEXED);

    builder.build()
}
```

### Field boosts

When scoring results, fields are weighted:

| Field | Boost | Rationale |
|---|---|---|
| subject | 3.0 | Most indicative of relevance |
| from_name | 2.0 | People search by sender often |
| from_email | 2.0 | Exact sender matches are high intent |
| snippet | 1.0 | Preview text, moderate signal |
| body_text | 0.5 | Body contains noise (signatures, quotes, boilerplate) |

These boosts are configurable but these defaults should work well for most users.

### Indexing lifecycle

1. **During sync**: New envelopes are indexed immediately (subject, from, snippet). Body text is NOT indexed at sync time because bodies haven't been fetched yet.
2. **During body fetch**: When a user opens a message and the body is fetched, the Tantivy document is updated to include `body_text`. This means the index gets richer over time.
3. **During re-index**: `mxr doctor --reindex` drops the Tantivy index and rebuilds from all data in SQLite (messages + bodies tables).

This progressive indexing strategy means search works immediately after sync (against headers/snippets) and improves as the user reads messages.

## Query syntax

mxr supports a query language that feels natural but is precise when needed:

```
# Simple text search (searches subject, from, snippet, body)
invoice

# Exact phrase
"deployment plan"

# Field-specific
from:alice@example.com
to:bob@example.com
subject:quarterly report

# Boolean
from:alice AND subject:invoice
budget OR forecast
NOT spam

# Labels/filters
label:work
label:newsletters
is:unread
is:starred
is:read
has:attachment

# Date ranges
after:2026-01-01
before:2026-03-15
date:2026-03-17         # Specific date
date:today
date:yesterday
date:this-week
date:this-month

# Combinations
from:alice subject:invoice after:2026-01-01 is:unread

# Negation
from:alice -subject:spam
label:work -label:archived
```

The query parser translates this syntax into Tantivy queries. It lives in the `search` crate.

### Implementation note

The query parser should be a separate, well-tested module. It takes a string and produces a `tantivy::query::Query`. This is the kind of thing that accumulates edge cases, so comprehensive test coverage is essential.

## Saved searches

Saved searches are a core primitive, not a nice-to-have. They are user-programmed inbox lenses.

### What they are

A saved search is a stored query string with a name and optional account filter. Results are computed live — they are NOT materialized views.

### Where they appear

1. **Sidebar**: Listed below labels, always visible
2. **Command palette**: Searchable by name
3. **CLI**: `mxr search --saved "Unread invoices"`

### Examples

```
Name: "Unread invoices"
Query: subject:invoice is:unread

Name: "Newsletters this week"
Query: label:newsletters date:this-week

Name: "From team"
Query: from:alice@work.com OR from:bob@work.com OR from:carol@work.com

Name: "Large attachments"
Query: has:attachment size:>5mb

Name: "Waiting for reply"
Query: is:sent after:2026-03-10 -label:replied
```

### Schema

```sql
CREATE TABLE IF NOT EXISTS saved_searches (
    id          TEXT PRIMARY KEY,
    account_id  TEXT,              -- NULL = all accounts
    name        TEXT NOT NULL,
    query       TEXT NOT NULL,
    sort_order  TEXT NOT NULL DEFAULT 'date_desc',
    icon        TEXT,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);
```

### CRUD via command palette

```
Ctrl-P → "Create saved search" → Name: "Unread invoices" → Query: subject:invoice is:unread → Done
Ctrl-P → "Edit saved search" → select → modify → Done
Ctrl-P → "Delete saved search" → select → confirm → Done
```

Or via CLI:
```
mxr search --save "Unread invoices" "subject:invoice is:unread"
```

## Search phase roadmap

### Phase 1 (v0.1) — what we build first

- Tantivy with BM25
- Field boosts (subject, from, snippet, body)
- Query parser supporting: text, phrases, field:value, boolean, date ranges, label/flag filters
- Saved searches (create, list, delete, execute)
- Progressive indexing (headers at sync, body on fetch)
- Command palette integration
- CLI search (`mxr search "query"`)
- Reindex command (`mxr doctor --reindex`)

### Phase 2 (future) — hybrid search with vector retrieval

We explicitly decided NOT to build vector search in v0.1. The reasoning:

- Embeddings pipeline, vector persistence, incremental updates, model packaging, local inference performance, and ranking fusion tuning all create a second system before the first is proven
- BM25 via Tantivy is already better search than any terminal email client ships today
- Vector search adds significant complexity and binary size (ML model)

When we do build it:

- **Embeddings**: Local model via `candle` (Hugging Face's Rust ML framework) with a small model like `all-MiniLM-L6-v2`
- **Vector index**: `usearch` or `hnsw_rs` (pure Rust ANN indexes)
- **Fusion**: Reciprocal Rank Fusion (RRF) combining BM25 and vector results

```rust
// RRF is simple to implement once both result sets exist
fn reciprocal_rank_fusion(
    bm25: Vec<ScoredDoc>,
    vector: Vec<ScoredDoc>,
    limit: usize,
) -> Vec<SearchResult> {
    let k = 60.0;  // Standard RRF constant
    let mut scores: HashMap<MessageId, f64> = HashMap::new();

    for (rank, doc) in bm25.iter().enumerate() {
        *scores.entry(doc.id.clone()).or_default()
            += 1.0 / (k + rank as f64 + 1.0);
    }
    for (rank, doc) in vector.iter().enumerate() {
        *scores.entry(doc.id.clone()).or_default()
            += 1.0 / (k + rank as f64 + 1.0);
    }

    let mut results: Vec<_> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    results.truncate(limit);
    results.into_iter().map(|(id, score)| SearchResult { id, score }).collect()
}
```

This gives semantic search ("emails about the deployment issue last month") combined with exact keyword matching ("find invoice INV-2024-0847"). But it's phase 2, after BM25 is proven.
