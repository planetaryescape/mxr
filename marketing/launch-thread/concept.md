# Concept - Signal path, one card at a time

## The direction

**Name:** Signal path, carded.

These ten images are the mxr meetup deck's visual system rebuilt for a reader
holding a phone. The identity is inherited whole: cool slate neutrals, one azure
signal, emerald for anything already local, rose reserved for stops, the mxr
aperture diamond, Bricolage Grotesque against JetBrains Mono, and diagrams whose
connectors are measured from real element geometry rather than hand-placed.

What changes is who is reading. A talk slide is a prop for a person speaking; a
social card is alone in a feed with no narration and about four seconds of
attention. So the adaptation moves along three axes:

Type gets much bigger. Body copy is 34px on the 1600x900 card, roughly 40% larger
than the talk deck's 24px lead, and titles run at 62px. That is the size that
survives a timeline thumbnail well enough to make someone tap.

Each card carries its own explanation. The talk deck could put one label on a
diagram and let the speaker fill in the rest. Here the storyboard's full sentence
sits on the card, above the graphic, so the image teaches on its own.

The chrome moves inside the card. A conference deck's progress bar and wordmark
are furniture around the slide. Exported PNGs have no furniture, so the position
marker and the product mark became part of the composition.

### The signature: the thread position is the signal path

Every card opens with a horizontal rail. The mxr mark sits at its left end, an
azure segment runs along it to the current position, an aperture diamond marks
where you are, and `03 / 10` closes the right end. It is the deck's signal-path
grammar doing an honest job: the reader's position in the thread is a value
travelling a line. It is the one element on all ten cards, so a reader
who scrolls past three of them recognises the fourth.

### Three silhouettes, one grammar

A single fixed template across ten cards reads as a form letter. The cards use
three compositions borrowed from the talk deck's own template family:

- **Open** (01): the mark at full size, an oversized headline, a framed product
  crop beside it.
- **Band** (02, 03, 04, 05, 06, 08, 09): headline and prose across the top, one
  wide piece of evidence below - the agent's terminal, providers converging, the
  latency track, the analytics specimen, the CLI readout, drafting against
  sending, clients on a bus.
- **Split** (07, 10): headline and prose across the top, then two columns - a
  prompt against its weighting, or the install commands against a product crop.

Every card ends up with the same skeleton - rail, headline, prose, evidence - so
the set is coherent, while the evidence region changes shape enough that ten in
a row do not read as a template.

## Palette

Unchanged from the approved deck, values included:

**Dark** (the canonical launch images)
- bg `oklch(0.185 0.013 255)` · surface `oklch(0.233 0.014 255)` · raised `oklch(0.284 0.016 256)`
- hairline `oklch(0.400 0.018 256)` · soft `oklch(0.320 0.015 256)`
- ink `oklch(0.960 0.006 250)` · ink-2 `oklch(0.845 0.010 250)` · ink-3 `oklch(0.730 0.014 252)`
- azure `oklch(0.685 0.155 250)` · emerald `oklch(0.800 0.135 168)` · rose `oklch(0.700 0.195 14)`

**Light** (the alternate set)
- bg `oklch(0.975 0.004 250)` · surface `oklch(0.999 0.001 250)` · raised `oklch(0.945 0.007 252)`
- hairline `oklch(0.862 0.010 252)` · soft `oklch(0.910 0.007 252)`
- ink `oklch(0.245 0.013 260)` · ink-2 `oklch(0.400 0.015 260)` · ink-3 `oklch(0.448 0.016 260)`
- azure `oklch(0.515 0.175 253)` · emerald `oklch(0.520 0.130 168)` · rose `oklch(0.530 0.205 18)`

Colour keeps its meaning from the talk deck, so the diagrams still read without a
key: azure is the request in flight, emerald is anything already on the machine,
rose is a stop. Rose appears on one card out of ten, the safety card, which is
what keeps it meaning stop. Every coloured state also carries a written label, so
nothing depends on colour alone.

