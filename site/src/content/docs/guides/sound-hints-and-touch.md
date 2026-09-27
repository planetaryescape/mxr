---
title: Sound, key hints and touch
description: The small things in the web app, all optional or quiet: low tide when you clear the desk, a soft sound palette, hints that name a key, and swiping rows on a touch screen.
---

These are the parts of the web app that make it pleasant rather than just
correct. None of them asks for anything, and none keeps score.

## Low tide

When you clear the desk, the web app draws a small shore: the water's edge
draws back, a few tide pools catch the light, and then it is still. One line
says **Low tide. Nobody's waiting on you.**, with the next promise you made
if one is open.

- It shows once per clearing: the first time you see the desk empty after it
  had work in it, whether you cleared it here, in the TUI or from the CLI.
  Coming back to a desk that was already clear shows the plain empty state.
- Finishing a focus session with everyone answered shows the same scene.
- Sweeping Reading or Paper trail clear shows a smaller version.
- With reduced motion (your system setting, or **Settings > Appearance >
  Motion**) it is a still picture.
- "Once per clearing" is remembered in this browser only.

The TUI says the same line, **Low tide. Nobody's waiting on you.**, in the
status bar and on the desk when you clear the desk or the reply queue, and
when you finish focus mode. There is no animation in the terminal.

## Sound

Sound is off unless you turn it on in **Settings > Sound**. It uses the same
setting as the daemon's chimes (`mxr chimes`), so turning it on in one place
turns it on everywhere:

| Event | Web app | TUI and CLI |
|---|---|---|
| Send | the `sent` sound | the `sent` sound |
| Archive, sweep | the `archived` sound, once per action | the `archived` sound |
| Snooze | the `snoozed` sound | the `snoozed` sound |
| Cleared desk | two soft notes rising | none |
| New mail, reminders, errors | none | the daemon plays them |

The web makes its sounds in the browser, with no audio files: short (under a
quarter of a second), soft, and scaled by the volume setting. It never plays
for moving around (`j`, `k`, opening a thread), in a background tab, or for
the repeats of a held key. When the web app acts, the daemon leaves the sound
to the browser, so nothing plays twice. If the browser won't start audio,
nothing plays.

## Key hints

Every action with a key shows it in tooltips, menus, the command palette
and `?`. If you use the same toolbar or row button three times with the
mouse, a quiet hint names its key once: **Tip: press e for Archive.** You
see it once per action, never while you are typing or using keys, and never
more than one hint every five minutes. The counts live in this browser.

## Touch

On a touch screen you can swipe list rows on the desk, in the inbox and in
Paper trail:

| Swipe | What it does |
|---|---|
| Short, right | Archive (on a Waiting on row: done waiting; on a Paper trail sender: preview a sweep of that sender) |
| Long, right | Move to Trash |
| Left | Snooze, with the natural-time field |

The colour and words under the row show what will happen from about 40 px,
and nothing happens until you let go. Trash needs the long throw: a fast
flick never trashes. Every swipe can be undone from its toast, as with the
keys. Vertical scrolling works as usual: the row only moves once your finger
is clearly going sideways. Mouse and trackpad users see none of this.
