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

Install the app with Homebrew:

```bash
brew install --cask planetaryescape/mxr/mxr-desktop
```

The cask installs `mxr` too if you don't have it. Each release also attaches
`mxr-desktop-<version>-aarch64.dmg` to its GitHub release. To build it
yourself, run `cd apps/desktop && npm ci --ignore-scripts && npx tauri build
--bundles app`; the bundle lands in
`apps/desktop/src-tauri/target/release/bundle/macos/mxr.app`, unsigned.

The release build is signed with a Developer ID certificate. Notarisation
runs when the release has App Store Connect API credentials. A build without
them is signed but not notarised, and macOS then asks you to confirm the
first launch. The release's job summary says which of the two happened.

## What it does

On launch the app runs `mxr web --print-url --no-open`, which starts or
reuses the local web bridge, and loads the address it prints. Closing the
window hides it; the Dock icon or the shortcut brings it back, and
Command+Q quits.

Only one copy runs. Opening a second one focuses the first.

The window remembers its size and position.

## The menu bar

Besides the standard mxr, Edit and Window menus, the menu bar has:

- **View**: Command palette (⌘K), Show or hide the assistant (⌘.), Keyboard
  help, and Enter Full Screen.
- **Go**: every place `g` reaches, such as Go to Now, To do, Go to Inbox and
  Go to Trash, then the tools: Rules, Settings, Diagnostics and the rest.
- **Message**: Reply, Reply all, Forward, Archive, Not now…, Move to…, Move
  to label…, Star or unstar, Mark read, Mark unread and Move to Trash.

The menus come from the same key definition as the web app and the TUI, and
a command does exactly what its key does. It acts on what is open or lit, asks
for the same confirmation, and says so when it has nothing to act on there.
Only the palette and the assistant have menu shortcuts. The single keys (`e`,
`r`, `g i`) work in the window as they do in a browser, and the Edit menu keeps
⌘Z, ⌘A and the other text shortcuts for typing.

To give a menu command a shortcut of your own, open **System Settings →
Keyboard → Keyboard Shortcuts → App Shortcuts**, add one for mxr, and type the
command's title exactly as the menu shows it, such as `Archive` or
`Go to Inbox`. Each title is the action's name in the key definition, so it
stays the same when you change the action's keys.

## New message from anywhere

Press Control+Option+Command+M in any app. mxr comes forward and opens a new
message over whatever you were looking at, so closing it with Esc puts you
back where you were. The shortcut is deliberately not Command+Shift+M, which Mail and
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

There is no menu-bar item, no notifications and no built-in updater. The web
app inside the window comes from the `mxr` you have installed, so
`brew upgrade mxr` updates it. `brew upgrade --cask mxr-desktop` updates the
window around it.
