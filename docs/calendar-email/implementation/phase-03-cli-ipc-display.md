# Phase 03: CLI And IPC Display

Goal: expose invite list/show through daemon IPC and CLI.

Depends on: [Phase 02](phase-02-data-model-store.md)

## Tasks

- Add protocol requests:
  - `ListInvites`
  - `GetInvite`
- Add response data variants.
- Classify requests as `core-mail`.
- Add daemon handlers.
- Add CLI:
  - `mxr invites list`
  - `mxr invite show <MESSAGE_ID>`
  - `--json`
  - `--jsonl` for list if useful
- Human output includes:
  - summary
  - organizer
  - time
  - location
  - method
  - current attendee/status
  - UID/sequence
  - warnings
  - available actions
- Add integration tests through daemon handler/CLI command path.

## Acceptance

- User can inspect invites without opening attachments manually.
- JSON output is stable and pipeable.
- TUI/web are not required yet.

## Validation

```sh
scripts/cargo-test -p mxr-protocol --tests
scripts/cargo-test -p mxr-daemon --tests
```

