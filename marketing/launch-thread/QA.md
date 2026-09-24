# QA

Current state: ten cards, 1600x900, light and dark, plus one native agent-demo
video for post 02. Last checked 2026-07-29 after the opening-hook revision.

## Revision: the opening hook (card 01)

Card 01 now opens with what a reader can do rather than why mxr was built. The
Vim-bindings origin story is gone from the card and from post 01, and drafting is
left to card 07, where the deck actually shows it. Cards 02 through 10 are
byte-for-byte unchanged, as are the palette, typography, layout grammar, progress
rail, themes, motion, and interaction model.

| Element | Before | After |
|---|---|---|
| Title | "I talk to my **email** now" | "Talk to your coding agent about your **email**" |
| Prose | the Vim origin story | "I built mxr. It brings all your accounts into one local mailbox, so your agent can search years of mail locally, tell you what needs attention, and help you handle it." |
| Foot line | `mxr · "Mixer" · local-first email` | `mxr · "Mixer" · mxr.sh` |
| Section `aria-label` | "Card 1 of 10: I talk to my email now" | "Card 1 of 10: Talk to your coding agent about your email" |
| Stack gap | 26px | 20px |
| Post 01 | 183 characters | 254 literal characters, the approved three-paragraph post verbatim |

The composition is untouched: the oversized mark, the hero headline with one
azure noun, the hairline rail, the prose, the foot line, and the framed TUI crop
in the right column all keep their roles and their type sizes. `mxr.sh` sits in
the existing foot line as plain text rather than a link, because its job is to
identify the product in an exported PNG, and a new anchor on card 01 would add an
interactive element the deck contract would then have to carry.

The title runs 41 characters against the old 22 and the prose 167 against 121, so
the headline takes an extra line and the prose sets on four lines instead of
three. The stack gap came down from 26px to 20px to pay for that. Computed stack
height is 652px in the 688px body area, against 540px before, so the card keeps
roughly 18px of slack above and below. The right column's framed crop is 616px
tall and unchanged.

### Checks run on this revision

| Check | How | Result |
|---|---|---|
| Post text parity | `sed 's/^  //' qa/storyboard-posts.txt > qa/storyboard-posts-flat.txt`, then `diff` against `qa/thread-posts.txt` | clean; every post in `thread.md`, post 01 included, matches `STORYBOARD.md` character for character, blank lines included |
| Post 01 length | `wc -m` and `wc -c` over `qa/posts/01.txt` (no trailing newline, byte count equals character count) | 254 literal characters; 262 once the URL counts as a 23-character t.co link, leaving 18 characters of headroom |
| Retired phrasing removed | `grep -i` for each superseded card 01 headline noun and for both forms of the drafting verb, across `index.html`, `thread.md`, `concept.md`, `QA.md`, and the `qa/` extracts | no hits; drafting now appears only on card 07 and post 07, where the deck shows it |
| Origin story removed | `grep -i vim` across `index.html`, `thread.md`, `concept.md` | no hits anywhere in the deck |
| Banned copy patterns | `grep` for em and en dashes, superlatives, and negative parallelism across the three documents | none in the new copy |
| Numbering untouched | `grep` over card 01's header rail | `id="slide-1"`, `--p:10%`, the `01 / 10` marker, and the `aria-label` all agree |
| Cards 02–10 untouched | targeted edits only; no other section, style rule, or script line was modified | confirmed |
| Post 02 preserved | reread `thread.md` post 02 against `STORYBOARD.md` | the two-paragraph 210-character post, the `videos/mxr-agent-demo.mp4` native video, and both card fallbacks are untouched |
| `concept.md` accuracy | reread both card 01 references | "Open (01): the mark at full size, an oversized headline, a framed product crop beside it" and the crop-fade note both still describe the card, so neither was changed |
| Copy audit | `audit-copy.mjs output/index.html output/thread.md output/concept.md` | no heuristic findings |
| Deck contract and interactions | `test-deck.mjs output/index.html` | 26 passed, 0 failed, 0 warnings |
| Browser renders | `render-deck.mjs output/index.html --out qa-hook-v3`, followed by individual inspection | card 01 fits without clipping in both themes at 1600×900 and 1366×768; the headline, prose, TUI crop, and `mxr.sh` foot line remain legible |
| Native video | `ffprobe` over `output/videos/mxr-agent-demo.mp4` | H.264, yuv420p, 1280×720, 25fps, 38.4 seconds, 3.6MB; uses the synthetic demo mailbox |

