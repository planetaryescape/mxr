# Commitment date provenance: what was deferred

Status: open. From R0b (`by_when_source`, D149).

## `verify-dates` is not built

A model-written date (`model_date`) stays unverified until the user re-enters
it. An optional, non-destructive `mxr commitments verify-dates [--dry-run]`
could upgrade a `model_date` row to `words` when verbatim words in the
evidence (subject, cleaned body, signature) resolve to the stored date. It
would never clear a date or write a different one. Build it only if, after
seeing the list of unverified dates, hand-fixing the few that matter is not
enough. The `recheck-dates` branch (`feat/recheck-commitment-dates`,
`96e609d0c`) holds the Keep path and its fixtures (subject-only, snippet-only,
HTML-only, signature, quoted question) as a starting corpus.

## The relationship worker reads HTML-only mail as text

`RelationshipMessageSample.body` is `text_plain`, else `text_html`, else the
snippet (`contact_style.rs` `row_to_message_sample`), and the worker passes it
to `clean` as the text argument (`relationship/src/commitments.rs`). `clean`
treats text as HTML only when `looks_like_html_document` says so, which needs a
known opening tag (`<div`, `<p`, `<table`, ...), or `<html` / a comment plus
two common tags. An HTML-only message that opens with plain text or another tag
reaches the model with markup in it. Fix: carry whether the body came from
`text_html` on the sample and call `clean(None, Some(body), ..)` for it. The same
pattern reads `sample.body` in `commitments.rs` (twice), `summary.rs`,
`service.rs` (twice) and `messages_ack.rs`, so it is a shared change, not a
commitments one. Read from the code at the commit that adds this file; not
reproduced with a test.

## Draft candidates carry no source

`draft_commitment_candidates` has `by_when` but no source column. Promotion
after send marks a dated candidate `words`, which is right for every candidate
extracted since R0 (v0.6.120). A candidate extracted by an older daemon and
still unsent at upgrade would be promoted as `words`. Candidates are rewritten
whenever the draft is extracted again, so the window is small; add the column
if it ever shows up.

## The live extractors still read less than the model could

They check due words against reader-cleaned text without the subject, and the
relationship worker shares one 14,000-character budget across up to 12
messages. A deadline only in a subject, a P.S. below the signature or a quoted
question leaves the commitment undated. That is the safe failure, and R2's
shared evidence reader is the place to fix it.
