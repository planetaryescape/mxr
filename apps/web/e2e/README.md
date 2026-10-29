# Web E2E suite (Playwright)

These specs run the web app against a real `mxr` daemon (the fake provider's demo mailbox, started by `scripts/e2e-server.mjs`) with SQLite storage. Each test is the one journey that guards a core capability end to end: browser → bridge → daemon → store and back, or a daemon or sync event reaching the page.

## The journeys

| Spec | Capability it guards |
| --- | --- |
| `compose-send.spec.ts` | Sending: the undo window runs out, the daemon sends, the message is in Sent. |
| `triage.spec.ts` | Archive: Archive submits exactly what the daemon reviewed, the reader advances, the conversation is in All Mail, `u` undoes it. |
| `mutations.spec.ts` | A batch Archive where the provider rejects one message keeps Undo for the one that completed. |
| `messages.spec.ts` | Done in Messages takes the conversation out of attention in the daemon, and `u` brings it back. |
| `search.spec.ts` | Newly synced mail is searchable; archive from results and undo against the daemon. |
| `snooze.spec.ts` | Not now: the conversation waits on `/snoozed` until Wake now returns it to the inbox. |
| `labels.spec.ts` | A label created and applied from the picker is stored and has its own list. |
| `mode-move.spec.ts` | A mode move (`X`) and its undo reach a second client over the event stream. |
| `freshness.spec.ts` | A new message reaches the page live; a failing sync turns into a warning that links to its details. |
| `daemon-stopped.spec.ts` | With the daemon stopped, loaded mail stays usable and changes are refused, then everything recovers without a reload; a reply typed during the outage saves once it is back. |
| `updates.spec.ts` | Let go of all acts on exactly the previewed set, and undo restores it. |
| `unsubscribe.spec.ts` | Unsubscribe commits the daemon's own preview token, for both choices. |
| `sorting-rules.spec.ts` | A sorting rule's preview, its apply and a later personal correction agree in the daemon. |
| `onboarding-redirect.spec.ts` | A blank install lands on onboarding from any URL. |
| `token-expiry.spec.ts` | An expired bridge token routes to the token page with an alert. |
| `html-rendering.spec.ts` | HTML mail renders in a sandbox without scripts, with trackers stripped and remote images held back (only a real browser enforces the sandbox). |

`speed.spec.ts` is not part of this run. It is the keydown-to-paint gate against a production build: `npm run e2e:perf` (`playwright.perf.config.ts`).

## What belongs here

Add to a capability's journey before adding a test. A new spec needs a capability no journey above covers and an assertion that Vitest (`apps/web/src/**/*.test.tsx`) and the Rust tests can't make.

Not here: layout, responsive widths and resizing, motion, copy and teaching text, keyboard and shortcut details, control inventories, axe sweeps, loading states, error pages, date parsing and reducer logic. Component state and keys run in Vitest in milliseconds; the keymap definition tests own the keys; token contrast is checked in `src/styles/tokens.contrast.test.ts`. Never check in milestone or spike specs (`w6-*.spec.ts`, `milestone-*.spec.ts`, `spike-*.spec.ts`).

## Running it

`npm run e2e` runs the whole suite in Chromium; pull requests and `main` both run it (`npm run e2e:engine` is the same run). `npm run e2e:webkit` runs it on WebKit.

Each run starts its own daemon on `MXR_E2E_BRIDGE_PORT` (default 17777, control port one above) and Vite on `MXR_E2E_APP_PORT` (default 5173), with its state in `MXR_E2E_RUNTIME_DIR`. Give a second checkout its own ports and directory. `scripts/e2e-server.mjs` refuses to start when the bridge or control port is taken, because a daemon that can't bind its bridge keeps running without it and the suite would otherwise run against the other daemon. `MXR_E2E_BIN` points at the `mxr` binary to run (default `target-cli/debug/mxr`).

Every spec shares one daemon and one mailbox, on one worker, so a test that changes shared state should leave it as it found it: undo the change, or archive the mail it delivered.
