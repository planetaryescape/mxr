# Search fallback follow-ups

Two behaviours differ from the lexical index on purpose, and one limitation of paging is kept on record. Each entry says why it stays.

## Search `+c++` matches the word `c`; rules match the literal `c++`

The index cuts query terms on non-alphanumeric characters (`tokenize_text_value` in `crates/search/src/query_builder.rs`), so `+c++` searches the token `c` and finds any mail with the word "c". A rule's `c++` matches the literal text `c++`, as `+c++` in a rule does too (`StringMatch::whole_word`). The two forms disagree for the same typed word. Making search keep the raw token needs a mirror field in Tantivy, which means a schema change and a full rebuild on upgrade, for a query that is rare. Until a user reports it, the difference stays.

## Indexing ignores `render.html_command`

The index cleans HTML with the built-in converter even when `render.html_command` is set. Running the external command for every HTML message during a rebuild would start one process per message, so the index always uses the built-in converter. For users who set `html_command`, a word the external renderer lays out differently can read one way in the reader and match another way in search. If that shows up in practice, the fix is to run the command in batches or cache its output per body hash, and to include the setting in the content-version marker.

## Paging across a fallback and index switch

A client that pages through results while the index stops or starts can get a page from SQLite and the next page from the index. Each page is internally consistent, and the degraded notice stays on every page after the switch, so the client can tell. A cursor that pins the source would fix the mixing, but it is a cross-client protocol change for a state that lasts only while the index is stopped or rebuilding, so paging keeps offset semantics.

## Very wide tables lose their layout

html2text cost grows with table width: it clones the column widths into every row. So mxr flattens a table that has more than 64 columns in a row, or more than 50,000 cells (rows times widest row), into inline text: each row a line break, each cell its text followed by a separator. Documents over 2 MiB skip html2text and show the tag-stripped text. A legitimate wide table, such as a long financial statement, shows its cells in order without the grid. Both limits live in `crates/reader/src/html_limits.rs`, with a copy in `crates/outbound/src/html_limits.rs`.

