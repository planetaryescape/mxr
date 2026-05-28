# Calendar Email

Calendar email support means handling meeting invitations embedded in email, not building a full calendar product by default.

## Current Status

The first useful slice has shipped. `mxr` now parses `text/calendar`
parts and `.ics` attachment payloads, persists invite rows in SQLite,
surfaces invites through CLI/IPC/TUI/web, supports `has:calendar` search,
and sends standards-shaped accept/tentative/decline replies through the
existing outbound mail path.

The docs in `blueprint/` now describe both the original research and the
current code truth. The files in `implementation/` are historical pickup
files for the shipped slice, not the active backlog.

This doc set is split into two parts:

| Area | Path | Purpose |
|---|---|---|
| Blueprint | [blueprint/](./blueprint/) | Research, standards, current-state audit, product boundaries, data model, and architecture. |
| Implementation | [implementation/](./implementation/) | Phased task files. Each phase is intended to be picked up as one implementation unit. |
| Synthesis | [synthesis/](./synthesis/) | Obsidian-style evergreen notes distilled from the calendar-email work. |

## Scope

The validated first problem is:

- detect calendar invites in email
- show them clearly
- explain whether the user can respond
- generate accept/tentative/decline replies safely

Out of initial scope:

- full CalDAV sync
- standalone calendar views
- event creation/editing unrelated to received mail
- reminder engine
- server-side scheduling

Those may become product surfaces later, but they are not required for email invite handling.
