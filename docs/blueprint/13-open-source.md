# mxr — Open Source Strategy

## Principle

mxr is open-source-native. The repo should feel like it was built for contribution from day one, not open-sourced after the fact.

## What this means in practice

### 1. Clear crate boundaries

Contributors can reason locally. If you want to fix a search bug, you work in `crates/search/`. If you want to build an IMAP adapter, you work in `crates/providers/imap/` and only depend on `mxr-core`. You don't need to understand the whole system.

The crate dependency rules (documented in 01-architecture.md) enforce this. Provider crates depend ONLY on core. The TUI depends ONLY on core and protocol. Violations should be caught in code review.

### 2. Plain, legible code

- No clever macro-heavy abstraction towers
- No "you need to understand my architecture philosophy before fixing a bug"
- Normal if statements over ternary expressions
- Explicit error types rather than error-erasure
- Comments explain WHY, not WHAT
- Compile-time checked SQL queries via sqlx (catches schema drift at build time)

### 3. Stable extension seams

Contributors should be able to add:
- A provider adapter (implement traits from mxr-core)
- A CLI command (add to clap subcommand enum)
- An export format (add variant to ExportFormat enum)
- A rule action (add variant to RuleAction enum)
- A search field (add field to Tantivy schema)

without surgery on existing code. The architecture supports this via enums, traits, and clear interfaces.

### 4. Open defaults

Core functionality does NOT depend on:
- Proprietary APIs (Gmail is an adapter, not a requirement)
- Closed ML models
- Hosted services
- SaaS-only glue

The core app is fully valuable using open, local pieces: SQLite, Tantivy, SMTP, $EDITOR, standard Unix tools.

## Repo structure for contributors

### README.md

First thing people see. Should include:
- One-line description
- Screenshot / GIF of TUI
- Quick install (cargo install, brew, binary download)
- Quick start (add account, first sync)
- Link to architecture docs
- Link to CONTRIBUTING.md
- License

### CONTRIBUTING.md

Should cover:
- How to set up the dev environment
- How to run tests
- How to run the daemon locally
- How to use the fake provider for development
- Code style expectations
- PR process
- How to add a new provider adapter
- How to add a new CLI command
- How to add a new export format

### Issue templates

- Bug report (with required fields: steps to reproduce, expected vs actual, mxr version)
- Feature request
- Provider adapter proposal (for community adapters)

### CI (.github/workflows/)

On every PR:
- `cargo fmt --check` — formatting
- `cargo clippy -- -D warnings` — lints
- `cargo test` — all workspace tests
- `cargo build` — full build

On main merge:
- Above + binary builds for Linux/macOS
- Release tagging

### Labels

Pre-create issue labels:
- `good-first-issue` — for onboarding new contributors
- `provider:gmail` / `provider:smtp` / `provider:imap` — scope
- `crate:core` / `crate:store` / `crate:search` / etc. — crate scope
- `bug` / `feature` / `docs` / `performance`

## Provider adapter support levels

### Official (maintained in-repo by project maintainer)

- `mxr-provider-gmail` — Gmail API sync + send
- `mxr-provider-smtp` — SMTP send only
- `mxr-provider-fake` — In-memory test double

### Community (supported by interface + docs)

- IMAP
- Outlook / Microsoft Graph
- JMAP (Fastmail, etc.)
- Proton Bridge
- Exchange (ActiveSync)

### Adapter kit

For community adapter authors, the project provides:
1. `mxr-core` crate as a stable dependency
2. `MailSyncProvider` and `MailSendProvider` traits
3. `mxr-provider-fake` as a reference implementation
4. Conformance test suite (test functions adapter authors can call)
5. Fixture data (canonical test messages, threads, labels)
6. "How to build an mxr adapter" documentation

### Out-of-tree adapter support

Community adapters should be buildable as standalone crates:

```toml
# Cargo.toml of a hypothetical community IMAP adapter
[package]
name = "mxr-provider-imap"
version = "0.1.0"

[dependencies]
mxr-core = "0.1"
async-imap = "0.10"
```

This means `mxr-core` must have a stable public API. Breaking changes to the provider traits require a semver major bump with migration docs.

## Licensing

Dual MIT + Apache-2.0. This is the Rust ecosystem convention and is maximally permissive:
- MIT: simple, widely understood
- Apache-2.0: adds patent grant protection
- Dual license: users can choose whichever fits their legal requirements

All contributions are licensed under the same terms (stated in CONTRIBUTING.md).
