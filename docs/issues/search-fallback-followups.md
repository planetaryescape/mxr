# Search fallback follow-ups

Two behaviours differ from the lexical index on purpose, and one limitation of paging is kept on record. Each entry says why it stays.

## Search `+c++` matches the word `c`; rules match the literal `c++`

The index cuts query terms on non-alphanumeric characters (`tokenize_text_value` in `crates/search/src/query_builder.rs`), so `+c++` searches the token `c` and finds any mail with the word "c". A rule's `c++` matches the literal text `c++`, as `+c++` in a rule does too (`StringMatch::whole_word`). The two forms disagree for the same typed word. Making search keep the raw token needs a mirror field in Tantivy, which means a schema change and a full rebuild on upgrade, for a query that is rare. Until a user reports it, the difference stays.

## Indexing ignores `render.html_command`

The index cleans HTML with the built-in converter even when `render.html_command` is set. Running the external command for every HTML message during a rebuild would start one process per message, so the index always uses the built-in converter. For users who set `html_command`, a word the external renderer lays out differently can read one way in the reader and match another way in search. If that shows up in practice, the fix is to run the command in batches or cache its output per body hash, and to include the setting in the content-version marker.

## Paging across a fallback and index switch

A client that pages through results while the index stops or starts can get a page from SQLite and the next page from the index. Each page is internally consistent, and the degraded notice stays on every page after the switch, so the client can tell. A cursor that pins the source would fix the mixing, but it is a cross-client protocol change for a state that lasts only while the index is stopped or rebuilding, so paging keeps offset semantics.

## Very large or deeply nested documents lose their layout

html2text is not given a document over 2 MiB, nested more than 256 elements deep, with more than 20,000 table cells, a row wider than 64 cells, or more than 200,000 grid slots (colspan times rowspan summed over the cells). Such a document shows its tag-stripped text, so its tables and lists lose their layout. Spans are clamped to 100 before the render. The structure is measured by one pass of html5ever's tokenizer (`crates/reader/src/html_limits.rs`, with a copy in `crates/outbound/src/html_limits.rs`). Ordinary mail is far under every cap; a legitimate very wide statement is the likely casualty.