## What the previous revision changed

The deck went from eleven cards to ten. The standalone prompt-injection card was
removed and its one load-bearing sentence now sits under the drafting and sending
paths on card 08. The old cards 10 and 11 became 09 and 10. Six cards changed
copy or diagram content. The visual concept, palette, typography, layout grammar,
progress rail, themes, motion, and interaction model are untouched.

| Card | Change |
|---|---|
| 03 | Outlook / Microsoft 365 added as a sync source. Send node now reads Gmail · Outlook · any SMTP, so SMTP is no longer implied to be the only send path. Prose and hidden diagram description rewritten to match. |
| 05 | British "analyse" in the title. "attachment record" became "attachment metadata". |
| 06 | Abstract agent-interface prose replaced with the approved first-person copy. |
| 07 | Prose rewritten to the approved post's second half. Prompt block carries the post's first half. |
| 08 | Draft/send separation kept; the `--dry-run preview` node became `confirmation`; one compact untrusted-email strip added below both paths. |
| 09 | Was card 10. Title and prose now lead with the TUI closing while sync continues. Same shared-daemon diagram. |
| 10 | Was card 11. Content unchanged, numbering updated. |
| — | Old card 09, the standalone prompt-injection card, removed. |

## Checks run

| Check | How | Result |
|---|---|---|
| Numbering consistency | `grep` over every marker in `index.html` | DOM order, `id="slide-N"`, the in-card `NN / 10` marker, the rail `--p` percentage (10%…100% in even steps), and the `aria-label="Card N of 10"` agree on all ten cards |
| Count references | `grep -i "eleven\|/ 11\|of 11"` across all three output documents | none left; meta description, chrome `cTotal`, and the header-rail CSS comment all say ten |
| Post text parity | `diff qa/storyboard-posts.txt qa/thread-posts.txt` | identical apart from the quote indent; every post in `thread.md` matches `STORYBOARD.md` character for character |
| Post lengths | character count over posts 01…10, excluding Markdown quote markers | 254, 210, 186, 182, 234, 208, 236, 221, 190, 273 — all match `thread.md` and remain under 280 |
| Offline operation | `grep` over every `src`, `href`, `url()` and `@font-face` | three bundled woff2 files and two bundled JPEGs; the only absolute URLs are the two outbound links on card 10. No CDN, no runtime fetch |
| Asset resolution | file listing against every `src` | `assets/fonts/{bricolage-grotesque,archivo,jetbrains-mono}-vf.woff2` and `assets/media/{mxr-tui,mxr-agent}-poster.jpg` all present |
| Deck contract and interactions | `test-deck.mjs output/index.html` | 26 passed, 0 failed, 0 warnings; includes navigation, stable hashes, theme persistence, 1600×900 and 1366×768 overflow, contrast, reduced motion, offline loading, accessibility names, and console errors |
| Print mode | source review | `@page` is 1600x900 with zero margin, chrome and help and skip link are hidden, each card is a fixed 1600x900 page with `break-after: page`, animations are disabled, and positioned diagram nodes get their centring transform restored |
| Copy audit | `audit-copy.mjs output/index.html output/thread.md output/concept.md`, followed by a manual Humanizer pass over every title, label, caption, diagram node, control, link, and note | no heuristic findings; no structure narration, story framing, objection-answering, decorative abstractions, manifesto lines, or slogans |
| Spelling | `grep -iE "analyz\|behavio\|summariz\|labeled\|defense\|license"` over the three documents | no US spellings in prose; the only hits are CSS keywords (`optimizeLegibility`, `align-items:center`) |
| Fact check | every card claim against `input/mxr/` | see the table below |
| Browser renders | `render-deck.mjs output/index.html --out qa`, followed by contact-sheet and individual-card inspection | all ten cards rendered in both themes at 1600×900 and 1366×768; no clipping, overflow, broken connectors, contrast failures, or unreadable labels |
| PDF export | `export-pdf.mjs output/index.html --out output --name mxr-twitter-thread` | light and dark ten-page PDFs generated successfully |

