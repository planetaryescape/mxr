# Read-side HTML budget gaps, until render isolation ships

The send side refuses an HTML body over the budget in `crates/reader/src/html_limits.rs`. The read side (reader, TUI, search index, links, images) keeps the budget as a guard, not a boundary. The budget is a linear byte scan, and the parsers behind it (html5ever, html2text) can still be driven past it, so a message can still cost the reader more than the budget implies. Process isolation of HTML rendering, in a separate slice, is the fix. Until it ships, these are known to remain:

## Formatting-element reconstruction is not counted

html5ever rebuilds the list of active formatting elements when a block closes. A document with `<p>` followed by about 6,000 distinct `<b id="…">` elements, then about 6,000 `</p><p>x` pairs, makes that rebuild quadratic. The budget counts tags, not how many formatting elements are open at once, so the document passes and the parse is slow.

## Attribute counting is fooled by quoted `>` and adjacent quoted attributes

The scan ends a tag at the first `>` and counts attributes as whitespace- and `/`-separated tokens. A quoted `>` inside an attribute value ends the tag early, and adjacent quoted attributes (`a="1"b="2"`) count as one token. A tag can therefore carry more attributes than the 64-attribute cap allows, while the scan still counts it as within budget.

## Entity-encoded span values

A span attribute whose digits are written with character references, such as `rowspan="6&#48;00"` (decodes to `600`), is not seen as digits by the byte scan. The scan rejects a span with an entity and no leading digits, but not one that mixes digits and entities, so the parser may see a span far above the clamp.

The send side is not exposed to these: it refuses over-budget HTML outright, and the attribute and span gaps above only let a document through the read-side fallback, not past the send-side refusal, which the outbound checks run on the full document.
