# QA — I talk to my email now

Deck: `output/index.html` (18 slides, self-contained, offline). Concept: Signal path.

## Checks run

| Check | Command | Result |
|---|---|---|
| Copy audit | `audit-copy.mjs output/index.html output/speaker-notes.md` | Pass — no heuristic findings |
| Functional / contract | `test-deck.mjs output/index.html` | 26 passed, 0 failed, 0 warnings |
| Visual render | `render-deck.mjs output/index.html --out output/qa --sizes 1600x900,1366x768` | Pass — 72 screenshots, 4 contact sheets, no console errors |
| Print mode | Playwright: `?print=1&theme=light`, `emulateMedia('print')` | All 18 slides render stacked, `data-theme=light`, `.print` active, 0 console errors |
| Demo video interaction | Playwright drive of slide 2 | Plays on click, pause toggles, leaving the slide pauses + resets to 0:00 and restores the overlay |

`test-deck.mjs` covers: `window.__deck` v1 API, slide/theme state, keyboard nav (arrows, space, Home/End), stage-click advance, hash persistence across reload, inactive-slide `inert`/`aria-hidden`, control accessible names, page overflow at both target sizes, computed text contrast (WCAG AA) in both themes on every slide, external-link safety, reduced-motion (active slide visible, no autoplay), and offline load (no HTTP requests, no console errors).

