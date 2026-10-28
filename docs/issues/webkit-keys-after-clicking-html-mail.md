# In WebKit, keys typed after clicking into an HTML mail do nothing

Status: open. Found 2026-10-28 on fix/ci-health while working out why the WebKit job failed `compose-send.spec.ts` "an inline reply keeps its text and audience when closed and resumed" on main (run 40672677511, both attempts).

## What happens

Open a conversation whose message renders as HTML, click in the middle of the message, then press `r`. Chromium opens the inline reply. WebKit does nothing.

The spec clicked the reader's centre to give it focus before pressing `r`, so it failed in WebKit only. It now clicks the reader's heading, so the WebKit job checks the reply journey instead of this gap.

## Evidence

A throwaway probe in Playwright 1.59 (WebKit 2272 and Chromium 1223, local daemon at 4a99a8ed6 plus this branch) recorded the following after the click:

- Both engines: `document.activeElement` is the message's `<iframe>`.
- Chromium: `r` reaches the app's dispatcher through `forwardKeydown` (`apps/web/src/features/thread/MessageBody.tsx`) and the reply opens.
- WebKit: capture-phase `keydown` listeners on both the parent `window` and the iframe's `contentDocument` record no event at all, and no reply opens.

The iframe is sandboxed without `allow-scripts` (`IFRAME_SANDBOX`). `forwardKeydown` is attached to its document from the parent at `load`.

## Not yet known

Whether real Safari and the Tauri shell (WKWebView) behave the same as Playwright's WebKit, whose input path differs from a user's. Settle it by clicking into an HTML mail in the desktop app and pressing `j` or `r`. If it reproduces, keys after reading an HTML mail are dead on macOS desktop until the user clicks outside the message.
