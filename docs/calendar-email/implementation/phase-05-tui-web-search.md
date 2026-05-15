# Phase 05: TUI, Web, Search

Goal: make invite handling visible in interactive clients and searchable.

Depends on: [Phase 04](phase-04-rsvp-dry-run-send.md)

## Tasks

- TUI:
  - render invite block in message view
  - show actions when available
  - show warnings when actions disabled
  - add keybindings/actions for accept/tentative/decline/details
- Web:
  - preserve/fetch invite metadata
  - render invite card in thread view
  - add action buttons with dry-run/confirm flow
  - keep bridge routes thin IPC wrappers
- Search:
  - add `has:calendar`
  - optionally add `calendar:request|cancel|reply`
  - add tests for parser/filter behavior
- Update help text and command palette where relevant.

## Acceptance

- Same daemon capability is reachable from CLI, TUI, and web.
- Disabled actions explain why.
- `has:calendar` finds invite messages.

## Validation

```sh
scripts/cargo-test -p mxr-tui --tests
scripts/cargo-test -p mxr-daemon --tests
```

Run web tests if touched:

```sh
npm --prefix apps/web test
```

