# Build log — v1-agent-mcp-gmail-launch

## 2026-06-04 — Scoping from v1 readiness analysis

User decision after v1 readiness analysis:

- MCP and agent surface are first-class v1, not roadmap.
- Gmail-over-IMAP archived mail gap is a v1 correctness blocker.
- Bundled Gmail OAuth may exist, but official user guidance should be "create your own OAuth client"; the bundled client is fallback/unverified, not the primary setup path.
- End-to-end launch proof must be fixed.
- Agent safety gap from the readiness report must be fixed: no more relying only on CLI convention for account scoping or source identity.
- V1 accepts unsigned macOS binaries; document the Gatekeeper friction honestly.
- Fix the current dirty worktree/docs state and document everything.

Created PE Tasker plan at `docs/implementation/v1-agent-mcp-gmail-launch/`.

### Current repo state at scoping

- Branch: `codex/fix-tui-summary-in-flight`.
- Latest GitHub release observed: `v0.5.57` at `117cc2d`.
- Local workspace version in `Cargo.toml`: `0.5.56`.
- Dirty files before this plan was added:
  - `PRIVACY.md`
  - `docs/blueprint/17-release-pipeline.md`
  - `docs/blueprint/18-addendum-oauth.md`
  - `docs/blueprint/19-addendum-docs-site.md`
  - `docs/blueprint/README.md`
  - `docs/web-app.md`
  - `site/src/content/docs/getting-started/gmail-setup.md`
  - `site/src/content/docs/guides/unsubscribe.md`
  - `site/src/pages/privacy.md`
  - `site/src/pages/terms.md`

Do not revert those existing edits. Task 005 must inspect and preserve/reconcile them.

### Routing rationale

- Use `pe-default-frontier` for all tasks. The work is Rust + daemon IPC + provider sync + release proof + product docs.
- Do not use `pe-default-local-coder`; current local route is TypeScript-tuned and this plan is high-blast-radius Rust/product work.
- Worker skills checked under `/Users/bhekanik/.dotfiles/.skills`.
- Selected skills:
  - Code tasks: `build-and-fix`, `code-review`, `tdd`, `mxr`, `security-best-practices`
  - MCP task additionally: `sdk`
  - Launch proof: `build-and-fix`, `code-review`, `tdd`, `mxr`, `deployment`
  - Docs task: `documentation-refiner`, `writing-docs`, `mxr`, `readme-optimizer`

### MCP library decision

Do not hand-roll MCP protocol plumbing. Use the official Rust SDK unless implementation evidence proves it cannot fit:

- `rmcp` official Rust SDK docs: https://rust.sdk.modelcontextprotocol.io/
- official repo: https://github.com/modelcontextprotocol/rust-sdk
- MCP SDK index: https://modelcontextprotocol.io/docs/sdk

Task 003 should add the crate dependency from crates.io and pin through Cargo.lock, not a git dependency, unless current crates.io release cannot satisfy stdio server support.

### Validation baseline

Local release gate scripts passed during readiness analysis:

- `bash scripts/release_version_gate_test.sh`
- `bash scripts/release_gmail_oauth_gate_test.sh`
- `bash scripts/release_workflow_test.sh`
- `bash scripts/provider_smoke_workflow_test.sh`
- `bash scripts/ci_workflow_test.sh`

Local `cargo build -p mxr` was inconclusive on this machine because Rust compilation stalled, while GitHub CI/release for `v0.5.57` are green. Workers should still run focused tests in their worktrees and report exact failures rather than assuming local stall equals code failure.

### Worktree caution

