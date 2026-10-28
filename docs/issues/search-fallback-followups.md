# Search fallback follow-ups

The SQLite fallback (used while the lexical index is stopped or rebuilding) matches subject, sender name and preview. Two small differences from the index remain.

## Decomposed accents split compound words differently

A compound query typed with a decomposed accent, such as `naïve-beta` with `i` followed by U+0308, is split by the index's tokenizer into `nai`, `ve`, `beta`. FTS5's `unicode61` tokenizer stores `naive`, `beta`, so the fallback's AND of tokens finds nothing where the index matches (`crates/daemon/src/handler/diagnostics/search_fallback.rs`, compound-term splitting).

Fix direction: NFC-normalise query text before splitting, or split with FTS5's own rules and strip diacritics the way `unicode61` does.

## A bare word that is exactly a sender address

The index's text query includes `from_email` as a whole-address term, so a bare `alice@example.com` matches mail from that address. The fallback's bare words search subject, sender name and preview only, so that query finds such mail only when the address also appears in those fields.

Fix direction: for a bare term containing `@`, also accept `LOWER(m.from_email) = LOWER(term)`, within the same AND.