Dependencies were already installed (`playwright-core` present under the skill's `scripts/`); Chrome found at the standard macOS path.

## Viewport and theme matrix

Rendered and reviewed at 1600×900 and 1366×768, in both light and dark. The stage scales uniformly and letterboxes; no cropping or internal scrolling at either size.

## Issues found and fixed

1. **Slide 9 — class-name collision.** The context-budget `.help` block also matched the keyboard-overlay `.help` rule (`position:absolute; inset:0`) and stretched over the title. Renamed to `.helpblk`.
2. **Slides 3, 8, 10, 11 — dead selectors.** Grid rules written as `#sN .class` never matched because the class sat on the same element as the id; the layouts silently collapsed to a single stacked column. Changed to compound `#sN.class` selectors. Slide 1/18 hero/close and slide 2 body alignment had the same pattern and were fixed too.
3. **Slides 4, 7, 13 — diagram connectors didn't meet their nodes.** The SVG used a `meet`-scaled viewBox while nodes were positioned by percentage, so lines and arrowheads floated. Rebuilt on a fixed-aspect box with a `preserveAspectRatio="none"` viewBox whose fractions match the node percentages, plus `non-scaling-stroke`. Verified endpoints against measured node fractions.
4. **Slide 2 — poster rendered as a near-black frame.** The webm fully preloaded, so Chromium painted the recording's empty first frame instead of the poster. Set `preload="none"` and constrained the frame to a centered 16:9; the rich poster now shows, and the recording still plays on click.
5. **Slide 8 — terminal specimen collapsed to prose.** `.readout__body` lacked `white-space`, so the command lines wrapped together. Added `pre-wrap`.
6. **Slide 13 — safety tiers mislabelled.** The four `read-only / restricted / draft-only / full` levels were spread under four different gates, implying one tier per gate. Moved them to a single caption under the `safety policy` gate.
7. **Em dashes.** Replaced every em dash with a spaced hyphen across `index.html`, `speaker-notes.md`, and `concept.md` (voice standard). Also reworded one self-narration line in the notes.
8. **Light-theme accent contrast.** Darkened the light-theme text shades of orange/teal/coral so small coloured labels clear WCAG AA on white surfaces.
9. **Cleanup.** Removed the leaked global `.bar` rule (was adding padding to the timing track), and dropped two unreferenced media assets (`mxr-mail.svg`, `mxr-og.png`) since the mark is inlined as SVG.

All checks were re-run after fixes and pass.

## Slides inspected at full size

Title (1), demo video (2), request→command map (3), storage flow (4), timing track (6), clients bus (7), CLI spec (8), context budget (9), drafting priority (10), relationship profile (11), draft-to-send gate (12), daemon policy gates (13), prompt-injection boundary (14), and closing (18) — in dark; plus the enforcement diagram (13) in light and full contact sheets in both themes at both sizes. Remaining slides (5, 15, 16, 17) reviewed on the contact sheets.

## Demo verification

`mxr-agent.webm` (1280×720, ~38s) with `mxr-agent-poster.jpg` is bundled locally. It plays, pauses, and replays from custom controls that do not advance the deck; leaving slide 2 pauses and resets it to the start. The recording is the synthetic `demo.mxr.local` mailbox — no personal mail. Reduced motion prevents autoplay.

## Themes and accessibility

Both themes are designed independently rather than inverted. The theme toggle stays in the control bar during presentation and via the `t` key. Keyboard focus is visible, controls have accessible names, inactive slides are `inert`/`aria-hidden`, and colour meaning is always backed by a text label or glyph. (The palette described here was replaced in Revision 3 below; see that section and `concept.md` for the shipped colours.)

## Genuine limitations

- **PDFs omit video playback.** The embedded demo appears as its poster frame in the PDF handouts. The recording remains available in the HTML deck.
- **Fullscreen and projector legibility.** Headless testing only confirms the fullscreen call does not throw. Real fullscreen, projector, and video-call legibility should be checked live before the talk.
- **Contrast over non-solid backgrounds.** The automated checker skips text over gradients/images (the title hero wash and the striped "context left free" block). Those were reviewed visually and read clearly in both themes, but they are not machine-measured.
- **Palette provenance.** The Impeccable palette generator was not run; the palette was composed by hand in OKLCH. Superseded by Revision 3, which replaced the palette entirely.

---

## Revision 1 - direct, spoken copy

Copy-only pass. Design concept, layout system, visual hierarchy, interactions, media, diagrams, and slide order are unchanged. No CSS, JS, colour, or structural edits (except a one-character em dash in a CSS comment).

### Public slide copy changed

- **Slide 6** - final line now "In my day-to-day use, model inference is the wait I notice." (dropped "- not the search").
- **Slide 7** - bottom line now states the client list and bridge directly; removed "did not need a second email implementation / became another client".
- **Slide 10** - "Grounded in messages I have already sent to this recipient." (kept the concrete "do not invent familiarity" model instruction).
- **Slide 11** - lead "Tone becomes a set of ordinary signals you can inspect."; support "These signals ground the draft and help catch obvious drift."
- **Slide 15** - "Use the CLI when the agent already has a shell. Use MCP when the client wants typed tool discovery." (dropped "Neither surface is universally better").
- **Slide 17** - "This fits apps where agents repeatedly access user-owned data and can take meaningful actions."

### Whole-deck copy audit (same classes)

Audited every public line and every note for ghost defenses, meta-narration, manifesto language, decorative abstraction, and forced negative parallelism. Two further instances found and fixed beyond the listed slides:

- **Slide 3 (public)** - "The output here is demo mail." (was "...demo mail, not real mail"; kept the demo-data fact, dropped the negative parallelism).
- **Slide 14 (public)** - "It cannot expand permissions or redirect the task." (was "...reach the path that decides what mxr is allowed to do"; replaced the abstract phrasing with the concrete security rule).

Genuine safety contrasts that teach a concrete rule were kept as instructed: slide 12 "Sending as me is a separate decision", slide 14 "quoted or summarized, but it can't expand permissions", slide 13 gate policy, slide 16 costs.

### Speaker notes changed

Slides 1, 3, 6, 7, 9, 10, 11, 13, 15, 17, 18 - removed the ghost defenses and talk-narration listed in the feedback (and one redundant "the point here is the shape of the loop" on slide 3), while preserving every technical fact and the per-slide timings. Notes 2, 4, 5, 8, 12, 14, 16 were audited and left unchanged; their negations are concrete facts or security contrasts, not ghost defenses.

### Checks re-run after the edits

| Check | Result |
|---|---|
| `audit-copy.mjs output/index.html output/speaker-notes.md` | Pass - no heuristic findings |
| Em dash scan (both files) | 0 in audience/presenter copy |
| `test-deck.mjs` | 26 passed, 0 failed - no overflow at 1600×900 or 1366×768, WCAG AA contrast holds in both themes, no console/page errors, offline and reduced-motion clean |
| `render-deck.mjs --sizes 1600x900,1366x768` | Re-rendered all 18 slides in both themes and both sizes, no console errors |
| Full-size re-inspection | Slides 6, 7, 11, 15 (dark) and the full dark contact sheet - changed copy fits with no wrapping or overflow; layout and diagrams unchanged |

No regressions. The visual deck is identical apart from the revised text.

---

## Revision 2 - blank PDF page 1

CSS-only fix. No copy, layout, colour, JS, or structural changes.

### Defect

Independent PDF QA found page 1 blank in both exported PDFs; the browser deck and all later PDF pages were correct.

Root cause: slide 1 is the only slide with `data-active="true"`, so its entrance elements match `.slide[data-active="true"] .anim { opacity: 0; animation: rise ... forwards }`. In print mode the existing rule `html.print .slide[data-active] * { animation: none !important }` stopped the animation but never restored opacity, so slide 1's `.anim` elements stayed at `opacity: 0` and page 1 rendered blank. Inactive slides never match the `opacity: 0` rule, which is why only page 1 was affected.

### Fix

Added one print rule (index.html, in the print block):

```css
html.print .slide .anim{animation:none!important;opacity:1!important;transform:none!important}
```

This forces every `.anim` element visible and untransformed in print mode, on active and inactive slides alike. Nothing else changed; screen animation, reduced-motion, and both themes are untouched.

### Verification

| Check | Result |
|---|---|
| Print-mode DOM (`?print=1`) | Slide 1's 5 `.anim` elements now compute `opacity:1`, `transform:none` |
| `test-deck.mjs` | 26 passed, 0 failed - screen deck unaffected |
| `export-pdf.mjs` | Re-exported `presentation-{light,dark}.pdf` and the delivery-named `mxr-agent-meetup-{light,dark}.pdf`, all from the fixed HTML, no console errors |
| `pdfinfo` | Each PDF: 18 pages, page size 1200 × 675.12 pts (16:9) |
| `pdftoppm` page 1 (light and dark, both name sets) | Full title slide renders - mark, headline, rail, subtitle, byline. No longer blank |
| `pdftoppm` middle (page 9) and final (page 18), light | Render correctly - no regression |
| `pdftotext` pages 1 and 18 | Text is selectable and extracts cleanly |

Both PDF name sets are byte-identical per theme (same fixed export). PDF page renders for review are in `output/qa/pdf/`.

---

## Revision 3 - connectors, spoken copy, and palette

Completion of the connectors / copy / palette brief. An earlier pass on this revision was interrupted part-way through `index.html`; the surviving work was verified against the live files, repaired where incomplete, and finished. The concept, slide order, layout grammar, typography, interactions, and demo media are unchanged.

### 1. Connector geometry rebuilt (measured, not hand-placed)

The old diagrams positioned nodes by percentage but drew connectors against a fixed `300x100` viewBox, so lines stopped short of boxes and arrowheads floated in gaps.

Connectors are now computed from the DOM. Each diagram is `.diagram[data-diagram]`; each labelled element carries `data-node`. On every render and resize the deck reads each node's **offset geometry** and writes the SVG `viewBox` to match the diagram's own coordinate space 1:1, then places endpoints on the named box edge with control points along the exit direction.

Two defects were found and fixed while doing this:

- **`.lay` centring was destroyed by the entrance animation.** `.lay` centres with `transform:translate(-50%,-50%)`, but the `rise` keyframe ends at `transform:none`, so every positioned node snapped to a corner offset once its animation finished. Added a `riseLay` keyframe that animates opacity and the vertical offset while preserving the centring transform, and an explicit print-mode rule that restores it.
- **`getBoundingClientRect` measured mid-animation and mid-scale.** Replaced with `offsetLeft/offsetTop/offsetWidth/offsetHeight`, which is the node's final centre in diagram space and is immune to entrance transforms and stage scaling.

Per-slide results:

- **Slide 4** - Gmail API and IMAP now run into `provider adapters`, adapters to SQLite with the arrowhead on SQLite's left edge, and adapters down to SMTP (dashed) with the arrowhead on SMTP's top edge. The old inbound arrow stopped short of SQLite and the outbound curve appeared to start near SQLite; both are gone.
- **Slide 7** - the six clients converge on one junction dot, then a single arrow enters the daemon's left edge, and the daemon connects to `mail operations`. The junction is a deliberate bus, not a coincidence.
- **Slide 13** - the rail starts at the right edge of `agent asks` and ends at the left edge of `providers`. Each gate is drawn from its own label's bottom edge down through the rail, so every label sits on the gate it names.
- **Slide 12** - rebuilt as a measured diagram: the three draft outputs feed `review`, then `reply --dry-run`, then a rose stop before `send`.
- **Slides 3 and 10** - audited. Slide 3 uses step markers (a list rail, not box connectors) and slide 10 uses a single centred flow arrow between two grid columns; both are centred on their source and target and needed no geometry change.

Arrowheads are one shared marker set, one size per colour, so weights match across all diagrams.

### 2. Safe bottom margin

Slide padding now reserves 116px at the foot (was 78px), diagram height dropped 470 -> 420px, and the redundant slide-3 caption was removed rather than squeezed. Slide 2's transport controls, and the captions on 7, 12, 13 and 17, now sit clear of the progress chrome.

### 3. Public copy rewritten in spoken English

Applied the brief's wording on slides 2 (title + compact `Synthetic mailbox · read-only`), 3 (caption removed), 4 (title + short legend), 5 (title + paragraph), 6 (caption), 7 (bottom line), 8 (title + all four annotations, with the one-word labels dropped), 9 (paragraph, `--help` as code), 10 (draft line + support line), 11 (lead + support line), 12 (bottom line, commands as code), 14 (paragraph + "Only these can authorize an action:"), 15 (bottom line), 17 (title, item 2, bottom line).

Whole-deck audit for the same class beyond the quoted examples:

- `warm` removed everywhere, including the notes and `concept.md`.
- `ground` / `grounding` / `drift` removed from slide 11 and its note.
- `model inference is the wait I notice` removed from slide and note; `model inference` survives only as a diagram label on slide 6, as the brief allows.
- Editorial kickers renamed: `the surface` -> `the cli`, `tone, concretely` -> `tone`, `generate ≠ send` -> `before send`, `trust boundary` -> `untrusted input`, `your entry point` -> `cli or mcp`, and slide 9 -> `the skill` so it no longer collides with slide 15.
- The `predictable` status badge on slide 8's terminal was dropped; it was an adjective sitting where a machine status belongs.
- Notes were re-read aloud and re-pointed at the new titles; slides 5, 6, 8, 9, 11, 15 and 17 were rewritten to match the slide copy.

Genuine security constraints were left intact: the sender-reputation rule, `confirm=true` not bypassing a profile, `draft-assist` never sending, and the untrusted-mail container.

### 4. Palette replaced

The orange-led palette is gone. The deck is now **cool slate with an azure signal**: a blue-grey neutral ramp at hue ~250-260, azure for the signal path, emerald for local/ready state, and rose reserved for stop states, so danger never shares a hue with the accent. There is no purple-to-blue gradient; the accent is one flat hue.

The real mxr orange is preserved as a dedicated `--logo` token used **only** on the product mark on slides 1 and 18. It is excluded from the token system, so no diagram, control, tag, code highlight, progress rail, or focus ring inherits it. A grep confirms no warm hue appears anywhere outside `--logo`.

Both themes were rebalanced, not inverted: dark is a blue-grey near-black, light is a cool paper-white tinted toward the same family. `concept.md` records the three directions explored and why this one was chosen.

### 5. Verification

| Check | Result |
|---|---|
| `audit-copy.mjs` on HTML + notes | Pass - no heuristic findings; 0 em dashes in either file |
| `test-deck.mjs` | 26 passed, 0 failed - contract, nav, hash, a11y, no overflow at either size, **WCAG AA computed contrast passes on every slide in both themes with the new palette**, offline, reduced-motion, no console errors |
| `render-deck.mjs` 1600x900 + 1366x768, both themes | 72 screenshots + 4 contact sheets, no console errors |
| **Connector endpoints** (`output/qa/scripts/verify-connector-endpoints.mjs`) | **72 connectors, 0 failures** at 1.5px tolerance, across both sizes and both themes. Every start and end lands on its source/target box edge, or on the declared slide-7 junction |
| **Bottom margin** (`output/qa/scripts/verify-bottom-margin.mjs`) | All text on all 18 slides ends above y=785 in 1600x900 stage space, checked separately at 1600x900 and 1366x768 |
| Print mode | Slide 1's 5 `.anim` elements still compute `opacity:1` (the blank-page-1 fix holds); all 27 positioned nodes keep their centring transform; all 4 connector groups render; 0 errors in both themes |
| `export-pdf.mjs` | Both name sets re-exported from the final HTML. Each PDF: 18 pages, 1200 x 675.12 pts (16:9) |
| PDF pages inspected | 1 (first), 2 (media), 4 / 7 / 13 (connector-heavy), 8 (densest), 18 (last), in both themes. Connectors meet their boxes in print; text extracts cleanly with `pdftotext` |
| Contact sheets | Both themes at both sizes reviewed |
| Full-size slides inspected | 2, 4, 6, 7, 8, 10, 12, 13, 17 plus 1 and 18; slide 10 and 13 checked in light as well as dark |

### Limitations

- **The recorded terminal truncates its own last line.** On the slide 2 poster the agent's prompt ends `...concise, do no…` because the recording's terminal wrapped it. That is real product footage, not deck-owned text, so it was left as recorded rather than falsified. All deck-owned text on that slide is complete.
- **Fullscreen and projector legibility** are still only verified headlessly; check on the real projector before the talk.
- **Contrast over non-solid backgrounds** (the title wash and the striped "context left free" block) is not machine-measured; both were reviewed by eye in each theme.
- Two PDF name sets remain in `output/` (`presentation-*` and `mxr-agent-meetup-*`), byte-identical per theme, both exported from the final HTML.
