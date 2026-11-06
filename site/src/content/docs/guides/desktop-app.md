---
title: Desktop app
description: A macOS window for your selected local or paired remote mailbox, with a global new-message shortcut.
---

The mxr desktop app opens your selected mailbox in its own window. A
system-wide shortcut starts a new message, and when mxr is unavailable the
app shows the connection problem.

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

On launch the app runs `mxr web --session-json --no-open`. For a local
workspace, it starts or reuses the web bridge. For a saved remote connection,
it opens that host over HTTPS and exchanges a single-use code for a browser
session. Its launch page exchanges the code even if a previous session is
still open. The code travels in a form POST, never the launch address. Closing the
window hides it; the Dock icon or the shortcut brings it back, and
Command+Q quits.

Only one copy runs. Opening a second one focuses the first.

The window remembers its size and position.

## A paired remote mailbox opens in the same window

Select the host with `mxr connect` and complete host approval before opening
the desktop app. It uses that saved connection and the permissions approved
for it. Upgrade the CLI and desktop app together; older CLI versions do not
support the session launch handshake.

The host must serve its web app at its configured HTTPS origin. The desktop
app does not start a second local mailbox when the host is unavailable.
Revoking the parent registration also revokes its browser access. To change
workspaces, change the saved connection and quit and reopen the desktop app.
Open tabs and retained drafts stay attached to their original workspace.

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
seconds, the window shows "mxr isn't running". Inspect the saved connection:

```bash
mxr connect --status
```

For a local workspace, start `mxr daemon`. For a remote workspace, check that
the selected host is reachable. Press **Try again** after resolving the
connection problem. Once the web app is open, its offline banner covers a
daemon that goes away later.

## Links, attachments and safety

The window only shows the web app it got from `mxr web` and its own down
page. Links to other sites open in your default browser. Attachments you
download save to your Downloads folder, and an existing file is never
overwritten. The app grants no Tauri commands to any page, so the web app and
the mail it renders can't call into the shell.

## Still works in a browser

`mxr web` opens the selected local or remote app in a browser. Local mail
uses `http://mxr.localhost:42829`; a paired remote connection uses its host
origin and prompts for the single-use code printed by the CLI. The app is
installable as a PWA if you prefer that to the desktop app.

## What it isn't

There is no menu-bar item, no notifications and no built-in updater. The web
app inside the window comes from the `mxr` you have installed, so
`brew upgrade mxr` updates it. `brew upgrade --cask mxr-desktop` updates the
window around it.
