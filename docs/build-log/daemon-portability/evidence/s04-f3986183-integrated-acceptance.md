# S04 integrated candidate evidence

This evidence snapshot records candidate `f3986183c92f532e125304cb15c3a70eb9c44c7c`
(tree `d2ccd484e206b414be52668954104a0075e3082a`) as observed on 2026-10-11.
It is a pre-release record; PR374 is not merged and no release artifact has been
verified.

The timeout and browser Undo wording follows this candidate's
`crates/daemon/src/ipc_client.rs#L6-L38` and
`apps/web/src/features/mail-actions/mailMutations.ts#L67-L85`.

The immutable macOS binary was built with
`cargo build -p mxr --features web-ui --locked`. Its SHA256 is
`aeff94d9eefc4dca26c6c0b7eabf90231db7b8e9b398661ae4076f64c7fc1ae5` and the
build took 88 seconds. The build log and source manifest are in the stage
worktree at
`/Users/bhekanik/.dotfiles/.codex/worktrees/mxr-portability-stage-01/mxr/target/s04-evidence/build-integrated-f3986183.log`
and `binary-f3986183.json`.

The root CLI/IPC/HTTP harness passed 17 cases in 13.252 seconds using that
binary. The case names are `json_list_and_explicit_submission_id`,
`json_list_continues_after_refused_receipt_without_dispatch`,
`completed_archive_history_once_after_concurrent_replay_and_restart`,
`lost_ack_concurrent_repeat_changed_payload_restart`,
`workspace_and_generation_refusal`, `partial_batch_with_proved_nonacceptance`,
`provider_acceptance_response_loss_stays_unknown`,
`crash_query_is_read_only_and_same_id_resumes_only_pending`,
`stale_provider_binding_refused_before_dispatch`,
`failed_durable_acceptance_prevents_provider_call`,
`failed_receipt_after_acceptance_never_repeats_effect`,
`provider_success_survives_failed_local_reconciliation_without_repeat`,
`client_retention_failure_prevents_effect`, `unsupported_output_refused_before_effect`,
`synchronous_known_rejection_fails_with_retained_json_receipt`,
`synchronous_unknown_fails_but_read_only_query_succeeds`, and
`retained_operation_count`. Each passed in `root-integrated-f3986183.json`.

The actual MCP harness passed three cases in 11.305 seconds:
`mcp_preview_retained_and_confirmation_required`,
`mcp_restart_outcome_and_same_identity_no_repeat`, and
`mcp_unknown_persists_restart_query_and_submit_no_repeat`. Each passed in
`mcp-integrated-f3986183.json`. Both runtime manifests name
the same source SHA and binary hash, and report their owned daemon/processes
stopped. The CLI crash probe killed only its owned daemon. These are process
crash and restart checks; they do not prove survival of an operating-system
crash or power loss. SQLite WAL with `synchronous=NORMAL` has that documented
limit; the related durability work is tracked in
[`archive-operation-recovery-followups.md`](../../../issues/archive-operation-recovery-followups.md).

Focused verification recorded 14 unit tests and 2 E2E tests passing; typecheck,
lint and formatting passed. The integrated scoped Rust gate passed, and the SPA
build took 3.95 seconds. Root's ledger records those checks and the harness
acceptance at
`/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-daemon-portability-20261010/ledger.md`.
These are measured command outcomes, not a summed delivery-time estimate.

The available direct TUI and browser recovery manifests still use candidate
`4cda759c3e8baf2c4dc85f3444ce5b77512de6d1` and binary SHA256
`e22c27a8b255c24489d71932725bea68fe1ed29afc983633f96b1b6da24ee02d`; they are
not proof against `f3986183`. Their root-acceptance records are
`/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-daemon-portability-20261010/s04-tui-corrected-root-acceptance.json`
and `s04-browser-corrected-root-acceptance.json`.

CI run [38164938586](https://github.com/planetaryescape/mxr/actions/runs/38164938586)
failed its fast Rust suite on the existing owed-reply timing assertion: 3,601
seconds instead of 3,600. The shared fixture reads the clock for each message;
a deterministic fixture correction is being verified. The semantic Rust suite
passed; Web E2E remains in progress as of 18:52 UTC. OpenAPI check
[38164938587](https://github.com/planetaryescape/mxr/actions/runs/38164938587)
passed, and web type regeneration produced no tracked changes. Docs OpenAPI
regeneration remains pending until release closure. A fresh whole-diff Opus
review started at 18:44:25 UTC with a 20-minute budget and remains in progress.
Final TUI and browser acceptance against the release candidate, merge,
release-please, published artifact verification, and installed-client journeys
remain open.
