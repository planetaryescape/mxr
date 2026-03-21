# Hybrid Search

Cross-phase implementation plan for stabilizing lexical search first, then layering in local semantic retrieval without violating mxr's local-first architecture.

## Non-negotiables

- SQLite stays canonical
- Tantivy stays the default lexical engine
- semantic search is optional, not required for basic search
- model weights are not compiled into the binary
- saved searches persist their search mode
- docs ship in the same change set as feature work

## Rollout Order

### 1. Lexical stabilization

Add daemon-level contract tests that exercise:

- sync -> store -> `Request::Search`
- sync -> store -> `Request::Count`
- CLI-visible search behavior
- saved search execution
- structured filters that currently feel broken

This is the red/green gate. Do not add semantic retrieval until lexical behavior is pinned down.

### 2. Semantic SQLite schema + profile state

Add canonical tables for:

- semantic profiles
- semantic chunks
- semantic embeddings
- saved search mode

Profile rows must track:

- profile name
- backend/model revision
- dimensions
- lifecycle status
- progress
- last error

### 3. Fake embedding backend + contract tests

Before using real models, add deterministic semantic tests through the daemon boundary:

- semantic-only paraphrase retrieval
- hybrid improving over lexical
- structured-only queries skipping dense retrieval
- saved search mode persistence
- profile switch safety

### 4. English default profile backend + lazy download

Implement local profile install/load for:

- `bge-small-en-v1.5`

Requirements:

- download on first semantic enable or explicit install
- cache under the mxr data dir
- keep lexical search working if install fails
- surface clear status/error output

### 5. Multilingual opt-in profile backend

Add:

- `multilingual-e5-small`

Requirements:

- never auto-download unless configured or explicitly installed
- switching to it triggers semantic rebuild
- old lexical behavior remains unaffected during transition

### 6. Attachment extraction

Start with:

- attachment filename + mime summary
- text-like local attachments

Then extend to richer extraction later. Do not block hybrid search baseline on PDF/OCR support.

### 7. Optional `bge-m3` support

Add advanced profile support behind explicit install/use only:

- `bge-m3`

This is not part of the default path.

### 8. CLI/TUI wiring

Wire the public surface:

- `mxr search --mode lexical|hybrid|semantic`
- `mxr count --mode ...`
- `mxr semantic ...`
- `mxr doctor --semantic-status`
- `mxr doctor --reindex-semantic`
- TUI search mode toggle
- saved search mode persistence

### 9. Rebuild, status, profile-swap flows

Complete operator flows:

- rebuild semantic index from SQLite only
- startup load of ready semantic profile
- clear semantic status reporting
- profile install/use progress
- keep old profile serving until new one is ready

## Test Matrix

Lexical:

- text search after sync
- label/date/flag filters
- count/search parity
- saved search execution

Semantic:

- English semantic retrieval
- multilingual retrieval only when multilingual profile is active
- hybrid improvement on semantic phrasing
- structured-only lexical fallback

Lifecycle:

- first enable downloads only active profile
- switching English -> multilingual triggers rebuild
- failed download does not break lexical search
- dense index rebuild from SQLite works

## Operator Notes

- default profile is English because it is lighter
- multilingual is explicit opt-in
- only configured profiles are downloaded
- local profiles keep content on-device
