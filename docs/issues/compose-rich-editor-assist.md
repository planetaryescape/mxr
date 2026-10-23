# The rich-text composer has no selection editing or model marks

**Status:** open · **Found:** cross-model verification of the composer help line (S4 round 1) · **Severity:** should-fix · **Deferred:** yes

## What happens

"Draft for me" is one help line, and its selection and model-mark features work in the Markdown editor only. The rich-text editor (`features/compose/tiptap/TiptapComposeEditor.tsx`, chosen under Settings › Compose or the "Rich text" segment) reports no selection and draws no marks, so:

- A selection made there does not change what the help line sends. Without one, the request is `finish` or `write` over the whole text region.
- Accepted model words carry no dotted hairline. The proof's "added" pieces are not marked in the rich editor.

The help line never offers "Edit the selection" there: `ComposeEditorPanel` clears the reported selection whenever the rich editor is active, so the placeholder falls back to "Finish my draft" or "Draft a reply". The site guide says selection editing works in the Markdown editor.

## What works today

- Text regions, proofs, accept and undo are the same in both editors, because they work on the markdown string the rich editor writes back through `onChange`.
- The rich editor serialises its document with `doc.textBetween(0, size, "\n\n")` and parses with `value.split(/\n{2,}/)` (`textToDoc`). Blank lines are the only paragraph boundary the draft ever sees.

## Proposed approach

1. **Selection out.** A ProseMirror position maps to a text offset with the same serialiser: `doc.textBetween(0, pos, "\n\n").length`. Report `{ from, to }` on `selectionUpdate` when the range is non-empty. Convert to the text region offsets the same way the Markdown editor does (`selectedRegionRange`), since the rich editor's text is the same string.
2. **Marks in.** Model spans are text offsets. Walk the top-level paragraphs in order, tracking the offset each one starts at (its text length plus the two-character separator), and add an inline decoration for each span that falls inside a paragraph. A span that crosses a paragraph boundary is split at it.
3. **Edits.** The span mapping already runs on every body change (`mapModelSpans`), so marks clear on edit with no rich-editor code.
4. **Known loss.** The serialiser drops inline marks and links, and `textToDoc` drops them on the way back in. Accepting a proof therefore rewrites bold and links in the region. That is existing behaviour of the rich editor and needs its own decision before the marks ship.

## Done when

- Selecting a word in the rich editor sends `edit_selection` with that word's UTF-8 range, and "Edit the selection" appears.
- An accepted added run is marked there and loses the mark on edit, as in the Markdown editor.
- A fixture document with two paragraphs and a span across the blank line has a test for each step above.
