# Activity Log — Status

Update as work lands. Leave file paths beside checkboxes when shipping.

## Phase 0 — Foundation & design
- [x] Locked decisions captured in [00-overview.md](./00-overview.md)
- [x] Action taxonomy + schema in [01-schema-and-taxonomy.md](./01-schema-and-taxonomy.md)
- [x] Context-shape appendix in [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md)

## Phase 1 — Storage
- [ ] Migration `crates/store/migrations/0NN_user_activity.sql` (use next free number)
- [ ] FTS5 mirror `crates/store/migrations/0NN_user_activity_fts.sql`
- [ ] `crates/store/src/user_activity.rs` — record/list/count/redact/prune/stats
- [ ] Wire into `crates/store/src/lib.rs`
- [ ] Unit tests
- [ ] Integration test (insert → query → prune → redact round-trip)

## Phase 2 — Capture
- [ ] `ClientKind` enum added to `crates/protocol/src/types.rs`
- [ ] Per-request mapping table `crates/daemon/src/activity/mapper.rs`
- [ ] `Recorder` module `crates/daemon/src/activity/mod.rs`
- [ ] Dispatcher wrap at `crates/daemon/src/handler/mod.rs:263-287`
- [ ] Source-tag plumbing: TUI (`crates/tui/src/ipc.rs` or equivalent)
- [ ] Source-tag plumbing: CLI (`crates/daemon/src/cli/mod.rs`)
- [ ] Source-tag plumbing: web bridge (`crates/web/src/routes_v6.rs`)
- [ ] Retention pruner extended for activity table (`crates/daemon/src/commands/logs.rs` or new module)
- [ ] Integration smoke: TUI key press → activity row visible via `mxr activity list`

## Phase 3 — Query IPC
- [ ] `Request::ListActivity / CountActivity / ActivityStats / ExportActivity / RedactActivity / PruneActivity / PauseActivity / ResumeActivity`
- [ ] `ActivityFilter` struct in `crates/protocol/src/types.rs`
- [ ] Handlers in `crates/daemon/src/handler/activity.rs`
- [ ] Dispatcher switch updated
- [ ] CLI snapshot tests for new help output

## Phase 4 — CLI
- [ ] `mxr activity` subcommand tree (`list`, `tail`, `stats`, `top`, `export`, `prune`, `redact`, `clear`, `pause`, `resume`, `replay`, `recall`)
- [ ] Alias `mxr act`
- [ ] Tab completions emitted (`mxr completions zsh|bash|fish` already exists — verify it picks up the new subcommand)
- [ ] CLI snapshot tests `crates/daemon/tests/snapshots/cli_help__cli_help_activity*.snap`
- [ ] End-to-end test: `mxr activity list --json` round-trip

## Phase 5 — TUI
- [ ] `Action::OpenActivityScreen` + handler
- [ ] Keybind `g a` (g-prefix for "go to") + palette entry "View activity"
- [ ] `ActivityScreen` component
- [ ] Filter bar (date range, source, action prefix, full-text)
- [ ] Detail drawer (`Enter` to expand)
- [ ] Jump-to-target (`o` opens referenced thread/draft)
- [ ] Redact-from-screen flow (`r` with confirmation)

## Phase 6 — Web
- [ ] Bridge routes `GET /v6/activity`, `GET /v6/activity/stats`, `GET /v6/activity/top`, `POST /v6/activity/export`, `POST /v6/activity/prune`, `POST /v6/activity/redact`, `POST /v6/activity/pause`, `POST /v6/activity/resume`
- [ ] OpenAPI schema regenerated
- [ ] React page `apps/web/src/routes/activity.tsx` (DataTable + filter sidebar + detail drawer)
- [ ] Bulk-select + bulk-redact UI
- [ ] PARITY_MATRIX entry added
- [ ] Playwright e2e smoke

## Phase 7 — Privacy
- [ ] Tiered retention config (`activity.retention.ephemeral_days`, `.standard_days`, `.important_days`) in `crates/config`
- [ ] Daily prune sweep covers all tiers separately
- [ ] `mxr activity clear --last 1h|1d|7d|all` (tombstones, doesn't hard-delete)
- [ ] `mxr activity pause [--for DURATION]` + `mxr activity resume`
- [ ] Opt-in `activity.track_link_clicks` (default `false`)
- [ ] `AGENTS.md` updated with "activity is local-only, never synced" invariant
- [ ] Docs: privacy section in user-facing README

## Phase 8 — Power features
- [ ] Saved activity filters (mirror saved-searches pattern)
- [ ] `mxr activity recall "before lunch"` heuristic
- [ ] `mxr activity replay --since 1h` (prose narrative)
- [ ] Activity dashboard in TUI (analytics screen panel) + web

## Phase 9 — Polish
- [ ] Insert latency bench < 1ms p99
- [ ] Query 10k rows < 100ms
- [ ] Compaction: collapse rapid-fire duplicates (same action+target within 250ms)
- [ ] `VACUUM` schedule documented
- [ ] `ARCHITECTURE.md` updated with the activity-log seam
- [ ] User-facing docs page

## Cross-phase invariants (verify at every phase merge)
- Activity write failures never propagate to user-facing actions
- Source enum is set correctly on every IPC request
- Heartbeats / polls / internal getters are **not** logged as activity
- No activity column ever holds credentials, OAuth tokens, password hashes, or attachment bodies