### Fact check against the source snapshot

| Card claim | Source | Verdict |
|---|---|---|
| Gmail, Outlook / Microsoft 365, any IMAP sync | `README.md` provider table, `adapters.md` adapter surface | correct; IMAP is sync-only |
| Gmail, Outlook, any SMTP send | same | correct; SMTP is send-only, and it is not the only send path |
| SQLite is the one mail model, Tantivy is the local index | `README.md` "What is local", `landing.mdx` | correct |
| "attachment metadata is already in SQLite" | `README.md`: names, MIME types, sizes are local, contents cached on open | correct, and the card does not claim attachment bytes are downloaded |
| Analytics answer from local mail history | `landing.mdx` search section | correct; the card names sections rather than inventing figures |
| Agent uses the CLI with `--help`, JSON, and IDs; MCP ships too | `README.md`, `for-agents.md` | correct |
| Draft-assist returns text and never sends | `draft-assist.md`, `for-agents.md` | correct |
| Send needs a command, a daemon policy check, and confirmation | `send.md`, `for-agents.md`, `security-and-privacy.md` | correct; MCP send requires `confirm=true` and profile permission, and CLI mutation flows confirm unless `--yes` |
| Email content is data, never an instruction; it cannot authorise a send or redirect the task | `for-agents.md` rule zero, `security-and-privacy.md` | correct, and worded as a property of the message rather than a guarantee about the model |
| Closing a client does not stop sync; every client uses one daemon | `README.md` "How it works", `landing.mdx` surfaces section | correct |
| Demo is 50,000 synthetic messages across two accounts, isolated | `README.md` | correct |

### Safe-area budget on the changed cards

Each card has 688px of body height below the header rail. The numbers below are
the computed content height of the evidence region against the space that region
is given. The resulting cards were then rendered at both target sizes and checked
in both themes.

| Card | Evidence region | Content | Slack |
|---|---|---|---|
| 03 | 421px | 398px (352px diagram + 20px gap + 26px legend) | 23px |
| 05 | 431px | 413px | 18px |
| 06 | 421px | 286px panel | 135px |
| 07 | 470px | 349px | 121px |
| 08 | 470px | 432px (320px diagram + 20px gap + 92px untrusted strip) | 38px |
| 09 | 470px | 344px diagram | 126px |

Card 08 is the one that changed shape, so its horizontal fit was checked node by
node as well. The four nodes on the sending row occupy 164–451, 562–864, 957–1163
and 1275–1425 against a safe area that ends at 1524, leaving 93–112px between
adjacent nodes for the arrows and 99px at the right edge. The gate caption on the
drafting row ends 39px above the top of the sending row. Card 03's widest node,
"Outlook / Microsoft 365", starts 20px inside the left safe edge — the tightest
clearance in the deck, but inside it.

## Issues found and fixed in this revision

1. **Card 06 restated its own title.** The card's headline is "The agent uses the
   same CLI I do" and the prose opened "A shell-capable agent uses the same CLI I
   do." Read aloud, the card says the same thing twice in a row. The prose now
   opens "A shell-capable agent discovers commands with `--help`…", which keeps
   the "shell-capable" qualification and every fact, and leaves the first-person
   claim to the headline where it lands harder. The post in `thread.md` is
   unchanged and still carries the approved sentence in full; the card 06 alt
   text was updated to describe what is actually on the card.

