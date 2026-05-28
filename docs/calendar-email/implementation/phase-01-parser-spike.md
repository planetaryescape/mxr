# Phase 01: Parser Spike

Goal: choose the parsing/generation library path and replace nothing yet unless the spike is conclusive.

Outcome: `icalendar` is the chosen production parser for the first
email-invite slice. Reply generation shipped as a narrow daemon-owned
`METHOD:REPLY` builder rather than broad library generation. The implementation validates inline invites, folded lines,
recurrence identity, cancellation, replies, organizer/attendee parameters, and
warnings through `mxr-mail-parse` tests.

## Tasks

- Add a small experimental module or test-only spike for `calcard`.
- Parse fixture invites with:
  - inline `text/calendar; method=REQUEST`
  - `.ics` attachment
  - folded lines
  - `VTIMEZONE`
  - recurring event with `RRULE`
  - cancellation
  - attendee `PARTSTAT`
- Extract:
  - method
  - component kind
  - UID
  - sequence
  - recurrence id
  - summary
  - description
  - location
  - start/end
  - organizer
  - attendees + params
- Test whether `calcard` can generate `METHOD:REPLY`.
- If generation is awkward, test `icalendar` for reply generation.
- Document result in this file or a sibling decision note.

## Acceptance

- A chosen parser approach and reply-generation boundary are documented.
- At least five real-ish fixtures parse.
- Known parser limitations are listed.
- No production behavior changes unless explicitly justified.

## Known Limits

- No recurrence expansion; mxr stores and displays `RRULE`.
- No full time-zone resolution; original `DTSTART`/`DTEND` values are preserved.
- No automatic calendar mutation or CalDAV sync.
- Reply generation is limited to iMIP `METHOD:REPLY`.

## Validation

```sh
scripts/cargo-test -p mxr-mail-parse --tests
```
