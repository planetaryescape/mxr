# Web axe findings after the polish sweep

Status: logged, not fixed. Two items are follow-ups outside this rung's files.

Re-checked 2026-11-05 on `fix/web-small-followups` (base `d574d0e29`). Item 2 is fixed, and item 1 and the Badge/Chip item are still open. None of the three is a label, role or contrast fix on a named element, so this pass changes no code:

- `region` on the resize handle: the handle is already a labelled focusable separator (`apps/web/src/components/ui/resizable.tsx:80`). Axe flags its place outside any landmark, which needs a landmark decision for the sidebar shell.
- Badge/Chip merge: a refactor that the rung deferred.
- Paper contrast: the measured ratios above all pass 4.5:1, and the remaining re-measure belongs with `paper-theme-contrast.md`.

## What

An axe run (WCAG 2.0/2.1 A and AA plus best-practice rules) covered Now, Messages, To do, Updates, Reading, Archive and Inbox at 1440px in all four themes: Midnight, Light, Eclipse and Paper. That is 28 page-theme runs. Every run passed `color-contrast`, and no run had a serious or critical violation. Two moderate findings remain. Both come from shared chrome or shared loading states that this rung did not change.

## Evidence

Run against `feat/polish-sweep` on top of origin/main db368832b, with the rung 5 code. Results were read from axe's JSON output in the worker's scratchpad.

| Page | Theme | Rule | Impact | Node |
| --- | --- | --- | --- | --- |
| all 7 pages | all 4 | `region` | moderate | `div[data-slot="resizable-handle"][aria-label="Resize sidebar"]` (components/AppShell.tsx:228) |

The Archive `landmark-complementary-is-top-level` finding was fixed in this rung. Its `aside` elements became `section` elements (features/archive/ArchiveRoute.tsx, Record and Subscription panes).

Paper muted text: `--muted-foreground` (hsl 30 12% 34%) measures 6.53:1 on `--background`, 6.80:1 on `--surface`, and 5.85:1 on `--muted`. These were computed from the tokens in apps/web/src/styles/tokens.css:388-410. The axe `color-contrast` check passed on every Paper page.

Incomplete (not violations): axe reported 3 `color-contrast` nodes per page that it could not resolve automatically. It did not record their targets.

Flaky, pre-existing: the `aria-prohibited-attr` serious violation on `div[aria-label="Loading conversations"]` (features/mailbox/MailViewParts.tsx:14-19). It appears only while the list skeleton is still visible when the axe check runs. This failed `e2e/accessibility.spec.ts` for `/messages` twice in three runs.

## Follow-ups

1. `region` on the sidebar resize handle. AppShell.tsx:228 renders the handle outside any landmark. Wrapping the handle's container in a labelled region, or hiding the handle from axe's region scan, fixes it. Not done here: AppShell is outside this rung's files.
2. Fixed in W4b: `MailViewParts.tsx` is gone and the shared `PageSkeleton` carries `role="status"` with its label, which also removes the accessibility spec flake.
3. Badge/Chip merge. Deferred as the brief asked. `components/ui/badge.tsx` and `components/Chip.tsx` still both exist.
4. `docs/issues/paper-theme-contrast.md` says two Paper labels are below 4.5:1. The rung 5 axe run did not reproduce this on any Paper page. That note is not updated here; whoever owns it should re-measure before closing it.
