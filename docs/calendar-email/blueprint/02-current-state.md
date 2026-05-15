# Current State

`mxr` has partial invite recognition. It does not have calendar invite handling as a product feature.

## What Exists

`MessageMetadata` has calendar metadata:

- [crates/core/src/types.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/core/src/types.rs:871)
- [crates/core/src/types.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/core/src/types.rs:903)

Current shape:

```rust
pub struct CalendarMetadata {
    pub method: Option<String>,
    pub summary: Option<String>,
}
```

`mail-parse` has a tiny parser:

- [crates/mail-parse/src/lib.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/mail-parse/src/lib.rs:130)

It scans lines for:

- `METHOD:`
- `SUMMARY:`

It does not unfold lines, parse parameters, parse VEVENT structure, preserve raw ICS, or validate iMIP semantics.

## Provider Extraction

Gmail:

- [crates/provider-gmail/src/parse.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/provider-gmail/src/parse.rs:285)

Gmail detects `text/calendar` and stores calendar metadata when body data is present. It treats calendar parts as attachments only if Gmail exposes an attachment id, filename, or attachment/inline disposition.

IMAP:

- [crates/provider-imap/src/parse.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/provider-imap/src/parse.rs:153)
- [crates/provider-imap/src/parse.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/provider-imap/src/parse.rs:169)

IMAP uses `mail-parser`, detects `text/calendar`, and stores metadata. Inline bare `text/calendar` is not automatically an attachment.

## Store

Calendar metadata persists through `bodies.metadata_json`:

- [crates/store/migrations/002_body_metadata.sql](/Users/bhekanik/code/planetaryescape/mxr/crates/store/migrations/002_body_metadata.sql:1)
- [crates/store/src/body.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/store/src/body.rs:73)

Attachments are stored separately:

- [crates/store/migrations/001_initial.sql](/Users/bhekanik/code/planetaryescape/mxr/crates/store/migrations/001_initial.sql:91)
- [crates/store/migrations/005_inline_attachment_metadata.sql](/Users/bhekanik/code/planetaryescape/mxr/crates/store/migrations/005_inline_attachment_metadata.sql:1)

Attachment rows store metadata, not raw bytes:

- id
- message id
- filename
- MIME type
- disposition
- content id/location
- size
- local cached path
- provider id

## Rendering

Calendar-only messages get best-effort readable text:

- [crates/core/src/types.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/core/src/types.rs:963)
- [crates/daemon/src/handler/mailbox.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/daemon/src/handler/mailbox.rs:285)

Current output can say:

- Calendar invite
- Summary
- Method
- Attachments

That is not enough to decide whether to accept.

## CLI / IPC / TUI / Web

IPC has generic body and attachment requests:

- `GetBody`
- `ListBodies`
- `DownloadAttachment`
- `OpenAttachment`

No invite-specific requests exist.

CLI has attachment list/open/download:

- [crates/daemon/src/commands/mutations/attachments.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/daemon/src/commands/mutations/attachments.rs:18)

Generic reply exists:

- [crates/daemon/src/commands/mutations/compose.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/daemon/src/commands/mutations/compose.rs:154)

TUI renders generic message attachments:

- [crates/tui/src/ui/message_view.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/tui/src/ui/message_view.rs:177)
- [crates/tui/src/ui/attachment_modal.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/tui/src/ui/attachment_modal.rs:31)

Web generated API includes `CalendarMetadata`, but app-level mailbox body types drop metadata:

- [apps/web/src/api/generated.ts](/Users/bhekanik/code/planetaryescape/mxr/apps/web/src/api/generated.ts:3181)
- [apps/web/src/features/mailbox/types.ts](/Users/bhekanik/code/planetaryescape/mxr/apps/web/src/features/mailbox/types.ts:113)

## Gaps

- No full iCalendar parser.
- No raw ICS retention guarantee.
- No calendar event/invite schema.
- No RSVP state.
- No accept/tentative/decline action.
- No dry-run RSVP preview.
- No trust/identity checks.
- No invite-specific display in CLI/TUI/web.
- No `has:calendar` search.
- No activity logging for invite response.
- No tests crossing provider parse -> store -> daemon -> CLI.

## Important Existing Advantage

The daemon-backed architecture is a good fit. Calendar invite handling can be daemon-owned and exposed to CLI, TUI, and web through one IPC contract.

