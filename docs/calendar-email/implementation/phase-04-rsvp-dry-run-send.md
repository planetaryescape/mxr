# Phase 04: RSVP Dry-Run And Send

Goal: implement accept/tentative/decline as standards-shaped iMIP replies.

Depends on: [Phase 03](phase-03-cli-ipc-display.md)

## Tasks

- Add protocol request:
  - `RespondInvite { message_id, action, dry_run }`
- Add preview/result response types.
- Implement attendee identity matching.
- Block response when:
  - no matching attendee
  - ambiguous attendee
  - missing organizer
  - missing UID
  - unsupported method/component
  - stale sequence
  - fatal parser warning
- Generate `METHOD:REPLY` ICS.
- Build outbound email via existing send path.
- Add CLI:
  - `mxr invite reply <MESSAGE_ID> accept --dry-run`
  - `mxr invite reply <MESSAGE_ID> tentative --dry-run`
  - `mxr invite reply <MESSAGE_ID> decline --dry-run`
  - same without `--dry-run`
- Dry-run prints recipient, attendee, event identity, warnings, generated ICS.
- Record local response only after send succeeds.
- Record activity through daemon recorder.

## Acceptance

- Dry-run and real send share selection/generation path.
- Generated MIME has `text/calendar; method=REPLY`.
- Attendee `PARTSTAT` changes correctly.
- No provider-specific scheduling API used.

## Validation

```sh
scripts/cargo-test -p mxr-daemon --tests
scripts/cargo-test -p mxr-outbound --tests
```

Also run a real daemon + fake provider smoke test if fixtures support it.

