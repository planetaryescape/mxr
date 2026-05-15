# Libraries

Do not implement iCalendar parsing by hand. Calendar syntax has folded lines, property parameters, recurrence, time zones, escaping, and update semantics.

## Candidates

| Library | Role | Strength | Concern | Fit |
|---|---|---|---|---|
| `calcard` | Parse/build iCalendar and related formats | Rust-native, Stalwart ecosystem, mail-adjacent | Need local spike for API fit and maturity | Best first parser candidate |
| `icalendar` | Build and parse RFC 5545 | Older, known Rust crate, builder API, recurrence support | Docs invite help to make it more mature; not an iTIP engine | Good candidate, especially for generation |
| `ical` | Parse iCalendar/vCard | Simple parser | Parse-only, lower-level | Secondary |
| `ics` | Generate iCalendar | Good for writing `.ics` | Generation-focused, not robust parsing | Secondary |
| `rrule` | Recurrence rules | Focused RRULE support | Does not solve full invite parsing | Later recurrence expansion |
| `libdav` | CalDAV/CardDAV client | Could support future CalDAV | Not needed for email RSVP | Future only |
| `libical` | Mature C implementation | Battle-tested, broad support | C/FFI/deployment/license complexity | Consider only if Rust crates fail |
| `ical.js` | JS iCalendar library | Thunderbird ecosystem | Not Rust; web only | Reference, not core |

## Implementation Decision

The implementation uses `icalendar` for production parsing and constrained
`METHOD:REPLY` generation.

`calcard` was the preferred first spike candidate, but `icalendar` fit the
current slice better: it parses unfolded RFC 5545 calendars through a maintained
Rust crate, exposes raw property names/values/parameters needed by mxr's
provider-agnostic model, and keeps reply generation in Rust without adding a C
or JS runtime dependency.

This is not a full iTIP engine. mxr still owns scheduling policy: stale
sequence checks, organizer-change warnings, attendee matching, dry-run previews,
and refusal to send replies for malformed or unsupported invites.

## Parser Acceptance Criteria

The selected parser path must continue to satisfy:

- Parses `VCALENDAR` with one or more `VEVENT`.
- Exposes raw properties and params if typed fields are incomplete.
- Handles folded content lines.
- Preserves unknown properties.
- Exposes `UID`, `SEQUENCE`, `RECURRENCE-ID`, `DTSTAMP`.
- Exposes `ORGANIZER` and `ATTENDEE` params such as `PARTSTAT`, `RSVP`, `ROLE`, `CN`.
- Handles `TZID` and `VTIMEZONE` enough to display local time safely or mark unresolved.
- Can serialize/generate a valid `METHOD:REPLY`, or can be paired with `icalendar`.
- Does not pull provider-specific or heavy runtime dependencies into low-level crates.

Reply generation is intentionally constrained to `METHOD:REPLY` and backed by
tests. Wider calendar publishing, free/busy, CalDAV, recurrence expansion, and
time-zone resolution remain separate future work.

## Sources

- `calcard`: <https://docs.rs/calcard/latest/calcard/icalendar/>
- `icalendar`: <https://docs.rs/icalendar/>
- `ical`: <https://docs.rs/ical/latest/ical/>
- `ics`: <https://docs.rs/ics/latest/ics/>
- `rrule`: <https://docs.rs/crate/rrule/latest>
- `libical`: <https://libical.github.io/>
- `libdav`: <https://docs.rs/libdav/latest/libdav/>
