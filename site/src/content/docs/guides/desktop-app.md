---
title: Desktop app
description: A macOS window around the same local web app, with a global new-message shortcut. The daemon is still the system.
---

The mxr desktop app is a window around the local web app. It opens your mail
in its own window, a system-wide shortcut starts a new message, and when mxr
isn't running it says so instead of showing a broken page.

It is the same app `mxr web` opens in a browser. The daemon is still the
system: SQLite, search, provider sync, credentials and background work all
stay in `mxr`. The desktop app bundles none of that.

## What you need

- macOS on Apple silicon. Linux and Windows aren't built yet.
- `mxr` installed (`brew install planetaryescape/mxr/mxr`). The app looks for
  it on your `PATH`, then in `/opt/homebrew/bin`, `/usr/local/bin`,
  `~/.cargo/bin` and `~/.local/bin`, because apps opened from Finder don't
  get your shell's `PATH`.

The app has no installer yet. Build it from a checkout with
`cd apps/desktop && npm ci --ignore-scripts && npx tauri build --bundles app`;
the bundle lands in
`apps/desktop/src-tauri/target/release/bundle/macos/mxr.app`. The build is
unsigned.
Signing, notarisation and a Homebrew cask come with the first release.

## What it does

On launch the app runs `mxr web --print-url --no-open`, which starts or
reuses the local web bridge, and loads the address it prints. Closing the
window hides it; the Dock icon or the shortcut brings it back, and
Command+Q quits.

Only one copy runs. Opening a second one focuses the first.

The window remembers its size and position. It has the standard macOS menu,
so copy, paste and the other text shortcuts work.

## New message from anywhere

Press Control+Option+Command+M in any app. mxr comes forward and opens a new
message. The shortcut is deliberately not Command+Shift+M, which Mail and
Chrome already use.

To pick another key, set `MXR_DESKTOP_SHORTCUT` before launching, for
example `MXR_DESKTOP_SHORTCUT=super+shift+m`. If the combination is taken by
another app, the window still works without it.

## When mxr isn't running

If `mxr` can't be found, or `mxr web` fails or doesn't answer within 15
seconds, the window shows "mxr isn't running" with the command to start the
daemon:

```bash
mxr daemon
```

Run it in a terminal and press **Try again**. Once the web app is open, its
own offline banner covers a daemon that goes away later.

## Links, attachments and safety

The window only shows the web app it got from `mxr web` and its own down
page. Links to other sites open in your default browser. Attachments you
download save to your Downloads folder, and an existing file is never
overwritten. The app grants no Tauri commands to any page, so the web app and
the mail it renders can't call into the shell.

## Still works in a browser

`mxr web` keeps working. It opens the same app at `http://mxr.localhost:42829`,
installable as a PWA if you prefer that to the desktop app.

## What it isn't

There is no menu-bar item, no notifications and no updater yet. Updates come
with the `mxr` you install, because the web app is embedded in the binary.
