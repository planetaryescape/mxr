# Search fallback follow-ups

The SQLite fallback (used while the lexical index is stopped or rebuilding) matches subject, sender name and preview. Two small differences from the index remain.

## Decomposed accents split compound words differently

A compound query typed with a decomposed accent, such as `naïve-beta` with `i` followed by U+0308, is split by the index's tokenizer into `nai`, `ve`, `beta`. FTS5's `unicode61` tokenizer stores `naive`, `beta`, so the fallback's AND of tokens finds nothing where the index matches (`crates/daemon/src/handler/diagnostics/search_fallback.rs`, compound-term splitting).

Fix direction: NFC-normalise query text before splitting, or split with FTS5's own rules and strip diacritics the way `unicode61` does.

## A bare word that is exactly a sender address

The index's text query includes `from_email` as a whole-address term, so a bare `alice@example.com` matches mail from that address. The fallback's bare words search subject, sender name and preview only, so that query finds such mail only when the address also appears in those fields.

Fix direction: for a bare term containing `@`, also accept `LOWER(m.from_email) = LOWER(term)`, within the same AND.

## Rules still match the raw body text

Search indexes the reader's cleaned text, but rules do not. `RuleMessage` sets `body_text` from `text_plain` only, and its `MessageView` implementation reads `text_plain` or `text_html` for other body fields (`crates/daemon/src/loops.rs:1854` and `:1899`). A rule on body text can match quoted replies, signatures or markup that search now ignores. Changing what rules match is a separate behaviour change, so it stays out of the reader-text change.

Fix direction: decide whether rules should match the reader text, then route `RuleMessage` through `mxr_reader::clean` the same way the index does, and say so in the rules guide.

## Changing html_command does not rebuild the index

The index cleans HTML with the `render.html_command` in effect when the daemon starts. Changing that setting does not mark the index stale, so existing mail keeps the old text until the next content-version bump or a manual `mxr doctor --reindex`. A configured command also runs once per HTML message during a rebuild, so a 50k-message rebuild starts one external process per message (`crates/daemon/src/state.rs`, `open_search_index` call in `AppState::new`).

Fix direction: include a fingerprint of the effective reader config in the content-version marker, or keep the index on the built-in renderer and document that search and the reader can differ for `html_command` users.
