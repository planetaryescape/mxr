# In WebKit, keys typed after clicking into an HTML mail do nothing

Status: fixed 2026-10-29 on fix/keys-after-html-mail-click. Found 2026-10-28 on fix/ci-health while working out why the WebKit job failed `compose-send.spec.ts` "an inline reply keeps its text and audience when closed and resumed" on main (run 40672677511, both attempts).

WebKit no longer runs in CI (2026-10-28: CI runs Chromium only), so nothing catches this gap or a regression of it except a manual `npm run e2e:webkit` run.

## What happens

Open a conversation whose message renders as HTML, click in the middle of the message, then press `r`. Chromium opens the inline reply. WebKit does nothing.

The spec clicked the reader's centre to give it focus before pressing `r`, so it failed in WebKit only. It now clicks the reader's heading, so the WebKit job checks the reply journey instead of this gap.

## Evidence

A throwaway probe in Playwright 1.59 (WebKit 2272 and Chromium 1223, local daemon at 4a99a8ed6 plus this branch) recorded the following after the click:

- Both engines: `document.activeElement` is the message's `<iframe>`.
- Chromium: `r` reaches the app's dispatcher through `forwardKeydown` (`apps/web/src/features/thread/MessageBody.tsx`) and the reply opens.
- WebKit: capture-phase `keydown` listeners on both the parent `window` and the iframe's `contentDocument` record no event at all, and no reply opens.

The iframe is sandboxed without `allow-scripts` (`IFRAME_SANDBOX`). `forwardKeydown` is attached to its document from the parent at `load`.

## Cause

WebKit runs no event listeners inside a frame sandboxed without `allow-scripts`, including listeners the parent page attached to the frame's document. A probe on 2026-10-29 (Playwright 1.59, WebKit 2272, Chromium 1223) recorded, after a click in the message: Chromium fired the frame's `mousedown` and `keydown` listeners and the forwarded key reached the dispatcher; WebKit fired neither, so `forwardKeydown`, the link handler and `onInteract` never ran. Only the parent's `window` `blur` fired, with `document.activeElement` on the `<iframe>`. Chromium never lost keys.

## Fix

`returnFocusFromFrame` (`apps/web/src/features/thread/messageFrameFocus.ts`), wired from `MessageBody`, works from the page side, so the sandbox keeps no scripts. When the window blurs with focus in the message frame, it waits one task and focuses the nearest focusable ancestor (the reader's scroll pane), which also makes the reader the active pane. Focusing during the `mousedown` itself does not stick in WebKit; one task later it does. Focus stays in the frame when Tab put it on a link there, so mail links remain reachable from the keyboard (Chromium still forwards keys from there; WebKit does not).

With focus back on the page, ⌘C reaches the page, not the frame. The page's `copy` handler copies the message's selection (plain text and HTML) when the page has no selection of its own, and `beforecopy` is cancelled in that case, which is what enables Copy for a selection-less page in Safari.

Verified in Playwright on Chromium and WebKit: after a click in the message `?` opens keyboard help (asserted in `html-rendering.spec.ts`, which fails on WebKit without the fix); a drag selection survives the focus return; ⌘C then ⌘V pastes the selected text (empty without the copy handler); a link click still opens a popup.

## Verified in Safari and the desktop app

Verified 2026-10-29 in Safari 27.0.1 and the desktop app (`/Applications/mxr.app` 0.6.153, WKWebView) on macOS 27.0, both loading the web app from Homebrew's mxr 0.6.189 against an isolated `mxr demo` profile. The desktop shell only hosts the page, so the shell's older version does not matter: it ran a stand-in `mxr` on its PATH that printed the demo bridge's URL. Input came from real HID mouse events (CGEvent) and System Events keystrokes and menu clicks, not a test driver. Every step passed in both:

| Step | Safari 27.0.1 | Desktop app (WKWebView) |
| --- | --- | --- |
| Click in an HTML message's body, press `?` | Keyboard help opened, listing "Reader (this view)" | Same |
| Double-click a word in the message, press ⌘C | Clipboard held `queries`, as plain text and HTML | Clipboard held `question`, as plain text and HTML |
| Drag-select text in the message, then Edit → Copy | Copy was enabled; clipboard held ` window functions, u` | Copy was enabled; clipboard held `s it is for now and revi` |
| Press `?` after selecting | Help opened | Help opened |

The message was the demo's "SQLite Notes: ten queries you should know" in Safari and "Launch checklist for Project Aurora" in the desktop app, both shown Formatted in the sandboxed frame. While focus is back on the page, the selection in the message stays visible in the inactive (grey) highlight.
