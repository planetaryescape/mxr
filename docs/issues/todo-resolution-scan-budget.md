# To-do resolution scans use a durable, bounded model queue

**Status:** fixed · **Found:** 2026-10-12 while reviewing `.84` integration at
`6be3e2d3175d222adddad70c0b842a2ec2f91369` · **Severity:** medium

At baseline `0ac274f627ba0fc5e833916112c9a065bb402d49`, ambiguous later mail
triggered one sequential model request per matching task, each with an
eight-second timeout. A counted immediate model observed 10, 100 and 400 calls
for fixtures of 5 messages × 2 tasks, 20 × 5 and 40 × 10. These establish call
growth, not remote-model latency. Sync already returned before this detached
post-sync scan; there was no measured user-facing sync delay.

The post-sync scan now keeps deterministic matching local and persists ambiguous
message/task pairs. The existing to-do worker handles at most four model completion checks
per batch under one eight-second deadline. The existing provider fallback for
empty reasoning responses can make one extra HTTP attempt per check; these
attempts share the same deadline. It resumes remaining pairs on later
ticks and after restart, with durable five-attempt retry backoff. Disabled
models, privacy gates and the circuit breaker pause work without consuming an
attempt.

The queue records account, task and message identity plus an input digest and a
generation identifier. Conditional writes prevent an old result from consuming
newer work. Source quotes, task fingerprints, account ownership and newer
contradictory evidence still gate every review offer; no task changes without
acceptance.

Implementation: `crates/daemon/src/handler/todo_resolution/worker.rs`,
`crates/store/src/todo_resolution_queue.rs`, and migration
`082_todo_resolution_queue.sql`. Regression coverage includes aggregate limits,
restart, cancellation, edited inputs, disabled accounts, provider/privacy changes
and retry isolation in the corresponding daemon and store tests. Portable backups
preserve pending checks, attempt counts and completed pairs. Schema 81 backups
remain readable; restored profiles keep checks paused until recovery activation.

## Preserve these boundaries when changing the worker

The four-check budget belongs to this resolution consumer. It is not a global
provider-request quota: other model features have their own work, and one
completion can invoke the empty-reasoning fallback. The shared deadline bounds
model waiting across the batch, not every database operation in the tick.

Reserve the attempt and retry time before awaiting the provider. Cancellation
then leaves retryable work with backoff; a revision check prevents the old
result from completing replacement work. Completed and exhausted rows suppress
replay of unchanged inputs. This does not promise exactly-once remote execution
when a provider response is lost.

Keep the queue in the portable backup allowlist alongside the scan progress
stored in `todo_runs`. Restoring a checkpoint without its deferred work can
leave candidates behind the scan cursor. Verify both schema-81 compatibility
and schema-82 queue state using `crates/store/src/backup/tests/mod.rs`; keep the
recovery pause in `todo_loop` before it invokes the worker.
