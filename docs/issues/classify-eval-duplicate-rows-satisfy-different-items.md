# A duplicate row can satisfy a different expected item

Status: deferred from R1c review.

`classify_eval_asks.rs` pairs a to-do row with an expected item by kind and direction, then compares fields. It does not require a row to be about the item it is paired with. For two expected Acme invoices A (10.00) and B (20.00), two rows titled "Pay invoice A", both 10.00, are paired one to A and one to B. The grader reports two true positives and no false negatives, and the field comparison reports the amount of B as wrong; the duplicate of A counts as a hit on B.

Today an email makes at most one to-do row, so the case does not arise in the corpus. It matters when R2c makes one row per distinct action, and several expected items of one kind share an email.

## What to decide

- Whether a pair needs a minimum agreement (for example the reference, or the amount) before it counts as a true positive, and what a below-minimum pair is called: a false positive plus a false negative.
- Whether the row needs to carry the reference the label has, which `todos` stores only inside the dedup key.
