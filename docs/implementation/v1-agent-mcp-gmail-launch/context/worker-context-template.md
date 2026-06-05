# Worker Context

You are working in the mxr repo. Follow repo `AGENTS.md` and `.agents/skills/mxr-development/SKILL.md`.

Core invariants:

- Local-first. No telemetry. No secrets/full bodies in activity `context_json`.
- New capabilities must land on daemon IPC plus CLI JSON/JSONL where applicable.
- Provider-specific logic stays inside provider crates.
- Destructive actions and batch mutations require preview/dry-run. The preview path must match the mutation path.
- TUI/web are clients over daemon surfaces; do not bypass the daemon.
- Use battle-tested libraries when they exist. For MCP, prefer official `rmcp`.
- Keep diffs scoped. Do not revert unrelated dirty worktree changes.
