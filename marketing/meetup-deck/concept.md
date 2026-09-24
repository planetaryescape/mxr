# Concept - Signal path

## The direction and how it fits the subject

**Name:** Signal path.

mxr is pronounced "Mixer," and the name is the design. A mixing console does one
job: it routes many inputs through one board and sends them out to many
destinations, and you can see the signal the whole way. That is also the
architecture of the talk. Many mail providers (Gmail API, IMAP, SMTP) route
through one local daemon into one mail model, and that model feeds many clients
(TUI, CLI, web, scripts, the agent skill, MCP). And it is the shape of the
thesis: a plain-language question travels along a path, through local retrieval
and then the model, and lands as an answer or a draft.

So the whole deck is built on one visual through-line: a **signal path** - a thin
horizontal rail with labelled nodes on it. On the timing slide it is a latency
track. On the architecture slides it is the routing bus. On the safety slides it
is a line with a gate the request has to pass. The audience learns one grammar
on slide 2 and reads every diagram after that without a legend.

This keeps the recorded terminal where it belongs: as one instrument on the
board (the evidence on slide 2 and slide 11), not as the visual language of every
slide. The deck is an instrument panel, not a terminal.

Directions considered and dropped: a physical-mailroom metaphor (fell into the
warm-paper cliché and fought the engineering subject) and a rigid two-band
"human surface over machine" split on every slide (forced terminal chrome
everywhere and went monotonous over eighteen slides).

## Palette - cool instrument

The deck is not built from the product's orange. The mark stays orange because it
is the real logo; everything else is a cool, instrument-grade system so the deck
reads as local software rather than a vendor colour.

Three directions were tried. A monochrome slate with a single cyan signal was
legible but flat over eighteen slides and gave danger nowhere to live. An
ink-and-amber pairing kept drifting back to the warm register the brief rejects.
The chosen direction is **cool slate with an azure signal**: a blue-grey neutral
ramp, one azure accent for the signal path, emerald for anything local and ready,
and rose reserved for stop states. Azure sits at hue ~250 with a slate neutral at
the same hue family, so the deck reads as one material rather than grey plus a
sticker. There is no purple-to-blue gradient anywhere; the accent is a single flat
hue, and the only gradients are the two-stop progress rail and the profile meters.

Colour is functional. Each hue means one thing everywhere, so diagrams read
without a key:

- **Azure** - the signal path: the request, the agent, the daemon, the focused thing.
- **Emerald** - local and already on the machine: sync, SQLite, allowed states.
- **Rose** - untrusted input and hard stops. Rare, so it always means stop.

Danger is a different hue from the accent, never a darker azure. Meaning never
rests on colour alone: every local / allowed / blocked state also carries a text
label or a glyph.

### Dark theme (room and projector default)
Blue-grey near-black, not a warm one.
- bg `oklch(0.185 0.013 255)` · surface `oklch(0.233 0.014 255)` · raised `oklch(0.284 0.016 256)`
- hairline `oklch(0.400 0.018 256)` · soft `oklch(0.320 0.015 256)`
- ink `oklch(0.960 0.006 250)` · ink-2 `oklch(0.845 0.010 250)` · ink-3 `oklch(0.730 0.014 252)`
- azure `oklch(0.685 0.155 250)` · emerald `oklch(0.800 0.135 168)` · rose `oklch(0.700 0.195 14)`

### Light theme (laptop, print, video call)
A cool paper-white at near-zero chroma, tinted toward the same blue family.
- bg `oklch(0.975 0.004 250)` · surface `oklch(0.999 0.001 250)` · raised `oklch(0.945 0.007 252)`
- hairline `oklch(0.862 0.010 252)` · soft `oklch(0.910 0.007 252)`
- ink `oklch(0.245 0.013 260)` · ink-2 `oklch(0.400 0.015 260)` · ink-3 `oklch(0.448 0.016 260)`
- azure `oklch(0.515 0.175 253)` · emerald `oklch(0.520 0.130 168)` · rose `oklch(0.530 0.205 18)`
- Small coloured text uses the darker `-ink` variants so body copy clears WCAG AA
  in both themes; the computed-contrast check passes on every slide.

### The logo
`--logo` holds the real mxr orange (`oklch(0.705 0.170 47)` dark,
`oklch(0.600 0.180 45)` light) and is used only on the product mark on the title
and closing slides. It is deliberately excluded from the token system, so no
diagram, control, tag, or code highlight inherits it.

## Typography

Two families, a clear contrast axis: a proportional grotesque with character, and
a monospace for anything a machine prints.

