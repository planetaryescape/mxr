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

## Not yet verified

Real Safari and the Tauri shell (WKWebView), whose input path differs from Playwright's WebKit. Playwright's ⌘C runs the copy command directly, so the `beforecopy` path that Safari's Edit menu validation needs is untested. Settle it in the desktop app: click into an HTML mail and press `j`; select text in the mail and press ⌘C, then paste.
