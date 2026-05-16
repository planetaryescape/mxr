# mail-threading

`mail-threading` reconstructs email conversation threads from parsed RFC 5322
message metadata. It is for mail clients, archives, importers, support tools,
and mailbox analysis jobs that need local threading when a provider does not
offer trustworthy native thread IDs.

The crate implements the client-side `THREAD=REFERENCES` behavior from RFC
5256 and follows Jamie Zawinski's threading algorithm lineage. It returns flat
thread membership for application code, not an IMAP wire-protocol `THREAD`
response.

- RFC 5256: <https://www.rfc-editor.org/rfc/rfc5256>
- JWZ threading: <https://www.jwz.org/doc/threading.html>
- RFC 5322 identification fields: <https://www.rfc-editor.org/rfc/rfc5322>

## Install

```toml
[dependencies]
mail-threading = "0.1"
```

Enable `serde` when public input/output types need to cross a JSON, IPC, or
storage boundary:

```toml
[dependencies]
mail-threading = { version = "0.1", features = ["serde"] }
```

## Why this exists

Rust has mature email parsers and transport clients, but it has not had a
small maintained crate focused on local RFC 5256/JWZ thread reconstruction.
Projects that need this behavior have typically had to embed an app-specific
implementation.

`mail-threading` is extracted from `mxr`, where local threading is required for
providers without native thread IDs. The shared JSON conformance corpus is
part of the package so other implementations, including a future TypeScript
port, can test against the same behavior.

## Scope

Callers pass already-parsed message fields:

- caller-stable message ID
- optional RFC 5322 `Message-ID`
- optional `In-Reply-To`
- ordered `References`
- message date
- subject

The crate does not parse raw email, MIME, encoded words, IMAP commands, or
provider API payloads. Use a parser such as `mail-parser` first, then pass the
decoded metadata here.

Provider-native thread IDs should still win when they are available and
trustworthy. This crate is for local reconstruction and fallback behavior.

## Quick example

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

## API

Most callers need one of these two functions:

- `thread_messages(&[Message]) -> Vec<Thread>`
- `thread_messages_with(&[Message], &ThreadingOptions) -> Vec<Thread>`

The first uses defaults. The second lets you configure subject fallback,
phantom pruning, and recognized subject prefixes.

Output uses `Message::id`. The RFC `Message-ID` header is used for ancestry
matching, but it is not treated as the application's only message identity.
That matters in real mailboxes, where `Message-ID` can be missing or
duplicated.

## Flexibility

The crate gives you the main policy knobs without making you fork it:

- Set `subject_merge: false` if you only want header-based threading.
- Leave `prune_phantoms: true` if you want compact public threads.
- Set `prune_phantoms: false` if you need to surface missing ancestors.
- Add or replace `subject_prefixes` if your mailflow uses different reply
  markers.

If you want something more opinionated than the default behavior, build on top
of the returned threads. The output is flat on purpose, so you can keep using
this crate for the hard part and then group, label, or reshape the threads in
your own application code.

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

Set `ThreadingOptions::prune_phantoms` to `false` when you need to inspect
missing ancestors explicitly.

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

Default subject normalization handles common RFC 5256 artifacts such as `Re:`,
`Fwd:`, `[Fwd: ...]`, `(fwd)`, subject blobs/list tags, repeated whitespace,
and several localized reply prefixes. Custom prefixes can be supplied through
`ThreadingOptions::subject_prefixes`.

## In real life

### Thread a mailbox import

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
```

What you get: a flat list of conversation threads you can store, index, or
group in a UI.

### Keep fallback behavior but turn off subject merging

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
let threads = thread_messages_with(
    &messages,
    &ThreadingOptions {
        subject_merge: false,
        ..ThreadingOptions::default()
    },
);
```

What you get: threads built only from `Message-ID`, `References`, and
`In-Reply-To`, without subject-only merges.

### Check the published corpus

```bash
cargo test -p mail-threading --all-features --tests
```

What you get: the crate's conformance suite, including the shared JSON corpus
and RFC coverage matrix checks.

## Message-ID policy

Every input message has a caller-stable `id` and an optional RFC 5322
`Message-ID`. Thread output returns caller IDs.

RFC 5256 says the first message with a duplicated `Message-ID` keeps that ID
for threading, while later duplicates are assigned unique IDs. `mail-threading`
does the same internally; duplicate messages remain visible in output under
their caller IDs.

Missing or invalid `Message-ID` headers are assigned internal synthetic
identities. They can still participate in subject fallback and can still appear
as normal output messages.

## Conformance

The conformance suite lives in `testdata/conformance` and ships with the
crate. Fixtures are JSON, so another implementation can run the same cases
without translating them first.

The RFC coverage matrix lives in `testdata/rfc5256-coverage.md`. It maps
covered RFC 5256 behavior to fixture IDs and labels partial, out-of-scope, and
intentionally divergent behavior.

Passing the corpus means the implementation matches the covered RFC 5256/JWZ
behaviors. It does not claim to test raw email parsing, provider APIs, IMAP
server behavior, or every possible malformed input.

Some choices are deliberate. The crate keeps flat output instead of IMAP
response trees, and it avoids forcing same-subject header-backed roots to merge
when that would create false positives. The exact cases are documented in the
coverage matrix.

## Non-goals

`mail-threading` does not implement:

- IMAP `SORT`
- IMAP command parsing
- IMAP `THREAD=ORDEREDSUBJECT`
- nested IMAP `THREAD` response formatting
- raw RFC 5322 or MIME parsing
- RFC 2047 encoded-word decoding
- full `i;unicode-casemap` collation

Those belong in surrounding parser, IMAP, or application layers.

## Complexity

Thread construction is hash-map based and intended to be linear in the number
of messages plus references, aside from deterministic sorting of output
threads and members.

## Feature flags

- `serde`: derives `Serialize` and `Deserialize` for public input and output
  types.

## Minimum supported Rust version

The current MSRV is Rust 1.88.

## Versioning

The crate follows semantic versioning. Public API changes, MSRV bumps, default
option changes, and conformance-output changes are semver-significant.

## See also

- RFC 5256: <https://www.rfc-editor.org/rfc/rfc5256>
- JWZ threading: <https://www.jwz.org/doc/threading.html>
- RFC 5322 identification fields: <https://www.rfc-editor.org/rfc/rfc5322>
- RFC coverage matrix: `testdata/rfc5256-coverage.md`