The real mxr orange lives in `--logo` and appears only on the product mark in the
card header and on the card 01 hero mark. No diagram, tag, node, or code
highlight can inherit it.

Card backgrounds are flat. The talk deck's faint radial wash stays on the screen
viewport behind the stage and never enters an exported card, because a flat field
reproduces more reliably through social image compression and keeps the contrast
measurable.

## Typography

- **Bricolage Grotesque** (variable, 200-800) for headlines and prose. Titles at
  62px/800, tracking -0.024em, `text-wrap: balance`. Prose at 34px/1.44 capped at
  1180px, which is roughly 68 characters.
- **JetBrains Mono** (variable) for anything a machine printed or a person typed:
  commands, node labels, tags, the position marker, the prompt on card 07.
- Archivo is bundled only as a fallback.

All three are local woff2 files under `assets/fonts/`. The cards load nothing over
the network.

Emphasis is rationed. Two cards carry one emphasised clause each - "The only
thing I wait for is the model" and "Sending is a separate operation" - because
those are the claims a reader should leave with. Nothing else is bolded.

## Graphic and diagram language

The rail is a 2px line carrying a signal, azure where the request is live. A node
is a rounded surface box with a mono label, ringed emerald when the thing is
local, azure when it is the request, and rose when it is a stop. The aperture -
the mark's diamond - is the focus glyph, used on every local node and on the two
inputs that weigh most on card 07. Instrument readouts are framed panels with a
mono status strip, used for the year-in-review command, the CLI sequence, and the
demo install; they stay restrained rather than becoming full terminal chrome.

Card 08 carries the deck's only rose. The drafting path ends at three rose bars
captioned "drafting stops here", and one strip below the two paths says what
email content is: data, never an instruction, unable to authorise a send or
redirect the task. That strip is a labelled node and a sentence rather than a
second diagram, so the card reads as one claim with one qualification instead of
a policy wall.

Screenshots are cropped into those frames as evidence, at a size where the
command and its output can be read, never used as wallpaper. Card 01 and card 10
fade the bottom of the crop so a half-row does not read as a clipping bug.

Card 05 introduces one new shape inside the same grammar: a broad command with
its report sections as emerald tags, then the narrower commands ruled off in two
columns rather than boxed. Emerald is correct there because every one of those
sections is computed from data already on the machine.

### Measured connectors

Connector geometry is computed on every render and resize, never authored by
hand. Each labelled element carries `data-node`; the deck reads its offset
geometry, which is the node's final centre in the diagram's own coordinate space,
writes the SVG `viewBox` to that space 1:1, then places each endpoint on a named
box edge with control points along the exit direction. Lines leave and enter
perpendicular to the edge they touch, at both target sizes, in both themes, and
in print. Offset geometry is used rather than `getBoundingClientRect` because it
is immune to the entrance animation's transform and to stage scaling.

On card 09 the six clients converge on one junction dot and a single arrow
continues into the daemon, rather than six arrows crossing into one box. Arrow
markers are generated per diagram, so a card's arrowheads never resolve to a
marker inside a hidden card.

## Motion

Motion belongs to the HTML preview, not the exported images. Nodes and text rise
once in a short stagger when a card becomes active; nothing loops, nothing
travels, nothing autoplays. Under `prefers-reduced-motion: reduce` every element
is drawn in its final state immediately. Print mode disables animation entirely
and restores each positioned node's centring transform, so an exported page is
identical to a settled screen.

## Why this fits

The readers are developers who will decide in one scroll whether mxr is worth a
`brew install`. They read a dataflow faster than a paragraph, they distrust
marketing surfaces, and they are looking at a phone. Large type, one idea per
card, real terminal evidence at a legible crop, and diagrams that show the claim
rather than assert it are what that reader responds to. Keeping the meetup deck's
palette, mark, and diagram grammar means the thread, the talk, and the product
site all look like the same piece of software.
