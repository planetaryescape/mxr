# All conversations still exceeds the cold-read target

Status: follow-up, not a release blocker under the explicit ship instruction.
Observed at PR #722 source head `775e19513`.

The isolated 111,345-message mailbox measured All at 254 ms in a full view
sweep and 290 ms as the first view after a separate restart. Both exceed the
adopted 250 ms target. Repeated 28–31 ms responses do not satisfy that target.
The released baseline failed after 10,879 ms; the substantial fix ships
without claiming this remaining budget gap is resolved.

The final trace measured 279 ms in composition: primary desk envelopes
completed at 69 ms, people facts at 135 ms, and selected-preview thread reads
occupied the later path through 246 ms. Decoded history facts were reused
only after source/facet/view clock, account, context and time validation;
no complete All response existed before this traced request.

Profile row construction/ranking and the remaining selected-preview work
before changing them. Keep live turns, Done state, people links, contact
facts, filters and ranking live; preserve account boundaries, selected text,
quote lookback and complete conversation attachments. Compare normalized
replies exactly, not only row counts.

Acceptance: independently restarted optimized daemons, normal startup
maintenance complete, tracing off, no prior All-response cache entry, cold
All consistently at or below 250 ms. Also measure a daemon-mediated source
change: direct SQLite writes do not invalidate the outer response cache and
are not valid evidence for that experiment. Never alter real mail to test it.

See [the audit](../research/mail-view-performance-2026-10-26.md) for the
baseline, prior candidates, tests and complete measurement context.
