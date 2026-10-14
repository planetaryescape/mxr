# The cold-connect MailView test fails intermittently in CI

**Status:** open · **Found:** 2026-10-14 on main `5d359cc81` · **Severity:**
low; a rerun passes, but it can block a release check

## What happens

`src/features/mailbox/MailView.connection.test.tsx` > "cold mailbox connection
state" > "a real connect 503 pauses a cold Inbox instead of rendering it empty,
then recovers without reload" failed in Web Unit Tests on main (run 38624744364,
job 116061119998) after 6375 ms with `Unable to find an element with the text:
Inbox zero`. The same tree passed on PR #461, and a rerun of the job passed.
Nothing in that commit touched the mailbox.

## Likely direction

The test waits for recovery after a 503 with real timers; under CI load the
recovery takes longer than the find timeout. Check whether it uses fake timers
or a bounded `findBy` timeout, and make the recovery deterministic.
