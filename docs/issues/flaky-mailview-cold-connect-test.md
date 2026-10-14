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

## Next step

The cause isn't established. Run the test in a loop under CPU load
(for example `for i in $(seq 50); do npx vitest run src/features/mailbox/MailView.connection.test.tsx || break; done`)
and read how it waits for recovery after the 503 (real or fake timers, `findBy`
timeout) before changing it.
