# Merges are inferred without evidence of a row collision

Status: deferred from R1c review.

`classify_eval_asks.rs` counts a miss as a merge when another email of the same sender domain has a matched item with the same kind and counterparty and a different reference. A missed October invoice A beside a correctly found November invoice B from the same sender is counted as a merge, though nothing collided.

The collision itself is visible in the store: a later email's upsert replaces the earlier row's `source_message_id` when the dedup key is equal (`crates/store/src/todos.rs`, `upsert_detected_todo`; the key is built in `crates/todo/src/pass.rs`, `dedup_key`). The eval cannot see that, because the replaced email leaves no trace.

## What to decide

- Whether the eval recomputes the dedup key for the two emails' expected items (kind, sender domain, object, due day) and counts a merge only when the keys are equal.
- Or whether the to-do pass records which messages a row absorbed, which is a behaviour change and belongs to R2c's item identity.
