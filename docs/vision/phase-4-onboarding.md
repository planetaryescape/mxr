# Phase 4 — Onboarding & resilience

> Goal: someone non-Rust can install and be reading email in 5 minutes. Existing users don't lose drafts.

See [01-delight-plan.md §Phase 4](./01-delight-plan.md#phase-4--onboarding--resilience) for full specs.

## Tracker

### 4.1 Crash-safe drafts

**Store + daemon-startup recovery ✅**
- [x] Migration `017_draft_heartbeat.sql` adds `last_heartbeat_at` column
- [x] `crates/store/src/draft_recovery.rs` with `touch_draft_heartbeat`, `list_orphaned_sending_drafts`, `reset_orphaned_draft`
- [x] Daemon `run_startup_maintenance` scans for orphaned `'sending'` drafts (heartbeat or `status_updated_at` older than 1h) and CAS-resets them back to `'draft'` for retry
- [x] RED+GREEN: `touch_heartbeat_persists_timestamp`
- [x] RED+GREEN: `list_orphaned_excludes_drafts_in_draft_status`
- [x] RED+GREEN: `list_orphaned_includes_stale_sending_drafts`
- [x] RED+GREEN: `reset_orphaned_returns_to_draft_status`
- [x] RED+GREEN: `reset_orphaned_is_noop_for_drafts_not_in_sending`

**Operational gaps**

- [ ] Periodic heartbeat while `$EDITOR` is open (`$EDITOR`-long-lived sessions vs send-path heartbeat only)

**Shipped ✅**
- [x] `touch_draft_heartbeat` called on stored-draft send (`handler/mutations.rs` after CAS to `Sending`)
- [x] CLI `mxr drafts recover` (`ListOrphanedDrafts` IPC, `commands/mutations/compose.rs`)

### 4.2 Doctor 2.0

**Findings layer ✅**
- [x] `DoctorFinding`, `DoctorFindingCategory`, `DoctorFindingSeverity` types in protocol
- [x] `DoctorReport.findings: Vec<DoctorFinding>` (with `#[serde(default)]` for forwards-compat)
- [x] `build_doctor_findings` in `status_helpers.rs` classifies:
  - storage / database / index / socket missing
  - restart-required / repair-required
  - sync errors per-account → OAuth / network / rate-limit / sqlite-lock / generic
  - recent log scanning for `invalid_grant` / `database is locked`
- [x] Each finding carries copy-pasteable shell commands as remediation
- [x] `mxr doctor` CLI renders Findings section with severity glyphs + indented remediation

**Tests + IPC/CLI parity ✅**
- [x] Behavior tests via `handler/status_helpers.rs` unit tests covering OAuth/network/rate-limit/sqlite-lock classifications (`classify_sync_error_*`, `classify_log_line_*`)
- [x] `mxr doctor --json` uses daemon-side `build_doctor_findings` for parity (`commands/doctor.rs`, same helper as IPC `GetDoctorReport`)

### 4.3 `mxr setup` wizard

**Demo + quick-start surface ✅**
- [x] `crates/daemon/src/commands/setup.rs` with `mxr setup` (quick-start guidance) and `mxr setup --demo`
- [x] Demo path drops a `Fake` sync-provider account into the user's config and sets it as default
- [x] `--key <name>` and `--force` flags for sane re-runs
- [x] CLI prints next-step commands so the user can immediately try `mxr daemon --foreground` then `mxr` (TUI)
- [x] CLI help snapshot covers the command

**Interactive + curated demo ✅**

- [x] `inquire` setup flow — presets, Gmail path, IMAP/SMTP wizard, optional LLM (`daemon/commands/setup.rs`)
- [x] Curated fake inbox seed for `--demo` (`provider-fake` fixtures + setup wiring)

## Phase 4 acceptance

- [ ] Kill the editor mid-compose; restart daemon; `mxr drafts recover` lists the orphaned draft
- [ ] Cause an OAuth failure; `mxr doctor --json` returns finding with `mxr accounts reauth` remediation
- [ ] `mxr setup --demo` finishes in <60s with seeded inbox visible in TUI
