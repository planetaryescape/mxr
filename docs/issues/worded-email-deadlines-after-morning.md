# Worded email deadlines disappear after the default morning

Status: open. Observed while validating the dependency migration superseding
[PR826](https://github.com/planetaryescape/mxr/pull/826), based on `origin/main`
`110fbf31fc1093efa7b2fccf54a4580ed28a4a23`.

`crates/todo/src/dates.rs::parse_date` sends worded dates to the scheduling
resolver, `mxr_core::natural_time::resolve_time`. That resolver requires a future
instant and supplies 09:00 when no time is written. A probe of the unchanged
resolver accepted `1 November 2026` against `2026-11-01T08:00:00Z`, then returned
`InPast` against `2026-11-01T10:00:00Z`.

The isolated `same_currency_reminders_and_payment_escalation_reuse_one_identity`
test, with `MXR_CLASSIFY_TRACE=1`, stored no date for
`Account #A123 amount due £25 by 1 November 2026.`. Its assessment had
`required_fields_present: false`, answered `not_sure` and projected no task.
The natural-time and todo date-parser sources match the named main commit.

The dependency PR makes the identity and lifecycle fixtures use future dates,
keeping their assertions and computed legacy month keys. It also replaces an
expired fixed calendar start with a future start. These fixtures test identity,
undo and upgrade behaviour.

Next action: add regression coverage for an explicit same-day email deadline
after 09:00 and an explicit past deadline. Resolve email calendar days without
losing the existing end-of-day semantics in `dates::end_of_day`, while preserving
the scheduling resolver's future-only contract.
