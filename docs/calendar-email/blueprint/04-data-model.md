# Data Model

Calendar invites should become first-class mail-derived data, not just a larger `MessageMetadata.calendar` blob.

## Principles

- SQLite remains canonical.
- Search indexes are rebuildable from SQLite.
- Provider adapters map into provider-agnostic types.
- Daemon owns workflows.
- CLI remains canonical user surface.
- RSVP mutations must be dry-runnable.
- Raw ICS should be retained locally for inspectability and reply generation.

## Core Types

Proposed core-level types. Names are directional, not final.

```rust
pub struct CalendarInvite {
    pub id: CalendarInviteId,
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub method: CalendarMethod,
    pub component_kind: CalendarComponentKind,
    pub uid: String,
    pub recurrence_id: Option<String>,
    pub sequence: Option<i64>,
    pub dtstamp: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<CalendarDateTime>,
    pub ends_at: Option<CalendarDateTime>,
    pub status: Option<CalendarEventStatus>,
    pub organizer: Option<CalendarPerson>,
    pub attendees: Vec<CalendarAttendee>,
    pub current_user_attendee: Option<CalendarAttendee>,
    pub rsvp_requested: bool,
    pub raw_ics: String,
    pub parse_warnings: Vec<String>,
}
```

Enums:

- `CalendarMethod`: request, reply, cancel, publish, add, refresh, counter, decline_counter, unknown.
- `CalendarComponentKind`: event, todo, journal, freebusy, timezone, unknown.
- `CalendarParticipationStatus`: needs_action, accepted, declined, tentative, delegated, completed, in_process, unknown.
- `CalendarDateTime`: UTC, local with TZID, floating, date-only, unresolved.
- `CalendarInviteAction`: accept, tentative, decline.

## Store Tables

Proposed table:

```sql
CREATE TABLE calendar_invites (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    method TEXT NOT NULL,
    component_kind TEXT NOT NULL,
    uid TEXT NOT NULL,
    recurrence_id TEXT,
    sequence INTEGER,
    dtstamp INTEGER,
    summary TEXT,
    description TEXT,
    location TEXT,
    starts_at_json TEXT,
    ends_at_json TEXT,
    status TEXT,
    organizer_json TEXT,
    attendees_json TEXT NOT NULL DEFAULT '[]',
    current_user_attendee_json TEXT,
    rsvp_requested INTEGER NOT NULL DEFAULT 0,
    raw_ics TEXT NOT NULL,
    parse_warnings_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

Indexes:

```sql
CREATE INDEX idx_calendar_invites_message ON calendar_invites(message_id);
CREATE INDEX idx_calendar_invites_account_uid ON calendar_invites(account_id, uid);
CREATE INDEX idx_calendar_invites_time ON calendar_invites(starts_at_json);
CREATE INDEX idx_calendar_invites_method ON calendar_invites(method);
```

The exact date-time storage can be refined during implementation. If querying by time becomes important, add normalized UTC columns after resolving time zones.

## Identity

Use `UID` as the event correlation key, scoped by account. Do not use email `Message-ID` as event identity.

Use `RECURRENCE-ID` to distinguish a specific instance from the full series.

Use `SEQUENCE` and `DTSTAMP` to decide whether an invite/update/cancel is stale.

## Raw ICS

Store raw ICS text for:

- debugging
- export
- exact dry-run display
- reply construction
- future parser improvements

This is local-only mail data and should not leave the device except through explicit user export/send.

## Message Metadata Compatibility

Keep `MessageMetadata.calendar` for lightweight body rendering compatibility, but make it a summary of parsed invite data, not the source of truth.

Possible new shape:

```rust
pub struct CalendarMetadata {
    pub method: Option<String>,
    pub summary: Option<String>,
    pub uid: Option<String>,
    pub starts_at_display: Option<String>,
    pub has_actionable_rsvp: bool,
}
```

Do not stuff all invite data into `MessageMetadata`; use store tables for first-class workflows.

