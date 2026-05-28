---
title: Public Rust crates
description: Use the standalone email crates that mxr consumes.
---

mxr is shipped as one binary. Most workspace crates stay private on purpose.

Four smaller email contracts are public Rust crates because they are useful outside mxr and have their own conformance story.

## Check what mxr consumes

```bash
cargo tree -p mxr-search -i mail-query
cargo tree -p mxr-sync -i mail-threading
cargo tree -p mxr-mail-parse -i list-unsubscribe
cargo tree -p mxr-export -i mailbox-formats
```

What you get: the exact registry crate version currently wired into this mxr checkout.

| Crate | Contract | mxr consumer |
|---|---|---|
| [`mail-query`](https://crates.io/crates/mail-query) | Gmail-style search parser and typed AST | `mxr-search` |
| [`mail-threading`](https://crates.io/crates/mail-threading) | RFC 5256 / JWZ client-side threading | `mxr-sync` |
| [`list-unsubscribe`](https://crates.io/crates/list-unsubscribe) | RFC 2369 / RFC 8058 unsubscribe header parsing | `mxr-mail-parse` |
| [`mailbox-formats`](https://crates.io/crates/mailbox-formats) | mbox variants and Maildir reader/writer | `mxr-export` |

## Use one directly

Add the package you need:

```bash
cargo add mail-query
cargo add mail-threading
cargo add list-unsubscribe
cargo add mailbox-formats
```

What you get: the standalone package, not the mxr daemon, store, or provider model.

## Know the boundary

The crate owns the portable contract. mxr owns local execution policy.

`mail-query`, for example, parses Gmail-style search syntax into an AST. mxr then maps that AST onto SQLite, Tantivy, and semantic search. Parser support does not always mean Gmail-identical execution over local data.

Inspect the two sides separately:

```bash
cargo info mail-query
rg -n "mail_query|register_filter|QueryNode" crates/search
```

What you get: package metadata from crates.io, then the mxr-specific execution layer that consumes it.

## See also

- [Conformance tests](/reference/conformance/)
- [Search workflow](/guides/search/)
- [CLI concepts](/reference/cli/concepts/)
