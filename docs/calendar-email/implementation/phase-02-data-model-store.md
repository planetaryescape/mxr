# Phase 02: Data Model And Store

Goal: persist rich invite metadata and raw ICS locally.

Depends on: [Phase 01](phase-01-parser-spike.md)

## Tasks

- Add core invite types:
  - `CalendarInvite`
  - `CalendarInviteSummary`
  - `CalendarAttendee`
  - `CalendarPerson`
  - `CalendarMethod`
  - `CalendarParticipationStatus`
  - `CalendarDateTime`
- Add typed ID if needed.
- Add store migration for `calendar_invites`.
- Add `crates/store/src/calendar.rs`.
- Store raw ICS.
- Dedupe invite rows per message/calendar identity.
- Keep `MessageMetadata.calendar` as compatibility summary.
- Derive invite rows during sync/body insert path.
- Add store tests for insert/get/list/update replacement.

## Acceptance

- Provider parse -> store persists invite metadata.
- Raw ICS survives round trip.
- Existing body metadata still works.
- No provider-specific types leak into core/store.

## Validation

```sh
scripts/cargo-test -p mxr-core --tests
scripts/cargo-test -p mxr-store --tests
scripts/cargo-test -p mxr-provider-gmail --tests
scripts/cargo-test -p mxr-provider-imap --tests
```

