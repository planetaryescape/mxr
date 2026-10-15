# Web axe findings after the polish sweep

Status: logged, not fixed. Two items are follow-ups outside this rung's files.

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
2. Loading skeleton `aria-label` on a generic div (MailViewParts.tsx). The fix is to give it `role="status"` (or to drop the label). Not done here: this file is outside the rung. It also explains the accessibility spec flake.
3. Badge/Chip merge. Deferred as the brief asked. `components/ui/badge.tsx` and `components/Chip.tsx` still both exist.
4. `docs/issues/paper-theme-contrast.md` says two Paper labels are below 4.5:1. The rung 5 axe run did not reproduce this on any Paper page. That note is not updated here; whoever owns it should re-measure before closing it.
