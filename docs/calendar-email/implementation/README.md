# Calendar Email Implementation

These phase files are the original pickup plan for the first calendar-email
slice. The slice has since shipped; keep these files as implementation
lineage, not as the active backlog. Use
[../blueprint/02-current-state.md](../blueprint/02-current-state.md) for code
truth and [../synthesis/](../synthesis/) for durable lessons.

| Phase | File | Outcome |
|---|---|---|
| 01 | [phase-01-parser-spike.md](phase-01-parser-spike.md) | Pick parser approach and document reply-generation boundary. |
| 02 | [phase-02-data-model-store.md](phase-02-data-model-store.md) | Persist rich invite metadata and raw ICS. |
| 03 | [phase-03-cli-ipc-display.md](phase-03-cli-ipc-display.md) | CLI/IPC invite show/list. |
| 04 | [phase-04-rsvp-dry-run-send.md](phase-04-rsvp-dry-run-send.md) | Accept/tentative/decline with dry-run and send. |
| 05 | [phase-05-tui-web-search.md](phase-05-tui-web-search.md) | TUI/web cards and search support. |
| 06 | [phase-06-hardening.md](phase-06-hardening.md) | Recurrence/cancel/update hardening and fixtures. |

Do not pick these up as-is without first revalidating them against the current
code. Several items that read as future work here now exist in the repo.
