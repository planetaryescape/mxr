# IMAP fills a missing From header with a synthetic address

## Where

`crates/provider-imap/src/parse.rs`, the `parsed_headers.from.unwrap_or_else(...)` fallback in the envelope builder.

## What it works around

Drafts and malformed mail can have no parseable `From`. The envelope type needs an address, so the parser invents one and logs a warning. A synthetic address corrupts sender grouping and reply-to-all.

## Smallest fix

Give `Envelope` an explicit "unknown sender" state, then drop the invented address. This is a schema change, so it needs its own migration.
