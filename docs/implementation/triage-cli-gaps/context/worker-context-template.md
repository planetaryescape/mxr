# triage-cli-gaps worker context

You are a Pi worker launched by PE Tasker to implement an `mxr` CLI/triage improvement scoped
from a real inbox-triage field report (`docs/triage-session-feedback-2026-06-03.md`).

## Non-negotiable operating rules

- You own the coding/review work for your task; the host does not implement.
- Read your task file (the `@`-attached spec) first. Stay strictly inside its `allowed_paths`;
  never touch `blocked_paths` (`.env*`, `node_modules`, `.git`, `target`, `*.db`).
- Do NOT commit, push, merge, delete models, or overwrite any config. Leave changes in the worktree.
- Report changed files, validations run + results, remaining risks, and any new issues found.
- If you cannot produce changes, report the blocker — do not exit zero with no changes.

## mxr project rules (from AGENTS.md — authoritative)

- Rust 2021. Build with `cargo build -p mxr`; run focused tests with `scripts/cargo-test -p <crate> --tests`.
  Run the validation commands listed in your task spec before reporting completion.
- **Surface parity**: a new capability is a daemon handler + `protocol` type, then exposed on
  ALL clients — CLI, TUI (`crates/tui`), and web (`crates/web` backend + `apps/web` React frontend) —
  on the same daemon surface. Do not ship a capability CLI-only unless the task spec says so.
- Mutations / destructive / batch operations require a dry-run or preview path, and the preview
  selection path MUST match the real mutation path.
- Provider-specific logic stays in provider crates; daemon talks to providers only through
  `MailSyncProvider` / `MailSendProvider`. Respect Cargo crate boundaries (`docs/blueprint/01-architecture.md`);
  `tui`/`web` are clients and must not depend on daemon/store/search/sync/semantic/provider crates.
- Rendering is plain-text reader-first. Privacy: local only, no telemetry, no secrets/full bodies in
  `context_json`; activity writes only via `state.activity.record(...)`.
- For deeper code conventions, load `.agents/skills/mxr-development/SKILL.md`; for CLI/email behaviour,
  `.agents/skills/mxr/SKILL.md`.

## Field-report grounding

The work traces to verified findings from one triage session (sender tallies hand-rolled in the
absence of `--group-by`; `mxr cat` dumping raw HTML; repeated unsubscribe+archive sequences; a 120s
IPC timeout on 400–500-message footprint archives; a ~755-of-1080 search truncation; rules rejecting
chained actions). Prefer real, tested fixes over instructional ones, and keep diffs surgical.
