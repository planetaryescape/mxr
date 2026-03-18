# mxr

A local-first, open-source, keyboard-native email client for terminal users, built around a daemon, a clean provider-agnostic model, and a programmable core.

## Architecture

```
┌──────────────┐     unix socket      ┌──────────────────────────────┐
│   TUI        │◄────────────────────►│         Daemon               │
│  (ratatui)   │                      │                              │
└──────────────┘                      │  ┌─────────┐ ┌───────────┐  │
                                      │  │  Sync   │ │  Rules    │  │
┌──────────────┐     unix socket      │  │ Engine  │ │  Engine   │  │
│   CLI        │◄────────────────────►│  └────┬────┘ └───────────┘  │
│  (mxr search │                      │       │                     │
│   mxr export │                      │  ┌────┴────┐ ┌───────────┐  │
│   etc.)      │                      │  │  Store  │ │  Search   │  │
└──────────────┘                      │  │ (SQLite)│ │ (Tantivy) │  │
                                      │  └─────────┘ └───────────┘  │
┌──────────────┐     unix socket      │                              │
│  Scripts /   │◄────────────────────►│  ┌──────────────────────┐   │
│  Shell hooks │                      │  │     Providers        │   │
└──────────────┘                      │  │  ┌───────┐ ┌──────┐  │   │
                                      │  │  │ Gmail │ │ SMTP │  │   │
                                      │  │  └───────┘ └──────┘  │   │
                                      │  └──────────────────────┘   │
                                      └──────────────────────────────┘
```

## Status

**Phase 0**: Proving the architecture. Daemon runs, TUI connects, fake data flows end-to-end through SQLite and Tantivy.

## Build

```bash
cargo build --workspace
```

## Run

```bash
# Start daemon in foreground (with fake test data)
cargo run -- daemon --foreground

# In another terminal, start TUI
cargo run
```

## Stack

| Component | Technology |
|---|---|
| Language | Rust |
| Async runtime | Tokio |
| Database | SQLite (sqlx) |
| Search engine | Tantivy |
| TUI framework | Ratatui + crossterm |
| IPC | JSON over Unix socket |

## License

MIT OR Apache-2.0

See [docs/blueprint/](docs/blueprint/) for the full technical blueprint.
