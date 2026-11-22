# Flaky e2e: the let-go dialog does not open after a reload

**Status:** open · **Found:** 2026-11-22 on main's merge commit for #1013 (e0e3042b6)

`let go of all acts on exactly the previewed cut, and u puts it back`
(`apps/web/e2e/updates.spec.ts:60`) failed in the `Web E2E (1)` job of run
45020688626. The same spec passed on PR #1013 shortly before. A re-run of
the failed job on the same commit passed with no code change.

## What failed

After `page.reload()`, the test presses `E` (`updates.spec.ts:92`) and waits
5 s for `updates-let-go-dialog`. The dialog never appeared:
`expect(locator).toContainText` reported "element(s) not found". The retry
then timed out at 90 s in `page.waitForResponse`. The first attempt had
already changed the shared daemon's state, so the retry did not start from
the state the test expects.

## Not caused by the test cull

The change between the passing PR run and the failing main run was test code
only. `git diff 96842311b e0e3042b6 -- apps/web` changes no runtime file
except `vitest.config.ts`, and the provider-fake fixture changes remove test
assertions, not demo data.

## Next step

Find out whether `E` can reach the page before the reloaded Updates view
binds its keys. The test checks that the reloaded row is not unread, which
can pass before the key handler is ready. If that is the cause, wait for a
signal that the view is interactive before pressing the key.