During readiness analysis, `site/public/openapi.json` was temporarily truncated by a stalled docs OpenAPI generation. It was restored to committed `HEAD`. If a worker sees that file dirty again, inspect before overwriting.
| 2026-06-04T17:36:07.778Z | task-001 | pe-tasker CLI | Next task discovery | succeeded | Selected task-001; executor=frontier_model; lane=frontier; risk=high. |
| 2026-06-04T17:36:08.071Z | task-001 | pe-tasker CLI | Worktree ensured | succeeded | /Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-001; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-001; created=true. |
| 2026-06-04T17:36:13.329Z | task-006 | pe-tasker CLI | Tmux session ensured | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1; created=true. |
| 2026-06-04T17:36:18.559Z | task-001 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-001; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-001/launch-20260604T173618530Z. |
| 2026-06-04T17:36:26.312Z | task-001 | pe-tasker CLI | Status transition | succeeded | ready -> in_progress; attempts=1. |
| 2026-06-04T18:00:03.646Z | task-001 | pe-tasker CLI | Deterministic validation | failed | 1 scope violation(s). |
| 2026-06-04T18:05:55.475Z | task-001 | pe-tasker CLI | Review recommendation | accept | model_review_allowed=true; reasons=deterministic validation passed and model review confidence 0.86 met minimum 0.8. |
| 2026-06-04T18:05:55.479Z | task-001 | pe-tasker CLI | Status transition | succeeded | in_progress -> completed; attempts=1. |
| 2026-06-04T18:06:39.123Z | routing-memory | pe-tasker CLI | Routing outcome recorded | passed | openai-codex/gpt-5.5/agent_safety; memory=/Users/bhekanik/code/planetaryescape/mxr/docs/implementation/v1-agent-mcp-gmail-launch/routing-memory.yaml. |
| 2026-06-04T18:06:43.383Z | task-001 | pe-tasker CLI | Integration recommendation | ask_user | merge_allowed=false; reasons=risk high requires human merge review. |
| 2026-06-04T18:06:58.969Z | task-002 | pe-tasker CLI | Worktree ensured | succeeded | /Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-002; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-002; created=true. |
| 2026-06-04T18:07:03.197Z | task-002 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-002; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-002/launch-20260604T180703169Z. |
| 2026-06-04T18:07:12.587Z | task-002 | pe-tasker CLI | Status transition | succeeded | ready -> in_progress; attempts=1. |
| 2026-06-04T18:27:11.338Z | task-002 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-002; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-002/launch-20260604T182711288Z. |
| 2026-06-04T21:30:44.802Z | task-002 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-002; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-002/launch-20260604T213044784Z. |
| 2026-06-04T21:37:03.620Z | task-002 | pe-tasker CLI | Status transition | succeeded | in_progress -> completed; attempts=1. |
| 2026-06-04T21:37:08.406Z | task-002 | pe-tasker CLI | Review recommendation | accept | model_review_allowed=true; reasons=deterministic validation passed and model review confidence 0.84 met minimum 0.8. |
| 2026-06-04T21:37:14.449Z | routing-memory | pe-tasker CLI | Routing outcome recorded | passed | openai-codex/gpt-5.5/provider_correctness; memory=/Users/bhekanik/code/planetaryescape/mxr/docs/implementation/v1-agent-mcp-gmail-launch/routing-memory.yaml. |
| 2026-06-04T21:37:18.636Z | task-002 | pe-tasker CLI | Integration recommendation | ask_user | merge_allowed=false; reasons=risk high requires human merge review. |
| 2026-06-04T22:53:59.061Z | task-001 | pe-tasker CLI | Integration execution dry-run | blocked | target=main; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-001; reasons=risk high requires human merge review. |
| 2026-06-04T22:53:59.158Z | task-002 | pe-tasker CLI | Integration execution dry-run | blocked | target=main; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-002; reasons=risk high requires human merge review. |
| 2026-06-04T22:54:09.398Z | task-001 | pe-tasker CLI | Integration landing dry-run | blocked | target=main; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-001; worktree=/Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-001; reasons=risk high requires human merge review. |
| 2026-06-04T22:54:51.657Z | task-001 | pe-tasker CLI | Integration landing dry-run | blocked | target=main; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-001; worktree=/Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-001; reasons=review recommendation is human_review. |
| 2026-06-04T22:54:51.674Z | task-001 | pe-tasker CLI | Integration execution dry-run | blocked | target=main; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-001; reasons=review recommendation is human_review. |
| 2026-06-04T23:01:32.074Z | task-001 | pe-tasker CLI | Status transition | succeeded | completed -> accepted; attempts=1. |
| 2026-06-04T23:01:32.287Z | task-002 | pe-tasker CLI | Status transition | succeeded | completed -> accepted; attempts=1. |
| 2026-06-04T23:02:10.644Z | task-003 | pe-tasker CLI | Status transition | succeeded | pending -> ready; attempts=0. |
| 2026-06-04T23:02:16.656Z | task-003 | pe-tasker CLI | Worktree ensured | succeeded | /Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-003; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-003; created=true. |
| 2026-06-04T23:03:09.625Z | task-003 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-003; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-003/launch-20260604T230309588Z. |
| 2026-06-04T23:03:15.561Z | task-003 | pe-tasker CLI | Status transition | succeeded | ready -> in_progress; attempts=1. |
| 2026-06-04T23:34:48.517Z | task-003 | pe-tasker CLI | Review recommendation | accept | model_review_allowed=true; reasons=deterministic validation passed and model review confidence 0.86 met minimum 0.8. |
| 2026-06-04T23:34:48.525Z | task-003 | pe-tasker CLI | Integration recommendation | blocked | merge_allowed=false; reasons=task task-003 is in_progress. |
| 2026-06-04T23:35:18.599Z | task-003 | pe-tasker CLI | Integration recommendation | blocked | merge_allowed=false; reasons=task task-003 is in_progress. |
| 2026-06-04T23:35:18.604Z | task-003 | pe-tasker CLI | Status transition | succeeded | in_progress -> completed; attempts=1. |
| 2026-06-04T23:35:33.388Z | task-003 | pe-tasker CLI | Integration recommendation | ask_user | merge_allowed=false; reasons=risk high requires human merge review. |
| 2026-06-04T23:35:49.725Z | task-003 | pe-tasker CLI | Status transition | succeeded | completed -> accepted; attempts=1. |
| 2026-06-04T23:41:00.050Z | routing-memory | pe-tasker CLI | Routing outcome recorded | passed | gpt-5.5/mcp_server; memory=/Users/bhekanik/code/planetaryescape/mxr/docs/implementation/v1-agent-mcp-gmail-launch/routing-memory.yaml. |
| 2026-06-04T23:41:30.251Z | task-004 | pe-tasker CLI | Status transition | succeeded | pending -> ready; attempts=0. |
| 2026-06-04T23:41:53.371Z | task-004 | pe-tasker CLI | Worktree ensured | succeeded | /Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/mxr/v1-agent-mcp-gmail-launch/task-004; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-004; created=true. |
| 2026-06-04T23:42:23.405Z | task-004 | pe-tasker CLI | Worktree ensured | succeeded | /Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-004; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-004; created=true. |
| 2026-06-04T23:42:31.881Z | task-004 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-004; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260604T234231810Z. |
| 2026-06-04T23:42:38.568Z | task-004 | pe-tasker CLI | Status transition | succeeded | ready -> in_progress; attempts=1. |
| 2026-06-04T23:58:55.989Z | task-004 | pe-tasker CLI | Review recommendation | retry | model_review_allowed=false; reasons=deterministic validation failed: no failure detail provided. |
| 2026-06-04T23:59:16.642Z | task-004 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-004; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260604T235916616Z. |
| 2026-06-05T06:16:11.221Z | task-004 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-004; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260605T061611198Z. |
| 2026-06-05T06:22:35.248Z | task-004 | pe-tasker CLI | Review recommendation | accept | model_review_allowed=true; reasons=deterministic validation passed and model review confidence 0.86 met minimum 0.8. |
| 2026-06-05T06:22:48.813Z | task-004 | pe-tasker CLI | Status transition | succeeded | in_progress -> completed; attempts=2. |
| 2026-06-05T06:22:52.662Z | task-004 | pe-tasker CLI | Status transition | succeeded | completed -> accepted; attempts=2. |
| 2026-06-05T06:23:13.662Z | task-004 | pe-tasker CLI | Integration recommendation | ask_user | merge_allowed=false; reasons=risk high requires human merge review. |
| 2026-06-05T06:25:07.030Z | routing-memory | pe-tasker CLI | Routing outcome recorded | passed | gpt-5.5/launch_proof; memory=/Users/bhekanik/code/planetaryescape/mxr/docs/implementation/v1-agent-mcp-gmail-launch/routing-memory.yaml. |
| 2026-06-05T06:25:25.141Z | task-005 | pe-tasker CLI | Status transition | succeeded | pending -> ready; attempts=0. |
| 2026-06-05T06:25:49.178Z | task-005 | pe-tasker CLI | Worktree ensured | succeeded | /Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/mxr/v1-agent-mcp-gmail-launch/task-005; branch=pe/mxr/v1-agent-mcp-gmail-launch/task-005; created=true. |
| 2026-06-05T06:26:56.448Z | task-005 | pe-tasker CLI | Pi worker launched | succeeded | pe-mxr-v1-agent-mcp-gmail-launch-1/task-005; model=openai-codex/gpt-5.5; logs=/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-005/launch-20260605T062656422Z. |
| 2026-06-05T06:27:07.453Z | task-005 | pe-tasker CLI | Status transition | succeeded | ready -> in_progress; attempts=1. |
| 2026-06-05T06:41:30.843Z | task-005 | pe-tasker CLI | Review recommendation | accept | model_review_allowed=true; reasons=deterministic validation passed and model review confidence 0.86 met minimum 0.8. |
| 2026-06-05T06:41:41.785Z | task-005 | pe-tasker CLI | Status transition | succeeded | in_progress -> completed; attempts=1. |
| 2026-06-05T06:42:33.447Z | task-005 | pe-tasker CLI | Integration recommendation | ask_user | merge_allowed=false; reasons=risk medium requires human merge review. |
| 2026-06-05T06:43:59.223Z | task-005 | pe-tasker CLI | Status transition | succeeded | completed -> accepted; attempts=1. |
| 2026-06-05T06:44:06.801Z | routing-memory | pe-tasker CLI | Routing outcome recorded | passed | gpt-5.5/docs_worktree; memory=/Users/bhekanik/code/planetaryescape/mxr/docs/implementation/v1-agent-mcp-gmail-launch/routing-memory.yaml. |

