# Conformance corpus

JSON test fixtures that pin the public contract of `list-unsubscribe`.

The format is deliberately language-neutral so a future TypeScript port,
or any other implementation, can adopt the corpus without reverse-engineering
Rust tests. See [`schema.json`](./schema.json) for the structure.

Each fixture file has the shape:

```json
{
  "name": "rfc2369-mailto-only",
  "description": "Single mailto URI in List-Unsubscribe, no Post header.",
  "spec": {
    "source": "RFC 2369",
    "url": "https://www.rfc-editor.org/rfc/rfc2369#section-3.2",
    "behavior": "..."
  },
  "input": {
    "list_unsubscribe": "<mailto:u@example.com>",
    "list_unsubscribe_post": null
  },
  "expected": {
    "kind": "Mailto",
    "address": "u@example.com",
    "subject": null
  }
}
```

`expected.kind` is one of `OneClick`, `HttpLink`, `Mailto`, or `None`. The
remaining fields depend on the variant — `url` for the HTTP variants,
`address` and `subject` for `Mailto`.

The mapping from each fixture to the behavior it pins lives in
[`coverage.md`](./coverage.md). The test suite enforces three things:

1. Every fixture file is referenced in `coverage.md`.
2. Every contract-critical fixture name in
   [`tests/conformance.rs`](../tests/conformance.rs) has a corresponding
   JSON file.
3. The actual parser output for each fixture matches `expected`.

## Adding a fixture

1. Write the JSON under `conformance/<descriptive-name>.json`.
2. Add a row to `coverage.md`.
3. If the fixture pins a contract-level guarantee (not just an edge
   case), also add its name to the `required` array in
   `tests/conformance.rs`.
4. Run `cargo test -p list-unsubscribe --all-features --tests`.
