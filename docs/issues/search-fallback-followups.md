# Search fallback follow-ups

The SQLite fallback (used while the lexical index is stopped or rebuilding) matches subject, sender name and preview. Two small differences from the index remain.

## Decomposed accents split compound words differently

A compound query typed with a decomposed accent, such as `naïve-beta` with `i` followed by U+0308, is split by the index's tokenizer into `nai`, `ve`, `beta`. FTS5's `unicode61` tokenizer stores `naive`, `beta`, so the fallback's AND of tokens finds nothing where the index matches (`crates/daemon/src/handler/diagnostics/search_fallback.rs`, compound-term splitting).

Fix direction: NFC-normalise query text before splitting, or split with FTS5's own rules and strip diacritics the way `unicode61` does.

## A bare word that is exactly a sender address

The index's text query includes `from_email` as a whole-address term, so a bare `alice@example.com` matches mail from that address. The fallback's bare words search subject, sender name and preview only, so that query finds such mail only when the address also appears in those fields.

Fix direction: for a bare term containing `@`, also accept `LOWER(m.from_email) = LOWER(term)`, within the same AND.

## Rules still match the raw body text

Search indexes the reader's cleaned text, but rules do not. `RuleMessage` sets `body_text` from `text_plain` only, and its `MessageView` implementation reads `text_plain` or `text_html` for other body fields (`crates/daemon/src/loops.rs:1854` and `:1899`). A rule on body text can match words that search does not index, such as markup and script contents. Changing what rules match is a separate behaviour change, so it stays out of the reader-text change.

Fix direction: decide whether rules should match the reader text, then route `RuleMessage` through `mxr_reader::clean` the same way the index does, and say so in the rules guide.

## Search uses the built-in HTML converter, not html_command

The index cleans HTML with the built-in converter even when `render.html_command` is set, because a rebuild would otherwise start one external process per HTML message (`crates/daemon/src/state.rs`, `open_search_index` call in `AppState::new`). For `html_command` users, a word the external renderer produces differently (table layout, entity handling) can read one way in the reader and match another way in search.

Fix direction: only if this shows up in practice, run the external command in batches or cache its output per body hash, and include the setting in the content-version marker.

## The reader's HTML conversion panics on malformed tables

html2text 0.17.4 panics on some malformed tables, for example `<td colspan="18446744073709551615">` (unchecked column arithmetic). The search index now catches the panic and indexes that part as tag-stripped text (`crates/search/src/index.rs`, `convert_part`). The reader display path does not, so the same message panics there. Verified by calling `mxr_reader::clean` on the input: it panics.

Display call sites that reach the built-in converter with HTML bodies, not fixed here:

- `crates/tui/src/app/body_helpers.rs:396` (reader view, HTML body source), `:395` and `:397` for plain parts that look like HTML
- `crates/tui/src/runner.rs:6116`
- `crates/daemon/src/commands/cat.rs:24` (`mxr cat` reader output)
- Daemon handlers that clean bodies for LLM or assistant features: `crates/daemon/src/handler/promises.rs:98`, `updates.rs:109`, `todos.rs:926`, `thread_gist.rs:534`, `todo_resolution.rs:130`, `safety_llm.rs:95`, `commitments_extract.rs:67`, `decisions_extract.rs:97`, `briefing.rs:78`, `archive_ask.rs:103`

Fix direction: catch the panic in `mxr_reader::clean` itself and return the same fallback text the index uses, or pin html2text to a version that checks the arithmetic. The first covers every caller at once.
