# The cold-connect MailView test fails intermittently in CI

**Status:** resolved 2026-10-14 on `test/deflake-mailview-cold-connect` · **Found:** 2026-10-14 on main `5d359cc81` · **Severity:**
low; a rerun passes, but it can block a release check

## What happens

`src/features/mailbox/MailView.connection.test.tsx` > "cold mailbox connection
state" > "a real connect 503 pauses a cold Inbox instead of rendering it empty,
then recovers without reload" failed in Web Unit Tests on main (run 38624744364,
job 116061119998) after 6375 ms with `Unable to find an element with the text:
Inbox zero`. The same tree passed on PR #461, and a rerun of the job passed.
Nothing in that commit touched the mailbox.

## Cause

The test's stand-in daemon held every status probe open while the daemon
was down, not just the first. `waitForPausedColdQueries` releases only the
first. When the 1 s backoff probe (`scheduleProbe` in
`apps/web/src/lib/daemonAvailability.ts`) fired before the test sent its
focus event, that probe was held too and never settled. `probeDaemon`
shares an in-flight probe (`probing ??=`), so the focus probe waited on the
held one and the Inbox never recovered. On a fast machine focus came first;
on a busy CI runner it didn't. Waiting 1.5 s before the focus event
reproduced the failure every time.

## Fix

The stand-in holds only the first probe; later ones answer straight away.
The test passes with the 1.5 s wait in place and in 15 back-to-back runs
without it. The app code was right: a real probe always settles.
