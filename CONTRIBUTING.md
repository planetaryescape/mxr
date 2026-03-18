# Contributing to mxr

Thank you for your interest in contributing to mxr. This document covers everything you need to get started.

## Core Principles

These are non-negotiable. They guide every design decision. If a feature or implementation conflicts with these, the feature loses.

### 1. Local-first

Your email lives on your machine. SQLite is the canonical state store. The search index is rebuildable from SQLite. mxr works offline. Cloud services are optional transports, not requirements.

### 2. Provider-agnostic internal model

All application logic speaks one language: the mxr internal model. Gmail labels, IMAP folders, and flags all normalize into this model. No provider-specific concepts leak into core code. If a provider disappears, only its adapter crate needs rewriting.

### 3. Daemon-backed architecture

The daemon is the system. The TUI is a client. The CLI is a client. Scripts are clients. This separation means background sync, indexing, and rule execution happen regardless of whether the TUI is open.

### 4. $EDITOR for writing

mxr does not compete with your text editor. Compose opens $EDITOR with a markdown file. YAML frontmatter carries metadata. The daemon handles the rest.

### 5. Fast search is a first-class feature

Search is not an afterthought bolted onto a folder browser. Tantivy provides BM25 ranking, field-level boosts, and sub-second results across large mailboxes. Every email is indexed at sync time.

### 6. Saved searches are a core primitive

Saved searches are user-programmed inbox lenses. They live in the sidebar, appear in the command palette, and are the primary way users organize their view of email.

### 7. Rules engine is deterministic first

Rules are data, not scripts. They are inspectable, replayable, idempotent, and dry-runnable. "Show me what this rule would do" must work before "run this rule."

### 8. Shell hooks over premature plugin systems

Don't build a plugin framework. Pipe data to shell commands. Let users write automation in whatever language they want. Unix composition over framework lock-in.

### 9. Adapters are swappable

No provider-specific logic outside adapter crates. Ever. The adapter interface is the contract.

### 10. Correctness beats cleverness

No clever macro towers. No "you need to understand my architecture philosophy before fixing a bug." Plain, legible Rust code. Compile-time checked SQL queries. Explicit error types. When in doubt, be boring.

## Non-negotiables Checklist

Before submitting a PR, verify:

- [ ] Local-first by default
- [ ] SQLite is the canonical state store
- [ ] Search index is rebuildable from SQLite
- [ ] Provider adapters are replaceable
- [ ] No provider-specific logic outside adapter crates
- [ ] Compose uses $EDITOR
- [ ] Core features do not depend on proprietary services
- [ ] Rules are deterministic before they are intelligent
- [ ] TUI is a client of the daemon, not the system itself
- [ ] Distraction-free rendering: plain text first, reader mode, no inline images

## Crate Dependency Rules

These are strict. Violations should be caught in code review:

1. **`core` depends on nothing internal.** It is the leaf node. All other crates depend on it.
2. **`protocol` depends only on `core`.** It defines the IPC contract between daemon and clients.
3. **Provider crates depend only on `core`.** They implement traits defined in core. They do NOT depend on store, search, or sync.
4. **`store` and `search` depend only on `core`.** They are storage backends, not business logic.
5. **`sync` depends on `core`, `store`, `search`.** It orchestrates data flow between providers and local state.
6. **`daemon` is the integration point.** It depends on most crates. This is expected and acceptable.
7. **`tui` depends only on `core` and `protocol`.** It talks to the daemon via IPC, never directly to providers, store, or search.

## Development Setup

### Prerequisites

- Rust stable toolchain (see `rust-toolchain.toml`)
- Git

### Build

```bash
cargo build --workspace
```

### Run

```bash
# Start daemon in foreground (uses fake provider with test data)
cargo run -- daemon --foreground

# In another terminal, start TUI
cargo run
```

### Test

```bash
cargo test --workspace
```

### Lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Code Style

- **Edition**: 2021
- **Error handling**: `thiserror` for typed errors in library crates, `anyhow` for application-level (daemon, CLI)
- **Async**: tokio runtime, `async-trait` for trait definitions
- **SQL**: sqlx with compile-time checked queries where possible
- **Logging**: `tracing` crate with structured spans
- **Testing**: Unit tests in modules, integration tests in `tests/` directory. Use in-memory SQLite and in-memory Tantivy for test isolation.

## How to Add a Feature

1. Check the [decision log](docs/blueprint/15-decision-log.md) — the decision may already be settled
2. Check the [blueprint](docs/blueprint/) — the feature may already be specified
3. Open an issue describing the feature and how it fits with the core principles
4. Implement with tests
5. Submit a PR

## How to Build an Adapter

See the [adapter development guide](docs/blueprint/03-providers.md) and the adapter kit documentation.

Quick summary:
1. Create a new crate that depends only on `mxr-core`
2. Implement `MailSyncProvider` and/or `MailSendProvider` traits
3. Run the conformance test suite
4. See `crates/provider-fake/` as a reference implementation

## PR Guidelines

- Keep PRs focused. One feature or fix per PR.
- Include tests for new functionality
- Run `cargo fmt` and `cargo clippy` before submitting
- Update documentation if your change affects user-facing behavior
- Reference related issues

## Architecture Reference

See [docs/blueprint/](docs/blueprint/) for the complete design specification and [docs/implementation/](docs/implementation/) for phased implementation plans.

## License

By contributing, you agree that your contributions will be licensed under the MIT OR Apache-2.0 dual license.
