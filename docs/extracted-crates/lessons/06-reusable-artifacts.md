# Reusable artifacts

Verbatim copy-paste templates for the next crate. Tested in production by `mail-threading` v0.1.0.

The reference repo is [`planetaryescape/mail-threading`](https://github.com/planetaryescape/mail-threading). When in doubt, look at the actual file there — it stays current.

## Standalone Cargo.toml template

Replace `<CRATE>`, `<DESCRIPTION>`, `<KEYWORDS>`, `<CATEGORIES>`, and `<MSRV>`. Verify categories are valid slugs (see [`01-preflight-checks.md`](./01-preflight-checks.md#categories-must-be-valid-cratesio-slugs)).

```toml
[package]
name = "<CRATE>"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
rust-version = "<MSRV>"
repository = "https://github.com/planetaryescape/<CRATE>"
homepage = "https://github.com/planetaryescape/<CRATE>"
documentation = "https://docs.rs/<CRATE>"
description = "<DESCRIPTION>"
readme = "README.md"
keywords = [<KEYWORDS>]
categories = [<CATEGORIES>]
include = [
    "Cargo.toml",
    "README.md",
    "src/**",
    "testdata/**",
    "tests/**",
    "LICENSE-MIT",
    "LICENSE-APACHE",
]

[lints.rust]
unsafe_code = "deny"
unused_must_use = "deny"

[lints.clippy]
unwrap_used = "warn"
panic = "warn"
todo = "warn"

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]

[features]
default = []
# serde = ["dep:serde"]   # uncomment if needed

[dependencies]
# add explicit version specs — no { workspace = true } in standalone

[dev-dependencies]
```

Notes:

- `version = "0.1.0"` not `version.workspace = true`
- `edition = "2021"` is explicit, not inherited
- Every dep gets a real version constraint, not `{ workspace = true }`
- `include` is an allowlist; verify with `cargo package --list`

## clippy.toml

One line:

```toml
msrv = "<MSRV>"
```

Must match `package.rust-version` in `Cargo.toml`. Drift between them is a bug.

## .gitignore

```text
/target
```

That's it. `Cargo.lock` is committed for libraries (see [`02-cargo-and-workspace-mechanics.md`](./02-cargo-and-workspace-mechanics.md#cargolock-for-libraries-commit-it)).

## .github/workflows/ci.yml

Full file:

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt -- --check
      - run: cargo test --all-features
      - run: cargo test --all-features --doc
      - run: cargo clippy --all-targets --all-features --locked -- -D warnings
      - run: cargo publish --dry-run
```

Zero edits between crates. Bump `actions/checkout` when v5 ships.

## .github/workflows/publish.yml

Full file:

```yaml
name: Publish

on:
  push:
    tags:
      - "v*.*.*"
  workflow_dispatch:

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt -- --check
      - run: cargo test --all-features
      - run: cargo test --all-features --doc
      - run: cargo clippy --all-targets --all-features --locked -- -D warnings
      - run: cargo publish --dry-run
      - name: Publish to crates.io
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo publish
```

Zero edits between crates. `CARGO_REGISTRY_TOKEN` must exist as a repo secret (see [`04-ci-and-publish-pattern.md`](./04-ci-and-publish-pattern.md#secret-setup-walkthrough)).

## License files

Copy verbatim from `mxr/`:

```bash
cp ../mxr/LICENSE-MIT .
cp ../mxr/LICENSE-APACHE .
```

`mail-threading` uses the same MIT-OR-Apache-2.0 dual license as `mxr`. Don't change without a deliberate decision — the licenses are the reason "use it freely" is the answer for anyone reading.

## Testdata schema (when shipping a conformance corpus)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Conformance fixture",
  "type": "object",
  "required": ["name", "description", "spec", "input", "expected"],
  "properties": {
    "name": { "type": "string" },
    "description": { "type": "string" },
    "spec": {
      "type": "object",
      "required": ["source", "behavior"],
      "properties": {
        "source": { "type": "string" },
        "url": { "type": "string", "format": "uri" },
        "behavior": { "type": "string" }
      }
    },
    "options": { "type": "object" },
    "input": {},
    "expected": {}
  }
}
```

`input` and `expected` are crate-specific. The metadata fields (`name`, `description`, `spec.*`) are universal.

## Conformance test enforcement skeleton

```rust
// tests/conformance.rs

#[test]
fn coverage_matrix_mentions_every_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = std::fs::read_to_string("testdata/coverage.md")?;
    for fixture in fixture_names()? {
        assert!(
            matrix.contains(&format!("`{fixture}`")),
            "coverage matrix does not mention fixture {fixture}"
        );
    }
    Ok(())
}

#[test]
fn conformance_corpus_contains_required_behavior_fixtures() {
    let required: &[&str] = &[
        // list every fixture the public contract depends on
    ];
    for name in required {
        assert!(
            std::path::Path::new(&format!("testdata/conformance/{name}.json")).exists(),
            "missing required fixture: {name}"
        );
    }
}

#[test]
fn conformance_fixtures_match_expected_threads() {
    // load each JSON, run the crate, assert output == expected
}
```

The first two tests are about coverage *integrity*; the third is about the algorithm. Both are necessary — fixtures without coverage docs is the failure mode where a crate silently shrinks its contract.

## Sibling-directory layout for the migration

```text
~/code/planetaryescape/
  mxr/                    ← original
  <crate>/                ← clone of the new standalone repo
```

Work happens in the sibling dir during Phases 2–5. The migration plan template assumes this layout (`cp ../mxr/LICENSE-MIT .`).

## Commit message templates

```text
feat: prepare <crate> for external publish

<one-line summary of polish landed in-tree>
```

```text
chore: make crate standalone

Replace workspace inheritance with explicit metadata, add MIT+Apache
licenses, MSRV-pinned clippy config, gitignore, and a CI workflow that
runs fmt, tests, doctests, clippy, and cargo publish --dry-run.
```

```text
ci: add publish workflow

Runs on tag push (v*.*.*) or manual dispatch. Validates fmt/tests/clippy
and dry-run before publishing with CARGO_REGISTRY_TOKEN from secrets.
```

```text
chore: consume <crate> from crates.io

Switch the workspace dependency from the path-based in-tree crate to
the published <crate> = "X.Y.Z" on crates.io, remove
crates/<crate> from workspace members, and delete the in-tree
copy. The crate now lives at planetaryescape/<crate>.
```

```text
docs: mark <crate> extraction complete

Update extractable-crates table and per-candidate doc to show the crate
shipped to crates.io, add status banner to the in-repo implementation
plan, mark the external-repo migration plan complete, and tick its
final checklist.
```

Per project convention: no Claude Code attribution, no Co-Authored-By trailers.
