# Conformance corpus

JSON test fixtures that pin the public contract of `mail-query`.

The format is language-neutral so a future TypeScript port (or any other
implementation) can adopt it unchanged. See [`schema.json`](./schema.json)
for the structure.

Each fixture has the shape:

```json
{
  "name": "filter-custom-via-options",
  "description": "...",
  "spec": { "source": "...", "url": "...", "behavior": "..." },
  "options": {
    "custom_filters": ["owed-reply"]
  },
  "input": "is:owed-reply",
  "expected_ast": { "Filter": { "custom": "owed-reply" } }
}
```

`expected_ast` is the serde-default (externally tagged) form of
`QueryNode`. Fixtures that test error paths use `expected_error` instead,
naming the `ParseError` variant.

The mapping from each fixture to the behaviour it pins lives in
[`coverage.md`](./coverage.md). The test suite enforces three things:

1. Every fixture file is referenced in `coverage.md`.
2. Every contract-critical fixture name (in
   [`tests/conformance.rs`](../tests/conformance.rs)'s `REQUIRED_FIXTURES`)
   has a corresponding JSON file.
3. The actual parser output matches `expected_ast` (or
   `expected_error`) for every fixture.

## Adding a fixture

1. Write the JSON under `conformance/<descriptive-name>.json`.
2. Add a row to `coverage.md`.
3. If the fixture pins a contract-level guarantee (not just an edge
   case), add its name to `REQUIRED_FIXTURES` in
   `tests/conformance.rs`.
4. Run `cargo test --all-features --tests`.
