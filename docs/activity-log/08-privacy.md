# Phase 7 — Privacy & Retention Controls

Goal: codify the trust model. The user fully controls what's recorded, what's kept, and what's purged. Local-first is a hard invariant, documented and enforced.

This phase touches every other phase (cross-cutting). Don't skip — it's the difference between "activity log" and "creepy activity log".

## Deliverables

1. Tiered retention config in `crates/config/src/types.rs` + defaults.
2. Daily prune sweep covers each tier independently.
3. `mxr activity clear --last DURATION|all` (browser-history pattern).
4. `mxr activity pause [--for DURATION]` / `mxr activity resume`.
5. Opt-in `activity.track_link_clicks` config (default `false`).
6. `AGENTS.md` invariant: "Activity is local-only. Never synced. Never phoned home. Never transmitted."
7. Public-facing docs: user-readable explanation of what's recorded, where it lives, how to clear it.
8. CI lint: a test that fails if any code path writes activity to a non-`Store` sink (best-effort grep test).
9. PII audit: a test that asserts forbidden context keys never appear in stored rows.
10. Recorder respects an environment kill-switch (`MXR_ACTIVITY=off`) — defense-in-depth for users who want to opt out entirely without removing the code.

## Config additions

```rust
// crates/config/src/types.rs

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ActivityConfig {
    pub enabled: bool,                                  // default true; user can flip off globally
    pub retention: ActivityRetentionConfig,
    pub track_link_clicks: bool,                        // default false
    pub track_subjects: bool,                           // default true; redact when off
    pub track_recipient_handles: bool,                  // default true
    pub track_search_queries: bool,                     // default true
    pub paused: bool,                                   // managed by daemon, not editable in mxr.toml directly
    pub paused_until: Option<i64>,                      // unix ms
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ActivityRetentionConfig {
    pub ephemeral_days: u32,                            // default 30
    pub standard_days: u32,                             // default 90
    pub important_days: u32,                            // default 365
}

impl Default for ActivityConfig { /* defaults as documented */ }
impl Default for ActivityRetentionConfig {
    fn default() -> Self {
        Self { ephemeral_days: 30, standard_days: 90, important_days: 365 }
    }
}
```

`paused` / `paused_until` are *runtime* fields exposed via IPC, not edited by the user in `mxr.toml`. They're persisted to a sidecar (e.g. `~/.local/share/mxr/activity-runtime.json`) so they survive daemon restarts.

## Tier-aware prune

Implementation in `crates/daemon/src/commands/activity_prune.rs`:

```rust
pub async fn prune_activity_all(store: &Store, cfg: &ActivityRetentionConfig) -> Result<()> {
    let now = current_unix_ms();
    let day_ms = 86_400_000_i64;

    for (tier, days) in [
        (Tier::Ephemeral, cfg.ephemeral_days),
        (Tier::Standard, cfg.standard_days),
        (Tier::Important, cfg.important_days),
    ] {
        let cutoff = now - (days as i64) * day_ms;
        let deleted = store.prune_activity_before(cutoff, Some(tier)).await?;
        if deleted > 0 {
            // synthesize an `activity.pruned` marker
            recorder.record(OwnedEntry {
                ts: now,
                source: ClientKind::Daemon,
                action: "activity.pruned".into(),
                target_kind: None,
                target_id: None,
                tier: Tier::Important,
                context: Some(json!({ "tier": tier.as_str(), "cutoff": cutoff, "deleted": deleted })),
                account_id: None,
            });
        }
    }
    Ok(())
}
```

Scheduled from the daemon's existing background loop (same path that runs `prune_events()`). Runs once at startup and then every 24h.

## Track-toggle redaction

When `track_subjects = false`, the recorder strips subject fields from `context_json` *before* insert. Same for `track_recipient_handles` (strip recipients), `track_search_queries` (replace query text with `<redacted>` placeholder).

These toggles affect new writes only — historical rows are unaffected (use `mxr activity redact --filter ...` to scrub history).

## Link-click capture

`activity.track_link_clicks`:
- When `false` (default), `link.click` actions are simply dropped at the mapper.
- When `true`, URLs are recorded verbatim in `context.url`.

Recommend in user-facing docs: leave off unless you want this. URL history reveals a lot.

Phase note: `link.click` is emitted from the TUI/web clients **only** when the user explicitly clicks a link (not on every URL render). The TUI launches links via `open` / `xdg-open`; that path emits `link.click` if enabled.

## `clear` command details

```
mxr activity clear --last 1h|1d|7d|30d|all [--include-important] [--dry-run] [--yes]
```

- Tombstones (does not hard-delete) — keeps retention prune deterministic.
- By default, leaves `important`-tier rows alone (they include sends, redactions, prunes, account changes — operationally relevant).
- `--include-important` clears those too.
- `--last all` clears everything, including `important`, with confirmation.

