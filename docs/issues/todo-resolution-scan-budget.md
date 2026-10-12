# To-do resolution scans have no aggregate model-call budget

**Status:** open · **Found:** 2026-10-12 while reviewing `.84` integration at
`6be3e2d3175d222adddad70c0b842a2ec2f91369` · **Severity:** medium; aggregate
latency has not been measured

`crates/daemon/src/handler/todo_resolution.rs` processes candidate messages and
their matching open tasks in nested sequential loops (`scan_messages`, lines
44–230). When rule matching cannot decide, each task awaits `model_resolution`
inline. One model request is capped at eight seconds (line 293), but
`list_open_todos_for_resolution_context` in `crates/store/src/todos.rs` (lines
815–840) has no result limit, and the scan has no aggregate time or call budget.
`crates/daemon/src/handler/todos.rs` awaits this scan on the normal post-sync
path (lines 62–65). A sender/thread with many open tasks and ambiguous later
mail can therefore delay the caller through multiple model timeouts. This is a
source-level bound analysis, not a measured user-facing delay.

Do not cap the loop by dropping remaining candidates: ordinary scans and
first-run backfill are the only opportunities to inspect some messages, so a
skipped item could lose its review offer permanently. Measure representative
scan sizes first. If an aggregate budget is needed, persist deferred
message/task work and resume it on a bounded background worker; keep the
existing account/task/fingerprint retry ledger as per-input backoff.
