---
title: No native desktop app
description: Why mxr uses a local daemon plus installable web app instead of an Electron or Tauri shell.
---

mxr doesn't ship a native desktop app.

That's a product choice, not a missing wrapper. The native part of mxr is
the daemon: SQLite, Tantivy, provider sync, credentials, `$EDITOR`,
filesystem access, and long-running jobs. The visual app is the browser
client served by `mxr web`.

Native where it matters. URL where it doesn't.

## What we stopped shipping

We removed the Electron shell, desktop updater, native app bundle, and
desktop packaging jobs. No `.app`, `.dmg`, `.deb`, `.rpm`, or Electron
release lane.

That removes a real maintenance bill: bundled Chromium, updater behavior,
platform packaging, code signing, notarization, and support paths that
mostly existed to put a browser window around the same local daemon.

## What replaces it

Run:

```bash
mxr web
```

mxr opens `http://mxr.localhost:42829`, served by the local daemon bridge.
The web app is installable as a PWA, so you still get the parts people
expect from desktop software:

- an app icon
- a standalone window
- cached app shell assets
- silent app updates on reload

The mailbox data is still local-first. The browser doesn't own SQLite,
provider credentials, or sync loops. It talks to the daemon over the local
HTTP/WebSocket bridge.

## Why not wrap the web app?

An Electron or Tauri wrapper would make mxr look native, but it wouldn't
make the product more local-first. The daemon already does that job.

A wrapper would add another runtime boundary, another update path, and
another packaging surface. A PWA gives mxr the installability story
without making us maintain a second app around the first one.

## What still installs

You still install `mxr` itself:

```bash
brew install planetaryescape/mxr/mxr
```

That gives you:

- `mxr` TUI for terminal-native mail
- `mxr ...` CLI for scripts and agents
- `mxr daemon` for sync, search, rules, and background work
- `mxr web` for the installable browser UI

The browser is the window. The daemon is the system.