Web equivalent: button menu in the activity page → confirmation modal with dry-run count.

## `pause` semantics

- `pause` sets `cfg.paused = true` and `cfg.paused_until = until`. Persists to runtime sidecar.
- Recorder consults flag on every record attempt.
- Recorder auto-resumes when `now >= paused_until` and emits a synthesized `activity.resumed`.
- The pause/resume markers themselves are written *regardless of paused state* — see [03-capture.md](./03-capture.md#synthesized-markers).
- `pause --for 0` is rejected.

## `AGENTS.md` invariant

Add a section:

```markdown
## Activity log invariants

The activity log records user actions. These rows are personal data. The following invariants are non-negotiable.

1. Activity rows never leave the user's device. No sync. No telemetry. No transmission.
2. Recorder failures never propagate to user-facing IPC responses.
3. `context_json` never contains credentials, tokens, password material, attachment bytes, or full mail bodies.
4. The recorder respects `MXR_ACTIVITY=off`. When set, no rows are written for the lifetime of the daemon.
5. Redaction is irreversible — tombstone rows do not retain their prior context.
6. Retention prune is irreversible. Schedule and parameters are user-configurable.
```

Add this content also to a user-facing page (e.g. `docs/guides/activity-log-privacy.md`) once Phase 7 ships.

## CI lint / structural test

Add a small test asserting:
- No code path other than `crates/daemon/src/activity/` calls into the storage `record_activity` API.
- No HTTP / websocket / network code references `user_activity` table.

Easy to enforce with a `grep`-style integration test:

```rust
#[test]
fn activity_writes_only_through_recorder() {
    let crates_glob = glob::glob("crates/**/*.rs").unwrap();
    let mut offenders = vec![];
    for entry in crates_glob.flatten() {
        let path = entry.display().to_string();
        if path.contains("/activity/") || path.contains("user_activity.rs") || path.contains("/tests/") {
            continue;
        }
        let body = std::fs::read_to_string(&entry).unwrap_or_default();
        if body.contains("record_activity(") || body.contains("user_activity") {
            offenders.push(path);
        }
    }
    assert!(offenders.is_empty(), "non-recorder code referencing activity: {offenders:?}");
}
```

## PII audit test

```rust
#[tokio::test]
async fn no_forbidden_keys_in_context() {
    let store = test_store().await;
    seed_activity_via_mapper(&store).await;
    let rows = store.list_activity(&Default::default(), 1000, None).await.unwrap();
    let forbidden = ["password", "token", "secret", "api_key", "refresh_token", "access_token"];
    for row in rows.rows {
        if let Some(ctx) = row.context_json {
            let lower = ctx.to_lowercase();
            for f in forbidden {
                assert!(!lower.contains(f), "row {} contains forbidden key '{f}': {ctx}", row.id);
            }
        }
    }
}
```

Run as part of the daemon integration test suite.

## User-facing docs

Create `docs/guides/activity-log.md`:

- What gets recorded (table of action types).
- What does **not** (cursor moves, scrolling, pane focus, polls).
- Retention defaults + how to change them.
- How to clear last hour / day / all.
- How to pause.
- How to export your data (`mxr activity export`).
- Where the data lives (`~/.local/share/mxr/mxr.db` table `user_activity`).
- Hard guarantee: never leaves the device.
- Known limitations: no encryption-at-rest in v1; rely on OS-level disk encryption.

Link from the main user README.

## Tests

- Retention prune deletes only matching tier within the cutoff (covered in Phase 1; re-test integration here).
- `clear --last 1h` tombstones rows from the last hour and leaves earlier untouched.
- `clear --last all` without `--include-important` keeps important-tier rows.
- Pause window auto-resumes; `activity.resumed` lands after the window.
- `MXR_ACTIVITY=off` env var: daemon boots, runs actions, no rows written.
- PII audit test green.
- Source-only test: no code path outside `activity/` references the table.

## Acceptance criteria

- `AGENTS.md` updated.
- User-facing guide written + linked.
- Env-var kill-switch works.
- All toggles persist across daemon restarts (runtime sidecar).
- Default retention values match this doc.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Users find retention surprising | Doc the defaults plainly. Show them in `mxr activity status`. |
| Track toggles forgotten on upgrade | Defaults sane; new fields use `serde(default)`. |
| Future code path silently logs sensitive data | PII audit test + source-only test fail the CI. |

## Exit criteria

Phase 7 is done when:
- Privacy invariants are codified in `AGENTS.md` and enforced by tests.
- Every track / pause / retention control surfaces in CLI, TUI, and web.
- User-facing privacy doc shipped.
- `STATUS.md` Phase 7 boxes ticked.
