---
title: Sound, key hints and touch
description: Turn on sounds, learn keys from hints, and swipe rows on a touch screen.
---

Turn on the optional sounds, pick up keys from the hints the web app shows,
and swipe rows on a touch screen. None of these asks for anything, and none
keeps score.

## Low tide

When you clear the desk, the web app draws a small shore that settles and
goes still, with one line: **Low tide. Nobody's waiting on you.** The next
promise you made follows it, if one is open.

- It shows once per clearing: the first time you see the desk empty after it
  had work in it, whether you cleared it here, in the TUI or from the CLI.
  Coming back to a desk that was already clear shows the plain empty state.
  "Once per clearing" is remembered in this browser only.
- Finishing a focus session with everyone answered shows the same scene
  ("Low tide. That's everyone.").
- Emptying Reading, Paper trail or the reply queue shows a smaller version.
- To make it a still picture, set **Settings > Appearance > Motion** to
  **Reduced**. The default, **Match the system**, follows your system's
  reduced-motion setting.

The TUI has no animation. It says **Low tide. Nobody's waiting on you.** in
the status bar and on the desk when the desk or the owed-replies list
clears, and "Low tide. That's everyone in the reply queue" when a focus run
ends.

## Turn sound on

Sound is off until you turn it on. In the web app, open **Settings > Sound**
and switch **Sound** on; the volume slider is there too. From the command
line:

```bash
mxr chimes status
```

```text
enabled=false
volume=0.35
new_mail=bell
sent=sent
archived=archive
trashed=thud
spam=alert
snoozed=pop
unsnoozed=glass
reminder=bell
error=alert
```

```bash
mxr chimes enable
mxr chimes set archived pop     # pick the sound for one event
mxr chimes test archived        # hear it
```

The web app, TUI and CLI share this one setting, so turning it on in one
place turns it on everywhere. The events and sounds are listed in the
[config reference](/reference/config/).

| Event | Web app | TUI and CLI (played by the daemon) |
|---|---|---|
| Send | `sent` | `sent` |
| Archive, sweep | `archived`, once per action | `archived` |
| Snooze | `snoozed` | `snoozed` |
| Trash, spam, wake a snooze | none | `trashed`, `spam`, `unsnoozed` |
| Cleared desk | two soft notes rising | none |
| New mail, reminders, errors | none | `new_mail`, `reminder`, `error` |

The web app makes its sounds in the browser, with no audio files: under a
quarter of a second, soft, and scaled by the volume. It never plays for
moving around or opening a thread, in a background tab, or for the repeats
of a held key. When the web app acts, the daemon leaves the sound to the
browser, so nothing plays twice. If the browser will not start audio,
nothing plays.

## Learn keys from hints

Every action with a key shows it in tooltips, menus, the command palette and
the keyboard help. If you click the same toolbar or row button three times, a
quiet hint names its key once, for example **Tip: press e for Archive.**

- Each action's hint shows once.
- No hint shows while you type, or within ten seconds of pressing a key.
- At most one hint every five minutes.
- The counts live in this browser.

The full list is in the [keybindings reference](/reference/keybindings/#web-app).

## Swipe rows on a touch screen

On a touch screen, swipe rows in any mail list: the desk, the inbox, folders,
labels, search results, the queues, and Paper trail. Reading has no swipe.

| Swipe | What it does |
|---|---|
| Short, right | Archive. On the desk: Done. On a Paper trail sender: preview a sweep of that sender. |
| Long, right | Move to Trash. Not on Waiting on rows. |
| Left | Snooze, with the time field. |

The colour and words under the row show what will happen from about 40 px,
and nothing happens until you let go. Trash needs the long throw: a fast
flick never trashes. Every swipe can be undone from its toast. The row only
moves once your finger is clearly going sideways, so vertical scrolling works
as usual. Mouse and trackpad users see none of this.
