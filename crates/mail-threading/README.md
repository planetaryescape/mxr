# mail-threading

`mail-threading` reconstructs email conversation threads from parsed RFC 5322
message metadata. It is a small Rust library for clients, archives, migration
tools, and support systems that need local threading when a provider does not
offer trustworthy native thread IDs.

The crate implements the RFC 5256 `THREAD=REFERENCES` shape of threading and
follows Jamie Zawinski's original algorithm lineage for practical client-side
thread reconstruction.

- RFC 5256: <https://www.rfc-editor.org/rfc/rfc5256>
- JWZ threading: <https://www.jwz.org/doc/threading.html>
- RFC 5322 identification fields: <https://www.rfc-editor.org/rfc/rfc5322>

## Why this exists

Rust email tooling has strong parsers and SMTP/IMAP clients, but no maintained
focused crate for client-side RFC 5256/JWZ threading. JavaScript has the same
gap: the obvious email-threading packages are stale or unrelated to email
threading. This crate is published from `mxr`, which already needs this
algorithm for providers without native thread IDs.

## Scope

`mail-threading` expects callers to pass parsed fields:

- `Message-ID`
- `In-Reply-To`
- ordered `References`
- message date
- subject

It does not parse raw email messages. Use a parser such as `mail-parser` before
calling this crate.

Provider-native thread IDs should still win when they are available and
trustworthy. This crate is for local reconstruction and fallback behavior.

## Example

```rust
use chrono::{TimeZone, Utc};
use mail_threading::{thread_messages, Message};

let messages = vec![
    Message {
        id: "root".to_string(),
        message_id: Some("<root@example>".to_string()),
        in_reply_to: None,
        references: vec![],
        date: Utc.with_ymd_and_hms(2026, 5, 15, 9, 0, 0).unwrap(),
        subject: "Hello".to_string(),
    },
    Message {
        id: "reply".to_string(),
        message_id: Some("<reply@example>".to_string()),
        in_reply_to: Some("<root@example>".to_string()),
        references: vec!["<root@example>".to_string()],
        date: Utc.with_ymd_and_hms(2026, 5, 15, 9, 5, 0).unwrap(),
        subject: "Re: Hello".to_string(),
    },
];

let threads = thread_messages(&messages);
assert_eq!(threads.len(), 1);
assert_eq!(threads[0].root_message_id, "root");
assert_eq!(threads[0].messages, vec!["root", "reply"]);
```

## Missing ancestors

Referenced messages are often outside the local mailbox or current query
window. The algorithm creates phantom containers for those missing ancestors.
By default phantoms are pruned from public output, so the first visible message
becomes the public thread root.

```rust
use chrono::{TimeZone, Utc};
use mail_threading::{thread_messages, Message};

let messages = vec![Message {
    id: "reply".to_string(),
    message_id: Some("<reply@example>".to_string()),
    in_reply_to: Some("<root@example>".to_string()),
    references: vec!["<root@example>".to_string()],
    date: Utc.with_ymd_and_hms(2026, 5, 15, 9, 5, 0).unwrap(),
    subject: "Re: Hello".to_string(),
}];

let threads = thread_messages(&messages);
assert_eq!(threads[0].root_message_id, "reply");
assert_eq!(threads[0].messages, vec!["reply"]);
```

## Subject fallback

Some clients strip `References` and `In-Reply-To`. With default options,
headerless messages with the same normalized subject are merged. This is a
practical fallback, not a guarantee: subject-only threading can over-merge
unrelated conversations.

```rust
use chrono::{TimeZone, Utc};
use mail_threading::{thread_messages_with, Message, ThreadingOptions};

let messages = vec![
    Message {
        id: "a".to_string(),
        message_id: Some("<a@example>".to_string()),
        in_reply_to: None,
        references: vec![],
        date: Utc.with_ymd_and_hms(2026, 5, 15, 9, 0, 0).unwrap(),
        subject: "Lunch".to_string(),
    },
    Message {
        id: "b".to_string(),
        message_id: Some("<b@example>".to_string()),
        in_reply_to: None,
        references: vec![],
        date: Utc.with_ymd_and_hms(2026, 5, 15, 9, 5, 0).unwrap(),
        subject: "Re: Lunch".to_string(),
    },
];

let merged = thread_messages_with(&messages, &ThreadingOptions::default());
assert_eq!(merged.len(), 1);

let strict = thread_messages_with(
    &messages,
    &ThreadingOptions {
        subject_merge: false,
        ..ThreadingOptions::default()
    },
);
assert_eq!(strict.len(), 2);
```

## Message identity and duplicate Message-ID policy

Every input message has a caller-stable `id` and an optional RFC 5322
`Message-ID`. Thread output returns caller IDs. This lets the crate represent
messages with missing or duplicate `Message-ID` headers.

RFC 5256 says the first message with a duplicated `Message-ID` keeps that ID
for threading, while later duplicates are assigned unique IDs. `mail-threading`
does the same internally; duplicate messages remain visible in output under
their caller IDs.

## Conformance

The conformance suite lives in `testdata/conformance` inside this crate and is
included in the published package. Fixtures are JSON so they can be reused by
the future TypeScript package. A behavior change starts as a fixture change,
then both implementations must pass the same corpus.

The RFC coverage matrix lives in `testdata/rfc5256-coverage.md`. It maps each
covered RFC 5256 behavior to fixture IDs and calls out partial, out-of-scope,
and intentionally divergent behavior.

Passing the corpus means the implementation matches the covered RFC 5256/JWZ
behaviors. It does not claim to test raw email parsing, provider APIs, or every
possible malformed input.

## Complexity

Thread construction is hash-map based and intended to be linear in the number
of messages plus references, aside from deterministic sorting of output
threads and members.

## Feature flags

- `serde`: derives `Serialize` and `Deserialize` for public input and output
  types.

## Minimum Supported Rust Version

The current MSRV is Rust 1.88.

## Versioning

The crate follows semantic versioning. Public API changes, MSRV bumps, default
option changes, and conformance-output changes are semver-significant.
