# Calendar Email Blueprint

## Index

| # | Document | Topic |
|---|---|---|
| 00 | [Overview](00-overview.md) | Product boundary, goals, non-goals, topic scoring |
| 01 | [Standards](01-standards.md) | RFC 5545 iCalendar, RFC 5546 iTIP, RFC 6047 iMIP |
| 02 | [Current State](02-current-state.md) | What `mxr` already does and where it falls short |
| 03 | [Libraries](03-libraries.md) | Rust and mature calendar library options |
| 04 | [Data Model](04-data-model.md) | Invite/event/attendee storage and raw ICS retention |
| 05 | [MIME And Parsing](05-mime-and-parsing.md) | Provider extraction, text/calendar, .ics attachments |
| 06 | [RSVP And Sending](06-rsvp-and-sending.md) | Accept/tentative/decline, dry-run, outbound iMIP |
| 07 | [Security](07-security.md) | Spoofing, stale sequence, identity, privacy |
| 08 | [Surfaces](08-surfaces.md) | CLI, IPC, TUI, web, search, activity |
| 09 | [Decisions](09-decisions.md) | Settled decisions and open questions |
| — | [Synthesis notes](../synthesis/) | Durable concepts learned from implementing email calendar invites |

## Reading Order

Read 00, 01, and 02 first. Then use the topical files as needed. The implementation phases in [../implementation/](../implementation/) are historical pickup files for the shipped first slice.