## Model performance ledger

| Time | Model | Task type | Outcome | Notes |
| --- | --- | --- | --- | --- |

## 2026-06-04 — Task 001 worker completed

Worker `task-001` exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-001/launch-20260604T173618530Z`

Worker summary:

- Added first-class IPC/activity origins: `human`, `script`, `agent`, `mcp`.
- Added config-backed agent/MCP profiles under `[agents.profiles.<agent|mcp>]` with `safety_policy`, `allowed_accounts`, `allow_send`, `allow_destructive`.
- Enforced Agent/MCP profiles in daemon dispatch: profile required, safety policy applied, account allowlists checked, send/destructive gates block unless enabled.
- Preserved human CLI/TUI/web/daemon behavior.
- Added tests for profile parsing, IPC source serialization, activity source preservation, account scoping, and send-gate blocking.

Worker-reported validation:

- `cargo build -p mxr` passed.
- `scripts/cargo-test -p mxr --lib` passed.
- `scripts/cargo-test -p mxr --test activity_invariants` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.
- `scripts/cargo-test -p mxr-config agent_surface_profiles_parse_with_safe_defaults` passed.
- `scripts/cargo-test -p mxr-protocol client_kind_serializes_first_class_origins` passed.

Host validation initially flagged a task-spec scope omission: `crates/daemon/tests/activity_invariants.rs` was not in `allowed_paths`. This is a legitimate test path for the task, so the spec was corrected to allow `crates/daemon/tests/**` before rerunning validation.

Host validation notes:

- `pe-tasker validate` from the main worktree passed scope, then stalled in `cargo build -p mxr` at the same local `mxr_tui` rustc/sccache stall seen during readiness analysis. The stuck validation/cargo PIDs were terminated; this is local validation infrastructure noise, not accepted code evidence.
- The worker worktree does not contain this uncommitted plan, so `pe-tasker validate` cannot run there directly.
- Manual host validation from worker worktree passed:
  - `git diff --check`
  - `scripts/cargo-test -p mxr-config agent_surface_profiles_parse_with_safe_defaults`
  - `scripts/cargo-test -p mxr-protocol client_kind_serializes_first_class_origins`
  - `scripts/cargo-test -p mxr --test activity_invariants activity_mapper_preserves_agent_and_mcp_sources`
  - `scripts/cargo-test -p mxr --lib agent_profile_enforces_account_allowlist_in_dispatch`
  - `scripts/cargo-test -p mxr --lib mcp_profile_send_gate_blocks_provider_send`
  - `scripts/cargo-test -p mxr --test cli_journey`

PE Tasker review/integration:

- `review recommend task-001` => `accept`, confidence 0.86.
- `integrate recommend task-001` => `ask_user`; merge blocked by high-risk human gate.
- Task 001 remains completed and not integrated into the main worktree. Task 003 must not start until task 001 is integrated or explicitly superseded.

## 2026-06-04 — Task 002 first pass and retry

Worker `task-002` first pass exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-002/launch-20260604T180703169Z`

Worker summary:

- Detects Gmail IMAP `X-GM-EXT-1`.
- Uses Gmail All Mail as the canonical Gmail-over-IMAP sync source.
- Fetches/parses `X-GM-LABELS`, `X-GM-MSGID`, and `X-GM-THRID`.
- Maps Gmail labels into mxr `label_provider_ids`.
- Adds paginated/resumable Gmail All Mail initial backfill.
- Keeps non-Gmail IMAP folder sync behavior unchanged.

Worker-reported validation:

- `scripts/cargo-test -p mxr-provider-imap --tests` passed.
- `scripts/cargo-test -p mxr-sync --tests` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.
- `cargo build -p mxr` passed.

Host validation:

- `git diff --check` passed.
- `scripts/cargo-test -p mxr-provider-imap --tests` passed.
- `scripts/cargo-test -p mxr-sync --tests` passed.
- `cargo build -p mxr` passed after rerunning alone. An earlier parallel run of `cargo build -p mxr` overlapped with `scripts/cargo-test`, and the test wrapper reaped the parallel cargo process as stale local validation noise.

Host review finding:

- The first pass still let `delta_gmail_all_mail_sync` fall back to `old_mailboxes.first()` when no exact All Mail cursor existed.
- Existing beta users may have pre-v1 per-folder Gmail IMAP cursors. The first v1 Gmail All Mail run must force a canonical All Mail backfill/full sync instead of treating an arbitrary folder cursor as All Mail, or archived-only mail can still be skipped.
- Task spec `002-gmail-imap-all-mail.md` was updated with this acceptance criterion before retry.

Retry launch:

- `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-002/launch-20260604T182711288Z` exited 0 but was launched with read-only tools (`read,grep,find,ls`) by host mistake.
- The retry confirmed the finding and produced a proposed patch/tests, but could not edit or validate.
- Relaunch required with writable Pi tools: `read,bash,edit,write,grep,find,ls`.

## 2026-06-04 — Task 002 writable retry completed

Writable retry `task-002` exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-002/launch-20260604T213044784Z`

Retry summary:

- `delta_gmail_all_mail_sync` now requires an exact persisted All Mail cursor.
- If Gmail advertises `X-GM-EXT-1` and the existing cursor only contains old per-folder mailboxes such as `INBOX`, the provider starts a canonical All Mail backfill instead of treating the first old mailbox as All Mail.
- Added focused regression coverage in `gmail_delta_without_all_mail_cursor_starts_canonical_backfill`.

Worker-reported validation:

- `scripts/cargo-test -p mxr-provider-imap --tests gmail_delta_without_all_mail_cursor_starts_canonical_backfill` passed.
- `scripts/cargo-test -p mxr-provider-imap --tests` passed.
- `scripts/cargo-test -p mxr-sync --tests` passed.
- `cargo build -p mxr` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.
- `git diff --check` passed.

Host validation:

- `git diff --check` passed.
- `scripts/cargo-test -p mxr-provider-imap --tests gmail_delta_without_all_mail_cursor_starts_canonical_backfill` passed.
- `scripts/cargo-test -p mxr-provider-imap --tests` passed.
- `scripts/cargo-test -p mxr-sync --tests` passed.
- `cargo build -p mxr` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.

Host review:

- The retry fixes the task-blocking migration bug found in the first pass.
- Residual known limitation: Gmail IMAP delta still tracks new UIDs and does not yet use Gmail-specific change history for older-message label churn. This was not part of the task acceptance criterion; v1 launch proof should document/cover expected behavior.

PE Tasker review/integration:

- `review recommend task-002` => `accept`, confidence 0.84.
- `routing record` => `provider_correctness` passed for `openai-codex/gpt-5.5`.
- `integrate recommend task-002` => `ask_user`; merge blocked by high-risk human gate.
- Task 002 remains completed and not integrated into the main worktree.

## 2026-06-04 — User-approved task 001/002 integration

User approved the high-risk integration gate in chat.

Because the primary worktree on `codex/fix-tui-summary-in-flight` contains unrelated dirty compose/docs changes, including overlap in `crates/daemon/src/handler/mod.rs` and `crates/protocol/src/types.rs`, the reviewed task work was not merged into that dirty worktree. A clean integration worktree was created instead:

- Branch: `codex/v1-agent-mcp-gmail-launch`
- Worktree: `/Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/integration`
- Task 001 commit: `c0651b54 feat: add agent and mcp safety profiles`
- Task 002 commit: `05e23427 fix: sync gmail imap from all mail`
- Integration head: `704c8f79 merge: land task-002 gmail imap all mail`

Integrated-base validation:

- `git diff --check` passed.
- `scripts/cargo-test -p mxr-config agent_surface_profiles_parse_with_safe_defaults` passed.
- `scripts/cargo-test -p mxr-protocol client_kind_serializes_first_class_origins` passed.
- `scripts/cargo-test -p mxr --test activity_invariants activity_mapper_preserves_agent_and_mcp_sources` passed.
- `scripts/cargo-test -p mxr --lib agent_profile_enforces_account_allowlist_in_dispatch` passed.
- `scripts/cargo-test -p mxr --lib mcp_profile_send_gate_blocks_provider_send` passed.
- `scripts/cargo-test -p mxr-provider-imap --tests gmail_delta_without_all_mail_cursor_starts_canonical_backfill` passed.
- `scripts/cargo-test -p mxr-provider-imap --tests` passed.
- `scripts/cargo-test -p mxr-sync --tests` passed.
- `cargo build -p mxr` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.

Task 001 and task 002 statuses were moved to `accepted`.

## 2026-06-04 — Task 003 MCP SDK research

Context7 lookup selected official RMCP docs:

- Library: `/websites/rs_rmcp_rmcp`
- Relevant docs: `rmcp` server/tool macros, `tool_router`, `ServerHandler`, JSON Schema via `schemars`, and stdio transport.

Implementation guidance for worker:

- Use the official `rmcp` crate unless Cargo resolution proves it impossible.
- Expected features: `server`, `macros`, `schemars`, and `transport-io`.
- Prefer `#[tool_router(server_handler)]`/`#[tool]` for tool definitions with typed `serde`/`schemars` params/results.
- Use stdio transport from `rmcp::transport::io::stdio()` or equivalent documented current API.

## 2026-06-05 — Task 003 MCP server accepted and integrated

Worker `task-003` exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-003/launch-20260604T230309588Z`

Worker summary:

- Added `crates/mcp` as a first-party MCP crate using official `rmcp` 1.7.0.
- Exposed both `mxr mcp serve` and `mxr-mcp`.
- MCP stdio server calls daemon IPC with `ClientKind::Mcp`.
- Tools cover status, list/search, read message/thread, draft assist/save draft, mutation preview/apply, and gated send.
- Mutation preview mirrors the existing CLI dry-run shape for direct message-id selections by resolving envelopes before mutation.
- Updated CLI help snapshots for the new command and first-class activity sources.

Worker-reported validation:

- `cargo build -p mxr` passed.
- `scripts/cargo-test -p mxr --lib` passed.
- `scripts/cargo-test -p mxr-mcp --tests` passed.
- `scripts/cargo-test -p mxr --test cli_help` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.

Host validation:

- `git diff --check` passed.
- `scripts/cargo-test -p mxr-mcp --tests` passed.
- `scripts/cargo-test -p mxr --lib` passed.
- `scripts/cargo-test -p mxr --test cli_help` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.
- `cargo build -p mxr` passed.

PE Tasker review/integration:

- `review recommend task-003` => `accept`, confidence 0.86.
- `integrate recommend task-003` => `ask_user`; merge blocked by high-risk human gate.
- User approval in chat was applied to the high-risk integration gate.
- Task branch commit: `ad5d8db2 feat: add first-party mcp server`.
- Integration branch merge: `8301d729 merge: land task-003 mcp server`.

Integrated branch validation:

- `git diff --check HEAD~1..HEAD` passed.
- `scripts/cargo-test -p mxr-mcp --tests` passed.
- `scripts/cargo-test -p mxr --lib` passed.
- `scripts/cargo-test -p mxr --test cli_help` passed.
- `scripts/cargo-test -p mxr --test cli_journey` passed.
- `cargo build -p mxr` passed.

## 2026-06-05 — Task 004 launch proof started

Task 004 was marked ready after task 001, task 002, and task 003 were accepted on `codex/v1-agent-mcp-gmail-launch`.

Worktree:

- `/Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-004`

Worker launch:

- Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260604T234231810Z`
- Model: `pe-default-frontier` (`openai-codex/gpt-5.5`)
- Attach: `tmux attach -t pe-mxr-v1-agent-mcp-gmail-launch-1`

Worktree hygiene note:

- An initial task-004 worktree was accidentally created at a duplicated root path.
- It was empty, removed immediately with `git worktree remove`, and recreated at the canonical path above before the worker was launched.

## 2026-06-05 — Task 004 first pass needs retry

Worker `task-004` exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260604T234231810Z`

Worker summary:

- Added `scripts/v1_launch_proof.sh`.
- Added `scripts/live_provider_smoke_evidence.sh`.
- Added `docs/implementation/v1-agent-mcp-gmail-launch/launch-proof.md`.
- Updated release/provider smoke workflows to run the deterministic proof and live-provider evidence.
- Updated `live_gmail_e2e` to print explicit missing-credential evidence.

Host review finding:

- Retry required: the deterministic proof defines `[agents.profiles.agent]` but never sends a real daemon IPC request tagged `source=agent`.
- Therefore the launch proof does not yet satisfy the task requirement to prove agent policy enforcement end to end.
- The MCP proof initially hung because the first pass used `Content-Length` framing; worker corrected it to newline-delimited JSON and reported the script passing.

PE Tasker review:

- `review recommend task-004` => `retry`.
- Task 004 spec was tightened to require real `source=agent` IPC proof for allowed and blocked policy paths.

## 2026-06-05 — Task 004 second pass needs focused retry

Worker `task-004` retry exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260604T235916616Z`

Worker summary:

- Hardened `scripts/v1_launch_proof.sh` with real raw daemon IPC requests tagged `source=agent`.
- Proved allowed agent `GetBody` and `SaveDraft`.
- Proved blocked agent `SendStoredDraft` and `SetFlags` via daemon policy error.
- Re-ran deterministic proof and gate scripts successfully.

Host review finding:

- The agent/MCP deterministic proof gap is fixed.
- The live-provider evidence script still only emits `creds_available` for IMAP/SMTP when credentials are present.
- Task 004 requires live Gmail and IMAP/SMTP smoke paths to be CI-safe and honest: when credentials exist, the path must run a real smoke check or explicitly emit `unavailable_no_live_smoke` evidence. It must not treat credential presence alone as proof.
- Task 004 scope was expanded to allow provider test crates so the retry can add real ignored live-smoke tests where appropriate.

## 2026-06-05 — Task 004 launch proof accepted and integrated

Worker `task-004` focused retry exited 0.

Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-004/launch-20260605T061611198Z`

Worker summary:

- Kept the real `source=agent` daemon IPC proof for allowed read/draft paths and blocked send/destructive paths.
- Added MCP read-message invocation to the deterministic proof, in addition to tool listing and gated send.
- Fixed JSON artifact emission defaults.
- Made live-provider evidence honest:
  - Gmail emits `live_smoke_passed` only after the ignored live Gmail test succeeds.
  - Missing credentials emit `skipped_missing_creds`.
  - IMAP/SMTP credentials without committed network-safe live smokes emit `unavailable_no_live_smoke`, not silent success.

Task branch commit:

- `9f24059e test: add v1 launch proof gate`

Host validation from task worktree passed:

- `git diff --check`
- `bash scripts/release_version_gate_test.sh`
- `bash scripts/release_gmail_oauth_gate_test.sh`
- `bash scripts/provider_smoke_workflow_test.sh`
- `bash scripts/live_provider_smoke_evidence.sh`
- dummy IMAP/SMTP credential evidence path in `scripts/live_provider_smoke_evidence.sh`
- `cargo build -p mxr`
- `bash scripts/v1_launch_proof.sh`
- `scripts/cargo-test -p mxr --test cli_journey`
- `scripts/cargo-test -p mxr --test live_gmail_e2e -- --nocapture`
- `scripts/cargo-test -p mxr-mcp --tests`

PE Tasker review/integration:

- `review recommend task-004` => `accept`, confidence 0.86.
- `integrate recommend task-004` => `ask_user`; high-risk merge gate reused the user's prior explicit approval to continue.
- Integration branch merge: `7a1b47eb merge: land task-004 launch proof`.

Integrated branch validation passed:

- `git diff --check HEAD~1..HEAD`
- `bash scripts/release_version_gate_test.sh`
- `bash scripts/release_gmail_oauth_gate_test.sh`
- `bash scripts/provider_smoke_workflow_test.sh`
- `bash scripts/live_provider_smoke_evidence.sh`
- dummy IMAP/SMTP credential evidence path in `scripts/live_provider_smoke_evidence.sh`
- `cargo build -p mxr`
- `bash scripts/v1_launch_proof.sh`
- `scripts/cargo-test -p mxr --test cli_journey`
- `scripts/cargo-test -p mxr --test live_gmail_e2e -- --nocapture`
- `scripts/cargo-test -p mxr-mcp --tests`

Routing memory:

- `openai-codex/gpt-5.5/launch_proof` recorded as passed.

## 2026-06-05 — Task 005 docs/worktree hygiene started

Task 005 was marked ready after task 004 was accepted and integrated.

Worktree:

- `/Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-005`

Worktree hygiene note:

- `pe-tasker worktree create` again expanded the root into a duplicated nested path for task 005.
- The mistakenly-created duplicate worktree was clean, removed immediately with `git worktree remove`, and its empty branch was deleted.
- The canonical task 005 worktree was then created manually from `codex/v1-agent-mcp-gmail-launch`.

Worker launch:

- Run dir: `/Users/bhekanik/.pe-tasker/runs/mxr/v1-agent-mcp-gmail-launch-1/task-005/launch-20260605T062656422Z`
- Model: `pe-default-frontier` (`openai-codex/gpt-5.5`)
- Exit code: 0

Worker summary:

- Updated README, TODO, SECURITY, PRIVACY, and site terms/privacy for v1 truth.
- Made Gmail OAuth docs BYOC-first, with bundled OAuth documented as an unverified fallback only.
- Documented Gmail-over-IMAP All Mail behavior for archived messages.
- Documented first-class agent and MCP surfaces, including `[agents.profiles.agent]`, `[agents.profiles.mcp]`, account allowlists, send/destructive gates, activity origins, `mxr mcp serve`, and MCP tools.
- Documented unsigned macOS binaries as accepted for v1, including Gatekeeper friction.
- Reconciled relevant dirty primary-worktree docs instead of reverting them.
- Regenerated `site/public/openapi.json`.

Host validation from task worktree passed:

- `git diff --check`
- `jq empty site/public/openapi.json`
- `bash scripts/release_version_gate_test.sh`
- `bash scripts/release_gmail_oauth_gate_test.sh`
- `bash scripts/provider_smoke_workflow_test.sh`
- `bash scripts/release_workflow_test.sh`
- `bash scripts/ci_workflow_test.sh`
- `cargo build -p mxr`
- `cd site && npm ci`
- `cd site && npm run build`

PE Tasker review/integration:

- `review recommend task-005` => `accept`, confidence 0.86.
- `integrate recommend task-005` => `ask_user`; medium-risk merge gate reused the user's explicit approval to continue.
- Task branch commit: `de25cb8f docs: align v1 launch guidance`.
- Integration branch merge: `2ce064d1 merge: land task-005 v1 docs`.

Integrated branch validation passed:

- `git diff --check HEAD~1..HEAD`
- `jq empty site/public/openapi.json`
- `bash scripts/release_version_gate_test.sh`
- `bash scripts/release_gmail_oauth_gate_test.sh`
- `bash scripts/provider_smoke_workflow_test.sh`
- `bash scripts/release_workflow_test.sh`
- `bash scripts/ci_workflow_test.sh`
- `cargo build -p mxr`
- `cd site && npm ci`
- `cd site && npm run build`
- `bash scripts/v1_launch_proof.sh`

Routing memory:

- `openai-codex/gpt-5.5/docs_worktree` recorded as passed.

## 2026-06-05 — V1 blocker plan complete

All five PE Tasker tasks are accepted and merged on `codex/v1-agent-mcp-gmail-launch`.

Integration head:

- `2ce064d1 merge: land task-005 v1 docs`

The primary worktree remains intentionally unmerged because it has unrelated dirty compose/TUI/web/docs work on `codex/fix-tui-summary-in-flight`. The clean integration branch is the v1 blocker branch to review or merge forward.