- **Display and body - Bricolage Grotesque** (variable). Mechanical but warm,
  slightly irregular; it has a point of view without shouting. Weights 400–800.
  Local: `assets/fonts/bricolage-grotesque-vf.woff2`.
- **Data, commands, labels - JetBrains Mono** (variable). This is the product's
  own vernacular (its CLI, TUI, and site all speak mono), so it is earned here,
  used only where a machine would print. Local: `assets/fonts/jetbrains-mono-vf.woff2`.
- Archivo (variable) is bundled as a neutral fallback only.

All fonts are bundled woff2; the deck loads no font over the network.

## Layout grammar and slide silhouettes

A small set of templates keeps the deck coherent without repeating one layout:

- **Hero** (1, 18): oversized headline, the mxr mark, a single rail.
- **Media** (2): the recording fills an instrument frame; caption below; custom transport controls.
- **Map** (3): the request on the left, the command sequence on a rail to the right, phrases wired to commands.
- **Flow** (4, 5, 6, 7, 9, 10, 12, 13, 16): the signal-path diagrams - providers converging, sync continuing without a client, the timing track, clients on a bus, context by priority, the stop before send, policy gates, the cost cross-section.
- **Spec** (8, 11, 14): one instrument readout (mono) with annotations wired to real flags, a real relationship profile, and the hostile-email data container.
- **Checklist** (15, 17): the fork comparison and the build list, each line tied to a component already shown.

Every slide sits on a fixed 1600x900 stage that scales uniformly and letterboxes;
nothing scrolls in presentation mode. Slide padding reserves 116px at the foot so
no caption crowds the progress chrome; a script checks that every text box ends
above y=785 at both target sizes.

## Graphic and diagram language

- **The rail:** a 2px line carrying a signal. An emerald segment = local and
  already on the machine; an azure segment = the request in flight.
- **Nodes:** a small ring or diamond (an echo of the mxr aperture) plus a mono
  label. Local nodes ring emerald; the agent node is azure; a stop is rose. No
  boxes-and-arrows card grids.
- **Instrument readouts:** a restrained framed panel with a mono status strip for
  commands, JSON, the profile, and the hostile email. Not full terminal chrome.
- **The aperture:** the mark's diamond used as a "focus" - the current thread and
  the user's instruction sit in the aperture; older context sits outside it.
- Diagrams are inline SVG so they stay crisp at projector scale and in print, and
  their text uses `fill: currentColor` so it themes cleanly.

### Measured connectors

Connector geometry is computed, never hand-placed. Each diagram is a
`.diagram[data-diagram]` box; each labelled element carries `data-node`. On every
render and resize the deck reads each node's **offset geometry** (`offsetLeft` /
`offsetTop` / `offsetWidth` / `offsetHeight`), which is the node's final centre in
the diagram's own coordinate space, and writes the SVG `viewBox` to match that
space 1:1. Endpoints are then placed on the named box edge (`left` / `right` /
`top` / `bottom`), and curves get control points along the exit direction so a
line always leaves and enters perpendicular to the edge it touches.

Offset geometry is used rather than `getBoundingClientRect` on purpose: it is
immune to the entrance animation's transform and to stage scaling, so the same
coordinates hold at 1600x900, 1366x768, in both themes, and in print. Positioned
nodes animate with a dedicated `riseLay` keyframe that preserves their centring
transform, and print mode restores that transform explicitly.

Converging lines meet at a deliberate junction dot rather than crossing into a
box: on slide 7 the six clients meet one bus point, and a single arrow continues
into the daemon. Arrowheads are one shared marker set at one size per colour, so
weights match across every diagram. A script verifies all 72 connector endpoints
against their source and target boxes at both sizes and both themes.

## Signature element

**The aperture on a rail.** The mxr diamond aperture is the focus point that
travels the signal path from a plain question to an answer, and the same rail
becomes the routing bus in every architecture diagram. One idea - *email is
something I ask, carried along a visible local path* - rendered the same way from
the title to the close.

## Motion and reduced motion

Motion shows state and structure, never decoration. The active slide's signal
segment draws in once; nodes settle in a short stagger without losing their
centring; the video plays only on click. The progress rail advances with the deck. Nothing loops.

Under `prefers-reduced-motion: reduce`, every segment and node is drawn in its
final state immediately, there is no travel, and the video never autoplays. The
active slide is always fully visible regardless of animation state.

## Why this suits the audience and the content

These are developers who build CLIs, daemons, local databases, and agents. They
read a clean dataflow faster than prose, and they distrust sales decks. The
signal-path grammar lets each architectural claim be *shown* as a path rather
than asserted, keeps the recorded evidence honest and central, and gives the
whole talk one quiet, technical identity that is legible from the back of a room
and over a video call.
