# Due words can't express a time range or a past time

Status: open. Found while moving decision and delivery dates onto
`mxr_core::natural_time::due_words` (the helper #555 introduced for commitments).

## A range resolves to its end

`due_words` strips leading filler words until what's left parses. For
"between 2pm and 4pm" neither the whole phrase nor "2pm and 4pm" parses, but
"and 4pm" does, so the range resolves to 4pm. Checked with a throwaway test at
`585438304`: both phrases resolve to the "and 4pm" instant. Commitments inherit
this; deliveries ask the model to split a window into `eta_from` and
`eta_until`, but nothing in code stops a whole range reaching the helper.

Since D149 the commitment extractors store a phrase like this undated: it
drops words to find a reading (`DueWords::dropped_words`), so
`unambiguous_at` returns `None`. The fix below is still needed for the
decision and delivery extractors.

Fix in `natural_time`: either parse "between X and Y" / "X–Y" as a span and
expose both ends, or refuse a phrase whose kept suffix starts with a
conjunction. Add tests for both forms and for "Thursday between 2pm and 4pm",
which today can lose its start time when the model copies only "Thursday".

## Past times can't be read

The natural-time parser only produces future instants (`ensure_future`,
`crates/core/src/natural_time/resolve.rs:121-133`; roll-forward in
`calendar()` at `:251-293`). "Delivered today at 10:42" in an email received
at 15:00 resolves to 22:42. So delivery `delivered_at` no longer comes from
the model at all and stays the ingest time, and decision `decided_at` can only
resolve phrases the parser reads as upcoming relative to the message date.

Fix: a past-reading mode for `resolve_time` (anchor "today at 10:42" before
the reference time when the reference is later), used only by extractors that
date events which already happened.
