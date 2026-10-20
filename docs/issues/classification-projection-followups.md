# Classification projection follow-ups

Status: deferred observations, not release blockers for the independent-review repair.
Do not add a purpose cache, migration or nested rule locks without evidence.

## Measure repeated purpose evaluation before memoizing

`crates/daemon/src/handler/mail_kind.rs` computes authored/purpose evidence for
classification, reasons, response occurrence and attention. Some callers repeat
that work for the same message. There is no measured real-scale regression in
this review. Profile a synthetic mailbox first; if material, consider a bounded
request-local result, with explicit freshness/intent ownership, rather than a
persisted purpose cache.

## Check Not sure candidate coverage after policy changes

`handler/arrivals.rs::not_sure_today` starts with the store's persisted candidate
selection before projecting current policy. A newly derived conflict that was
not a stored candidate may therefore be absent from the daily question set.
Reproduce this with synthetic old placements and current corrections before
changing the selection or its daily cap. Preserve answered-question history.

## Trace read-gate ownership before changing synchronization

`handler/arrivals.rs::get_arrival_modes` and the Freshness placement read do not
take the same rule-mutation guard as `effective_window`. A concurrent correction
can produce a mixed read that heals on refresh. Map all callers/reentrancy and
define the required consistency boundary before adding a lock: blindly adding
the guard to a shared helper can deadlock callers that already hold it. No
locking or cache redesign is included in the current repair.
