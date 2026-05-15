# Phase 06: Hardening

Goal: handle real-world invite edge cases after the core journey works.

Depends on: [Phase 05](phase-05-tui-web-search.md)

## Tasks

- Add fixture corpus:
  - Gmail invite
  - Outlook/Exchange invite
  - Apple Calendar invite
  - Thunderbird-generated reply
  - cancellation
  - updated sequence
  - recurring series
  - single recurring instance update/cancel
  - forwarded invite where current user is not attendee
- Harden stale update detection.
- Improve recurrence display without full expansion.
- Add organizer-change warning.
- Add alias configuration for attendee matching.
- Add import/export raw ICS command if still useful.
- Add repair/backfill command for existing stored messages with calendar metadata.

## Acceptance

- Known major providers parse into same core model.
- Unsafe replies are blocked with actionable explanation.
- Existing users can backfill invite rows without resyncing all mail.

## Validation

```sh
scripts/cargo-test -p mxr-mail-parse --tests
scripts/cargo-test -p mxr-store --tests
scripts/cargo-test -p mxr-daemon --tests
```

