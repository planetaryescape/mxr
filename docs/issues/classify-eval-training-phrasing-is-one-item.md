# The "training" phrasing is labelled as one item

Status: deferred from R1c review, a labelling policy call for R2c.

The dated-request family's "training" phrasing (`crates/provider-fake/src/fixtures/scenarios/bulk/people.rs`) says "the security training must be completed by {due}. Please finish it and send me the certificate." It is labelled as one `other` item (`variant_ask` in `bulk/mod.rs`). A judge that splits it into `other` (complete the training) and `send` (the certificate) would be graded with a false positive and a lower precision for the right reading.

## What to decide

- Whether sending the certificate is a separate action or the proof of the same obligation. Chapter 37 makes one item per distinct action; the certificate is evidence that the training was done, which argues for one item.
- If it is two, label two items for this phrasing, with the same due words, and let the rest of the dated-request phrasings stay at one.
