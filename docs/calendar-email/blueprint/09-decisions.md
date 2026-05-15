# Decisions

## D-CAL-001: Email Invite Handling Before Calendar Product

Decision: Build mail-derived invite handling first. Do not build full calendar sync/UI in the initial slice.

Rationale: The validated problem is received calendar invites in email. CalDAV and calendar UI are separate products with much higher maintenance cost.

## D-CAL-002: Use Standards-Based iMIP Replies

Decision: Accept/tentative/decline sends `METHOD:REPLY` over email.

Rationale: This works across Gmail, IMAP, SMTP, and other providers without provider-specific APIs.

## D-CAL-003: Use A Real iCalendar Library

Decision: Replace line scanning with the Rust `icalendar` crate for parsing and constrained `METHOD:REPLY` generation.

Rationale: RFC 5545 syntax and scheduling semantics are too large for custom parsing.

## D-CAL-004: Persist Raw ICS Locally

Decision: Store raw calendar text for parsed invite rows.

Rationale: Needed for debugging, dry-run, future parser improvement, and reliable reply generation.

## D-CAL-005: CLI First

Decision: CLI commands ship with daemon IPC before or with TUI/web support.

Rationale: Project principle. Also gives fast integration verification.

## D-CAL-006: No Silent Mutations

Decision: No automatic add/cancel/update of calendar state in the initial feature.

Rationale: Email invites are spoofable and stateful. User action and dry-run are required.

## D-CAL-007: Provider-Agnostic Core

Decision: Calendar invite logic normalizes into core/store/protocol types. Provider crates only parse/extract MIME data.

Rationale: Keeps Gmail/IMAP swappable.

## Open Questions

- Should raw ICS live in `calendar_invites.raw_ics` only, or also as a cached attachment-like local file?
- Should account aliases become user-configurable beyond stored account addresses and the primary account email?
- Should future JSON output add a stable `CalendarInvite` object distinct from body `CalendarMetadata`?