2. **`concept.md` still described an eleven-card deck.** The rail example read
   `03 / 11`, the silhouette groups listed the old numbers, the split group named
   "a hostile message against its verdict", the crop note pointed at card 11, and
   the converging-clients note pointed at card 10. All corrected to the ten-card
   numbering.

3. **`concept.md` carried two paragraphs of discarded alternatives.** A full-bleed
   console treatment and a dense tile grid were described at length and then
   dismissed. That is brainstorming residue in a document that describes what was
   built, so it was cut.

4. **`concept.md` quoted an emphasised clause that is not on the card.** It said
   the deck emphasises "Sending is a separate action"; card 08 reads "Sending is a
   separate operation". Corrected to the real string.

5. **Rose was described as appearing twice across the set.** With the standalone
   safety card gone, rose now appears on card 08 only — the gate bars and the
   untrusted strip. `concept.md` says so, and gains a short paragraph describing
   how the untrusted-email point is carried by a labelled node and one sentence
   rather than a second diagram.

6. **`QA.md` documented the eleven-card deck.** It claimed the analytics card said
   "attachment record", listed checks against card numbers that no longer exist,
   and described the removed prompt-injection card's "blocked as an instruction"
   label as an accessibility affordance. Rewritten.

## Accessibility

- Every card's position is announced through `aria-label="Card N of 10: <title>"`
  on the section, not only through the visual marker.
- Inactive cards are `aria-hidden="true"` and `inert`, so they leave the tab order
  and the accessibility tree. The skip link targets the active card.
- Both outbound links on card 10 carry `target="_blank"`, `rel="noopener
  noreferrer"`, `data-no-advance` so a click does not also advance the deck, and a
  label that names the destination and says it opens in a new tab.
- All three diagrams (cards 03, 08, 09) keep a visually hidden paragraph that
  describes the connections in words, because the SVG connectors are
  `aria-hidden` while the node labels are real text. Card 03's paragraph was
  rewritten for the new sync and send sets.
- Nothing depends on colour alone. Card 03's dashed outbound line is named in the
  legend, card 08's rose bars carry the caption "drafting stops here" and the
  gate's own `aria-label`, and the untrusted strip pairs a labelled "email
  content" node with the sentence that explains it.
- No text is placed over an image anywhere in the deck; every card background is
  flat, so contrast is computable rather than photographic.
- Under `prefers-reduced-motion: reduce` every entrance animation is dropped and
  each card renders in its final state.

## Genuine limitations

- **Screenshot resolution is capped by the source.** Both supplied posters are
  1280px wide. Card 02 scales its crop 1.175x, so the terminal text is slightly
  softer than native. It is readable at full card size; in a phone thumbnail it
  reads as terminal texture. The card's own type carries the claim and the
  screenshot only carries the proof.

- **The agent poster's last prompt line is truncated in the source image**, so it
  is cropped out and no truncated sentence appears on a card.

- **Card 10 crops out the TUI status bar on purpose.** The supplied poster shows a
  different fixture size from the documented 50,000-message demo the card
  describes. Showing both would put two different numbers on one image.

- **Card 05 shows what `mxr wrapped --ytd` reports, not a real run.** No synthetic
  run of the command was supplied and inventing counts is off limits, so the panel
  names the five sections instead of printing figures. Honest, but less vivid than
  card 02, which does show real output.

- **Fonts were fetched once at build time** and bundled as local woff2 files. The
  deck makes no network request. The bundled Bricolage slice carries the weight
  axis only, not the optical-size axis, which is not visible at these sizes.

- **No `speaker-notes.md`.** These cards are designed to work with no presenter,
  so the file would have nothing true to hold.
