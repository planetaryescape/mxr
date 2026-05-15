# mail-threading conformance corpus

This directory contains implementation-neutral JSON fixtures for
`mail-threading`.

Passing this corpus means an implementation matches the behavior covered by
the fixtures: RFC 5256/JWZ references threading, Message-ID normalization,
missing and duplicate Message-ID handling, phantom ancestor handling, cycle
prevention, deterministic ordering, and configurable RFC 5256 subject fallback.
It does not mean the implementation parses raw RFC 5322 messages; callers are
expected to pass parsed `Message-ID`, `References`, `In-Reply-To`, date, and
subject fields.

The same files are used by the Rust crate and the future TypeScript package.
Behavior changes start as fixture changes so the two implementations cannot
silently drift.

`rfc5256-coverage.md` maps RFC 5256 sections and REFERENCES algorithm steps to
fixture IDs. The Rust conformance test fails if a JSON fixture is not mentioned
there.

Fixture files live in `conformance/*.json`. Name files with lower-case
kebab-case, and include:

- `name`: stable fixture id.
- `description`: human-readable behavior.
- `spec`: the source, URL, and behavior being exercised.
- `options`: optional threading options; omitted fields use defaults.
- `input`: messages in deliberately chosen order. `id` is caller-stable
  identity; `message_id` is the optional RFC header value.
- `expected`: flat threads expected from public API output, expressed with
  caller IDs except when a fixture intentionally exposes an unpruned phantom
  root.

Use literal expected outputs derived from the cited behavior, not values copied
from any implementation.
