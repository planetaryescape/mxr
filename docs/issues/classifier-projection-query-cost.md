# Classifier projections repeat thread and delivery reads

Status: follow-up, not a release blocker. Observed at PR #687 head
`2053f18ed406b981ad56639b64a29703b4fd9a7a`; no isolated latency benchmark yet.

`handler/gist_kind.rs::thread_kinds` loads every envelope's `place_message`
before selecting the newest message from someone else. `AccountKinds::load`
then loads full threads, and `assessments::materialize` reloads those threads.
Keep the full-thread evidence needed for classification, but share its load
within a request and select gist candidates before loading each message.

`handler/home_projection.rs::released_specialists` queries source message IDs
for each released delivery. `handler/updates.rs::digest_from` does another
per-delivery source lookup. A batched store query could retain the account,
dismissal and expiry filters while reducing query count.

`handler/arrivals.rs::arrivals_at` also runs the bounded classifier report on
each arrivals request. The report validates up to 1,000 stored assessments by
loading their evidence. Measure this on a large mailbox before choosing a
smaller summary or a cache with explicit invalidation. A report error currently
fails the arrivals response; consider a logged unavailable summary if the
required placement reads have succeeded.

Before changing these paths, record query counts and timings for a long thread,
many retained deliveries and repeated arrivals reads. Verify identical stored
homes and deciding claims across modes, Inbox chips, arrivals and `mxr why`,
including specialist expiry and unlinked legacy delivery trackers.

## Fixed-time specialist projections use two clocks

At `c4a93e42073d3698006d4e22091628b1da33cbe2`,
`AccountKinds::load` passes `Utc::now()` to `released_specialists`, while
`place_threads` passes its caller's explicit `now`. Thread the caller's time
through `AccountKinds::load` and verify the same invite immediately before and
after expiry. This affects the live visibility window; the stored home remains
unchanged. Kept as a follow-up to the sync race fix.
