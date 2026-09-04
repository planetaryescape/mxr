# Plan 024: Explain the agent permissions the daemon actually enforces

> Execute after plan 011 in a clean worktree. Every command below is a **future executor command, not run during authoring**. This plan improves explanation and diagnostics; it grants no new authority. Update only this plan's index row unless a reviewer owns it.

## Status

- Priority: P2
- Effort: M
- Risk: MED. an explanation must match enforcement and avoid disclosing accounts outside the caller's scope.
- Depends on: `plans/011-safe-verification-and-ci.md`
- Category: dx, security
- Planned at: fetched `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04.
- Planning evidence: source inspection only; no permission probe against a real daemon.

## Why this matters

mxr already has account restrictions, source profiles, safety policy, send gates, and destructive-action gates. Users and agents must understand the effective combination rather than guessing which configuration caused a denial. A source profile is a local runtime restriction; it is not a grant from Gmail/Outlook and is not a sandbox against arbitrary same-user shell access.

## Current state

`crates/config/src/types.rs:515` contains:

```rust
pub struct AgentProfileConfig {
    pub safety_policy: SafetyPolicy,
    pub allowed_accounts: Vec<String>,
    pub allow_send: bool,
    pub allow_destructive: bool,
    pub allowed_destructive_actions: Vec<DestructiveAction>,
}
```

Defaults are read-only, no send, and no destructive permission. An empty destructive-action list means no additional per-action restriction when the coarse gate allows it; do not reinterpret it as deny-all. Read the existing account-allowlist implementation before describing its empty-list behavior.

`crates/daemon/src/handler/mod.rs:494` checks global safety policy, then `enforce_client_profile`. At line 1391:

```rust
let profile = config
    .agent_surfaces
    .profiles
    .get(profile_name)
    .ok_or_else(|| format!("{profile_name} IPC requests require a configured profile"))?;
```

`enforce_client_profile` then checks profile policy, send gate, destructive gate, action allowlist, and account allowlist. `request_safety_class`, `request_destructive_action`, and `account_token_allowed` already encode the rules. `Agent`/`Mcp` use profiles; human/TUI/CLI/script/web/daemon sources follow the existing source treatment. Do not reclassify them in this plan.

- `crates/mcp/src/lib.rs:164` exposes existing `mxr_status` via `Request::GetStatus`.
- `crates/mcp/src/lib.rs:456` preserves daemon error code/message as MCP errors but drops optional additional details.
- `crates/daemon/src/handler/diagnostics/mod.rs:1166` and `status_helpers.rs` construct status/doctor reports; CLI renderers live in `commands/status.rs` and `commands/doctor.rs`.
- `crates/daemon/src/handler/tests.rs` uses synthetic `AppState`, fake providers, and request safety classification tests. MCP tests in `crates/mcp/src/lib.rs` use `DaemonRequester` mocks. Use these rather than an external agent or real mailbox.

## Principles carried forward

- **Local Identity Registration Is Not Provider Authorization:** local allowed account/sender state does not prove that a provider accepts a send.
- **Credential Prompts Are Side Effects:** inspection must not read interactive keychains, refresh provider tokens, or trigger authentication.
- **Parser Parity Is Not Execution Parity:** an accepted config value or available MCP tool does not mean the current source/account may execute it.
- [AgentMail documents scope intersected with operation permissions](https://docs.agentmail.to/permissions); [Google documents provider OAuth scopes separately](https://developers.google.com/workspace/gmail/api/auth/scopes). Use those distinctions to explain mxr's existing authority layers, not to import their policy rules.

## Scope and drift check

Only modify:

```text
crates/daemon/src/handler/mod.rs
crates/daemon/src/handler/policy_explanation.rs (new, only if it reduces duplication)
crates/daemon/src/handler/tests.rs
crates/daemon/src/handler/tests/routing_and_search.rs
crates/daemon/src/handler/diagnostics/mod.rs
crates/daemon/src/handler/status_helpers.rs
crates/daemon/src/commands/status.rs
crates/daemon/src/commands/doctor.rs
crates/daemon/tests/cli_journey.rs
crates/protocol/src/types.rs
crates/protocol/src/lib.rs
crates/mcp/src/lib.rs
site/src/content/docs/guides/for-agents.md
site/src/content/docs/guides/agent-skill.md
plans/README.md
```

Mechanical forwarding of additive status/doctor fields is allowed in `crates/web/src/lib.rs` and `crates/web/src/tests.rs` only if compilation requires it. No new MCP tools, permission flags, sources, profile defaults, account-scope meanings, config dumps, OAuth flows, or authorization exceptions. Do not broaden diagnostic access for a missing/denied profile just to make an explanation visible: explain the existing denial.

Run `git fetch origin main` and `git rev-parse HEAD origin/main`. With prerequisite commits applied:

```bash
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/daemon/src/handler/mod.rs crates/daemon/src/handler/policy_explanation.rs crates/daemon/src/handler/tests.rs crates/daemon/src/handler/tests/routing_and_search.rs crates/daemon/src/handler/diagnostics/mod.rs crates/daemon/src/handler/status_helpers.rs crates/daemon/src/commands/status.rs crates/daemon/src/commands/doctor.rs crates/daemon/tests/cli_journey.rs crates/protocol/src/types.rs crates/protocol/src/lib.rs crates/mcp/src/lib.rs crates/web/src/lib.rs crates/web/src/tests.rs site/src/content/docs/guides/for-agents.md site/src/content/docs/guides/agent-skill.md
```

Stop on unrecognized changes to enforcement after reconciling prerequisites. Branch `codex/024-effective-agent-permissions`. No commit/push/PR unless requested. Authorized subject: `fix: explain effective daemon agent permissions`, user author, no attribution footer.

## Steps and verification

### 1. Characterize the existing policy matrix

Add `effective_agent_permissions` behavior tests that dispatch the actual requests and assert current allow/deny decisions for global policy × source policy × account scope × send/destructive gates. Include missing profiles, `read-only`, `draft-only`, `restricted`, full policy, explicit action restrictions, empty action list, account UUID/config-key/email matching according to current implementation, and unknown accounts. Include read, local draft edit, send, reversible mutation, destructive mutation, and a rejected diagnostic request.

Use fake provider call counters to prove denial occurs before side effects. Do not create an exhaustive duplicate policy implementation in the test oracle; name representative consequential cases and use the real dispatcher.

**Verify:** `scripts/cargo-test -p mxr --lib effective_agent_permissions` executes the new tests and passes **before** explanation changes. If a currently enforced rule conflicts with documentation, record it and explain the real rule; do not silently change authority in this plan.

### 2. Derive explanations from the enforcing decision

Refactor only enough of `enforce_safety_policy`/`enforce_client_profile` to return a typed reason/context that also formats the existing error. Keep the existing request classification and account resolution helpers authoritative. Do not build a second capability table by hand in MCP or renderers.

An explanation identifies the active source/profile, the rejecting gate and request kind, permitted account identifiers only after the existing allowlist resolution, and the precise configuration key relevant to repair. Account labels may be redacted for restricted callers; excluded account names, addresses, tokens, aliases, provider cursors, and config secrets must not appear. Avoid echoing arbitrary raw config/account tokens into errors. Distinguish a locally allowed operation from provider capability/auth state; never label send as guaranteed.

Preserve existing error codes and recognizable messages. If the current protocol supports structured safe details, use it; otherwise keep typed detail internal and expose safe text plus the additive status summary. Do not broaden the `Response::Error` wire contract beyond the listed scope.

**Verify:** `scripts/cargo-test -p mxr --lib effective_agent_permissions` passes with identical allow/deny/provider-call outcomes and reason assertions for every matrix case. Add tests with secret-like sentinel strings in configuration and excluded accounts, asserting they are absent from serialized explanations.

### 3. Attach a redacted summary to existing diagnostics

Add optional serde-defaulted permission summary fields to existing status/doctor data. Summaries must be computed at the daemon dispatch seam with the actual `ClientKind` and same config snapshot used for enforcement; never infer source from a command label. Agent/MCP responses describe only their own effective context. Human diagnostics may explain configured agent/MCP profiles using safe resolved account identifiers, without serializing raw `AgentProfileConfig` or provider configuration.

Represent unknown/inapplicable authority explicitly. A missing profile continues to receive its existing denied response, now with an actionable explanation. Do not bypass enforcement to allow `mxr_status`. Preserve bounded status behavior: use current config and already available scoped account state, return unknown if unavailable, and avoid full diagnostic scans, keychain reads, or provider calls.

Render the summary in existing CLI status/doctor output. Existing `mxr_status` already serializes daemon data, so reuse it. MCP denial output should preserve the safe reason without changing tool permissions or treating `confirm=true` as authority.

**Verify:** `scripts/cargo-test -p mxr-protocol --tests effective_agent_permissions`, `scripts/cargo-test -p mxr --test cli_journey effective_agent_permissions`, and `scripts/cargo-test -p mxr-mcp --tests` pass. Assert legacy responses without the new field decode as unknown, allowed MCP status displays the actual MCP profile, denied status remains denied, restricted responses omit other profiles/accounts, and inspection makes zero provider calls. Filtered commands must run at least one test.

### 4. Document the effective model and complete regression checks

Update the two existing agent guides with one compact mapping: global policy, source profile, account restriction, send/destructive gates, provider authorization, and external OS/sandbox boundary. Explain that `confirm=true` is a send intent gate, not a privilege grant. Explain profile selection exactly as it exists, without claiming arbitrary shell agents are confined by an MCP profile.

Generate example output from synthetic profiles and fake-daemon commands executed by tests; do not publish unrun example outputs. Reuse existing read-only/draft-only examples and redact identifiers. Keep guidance on disabling/changing a gate targeted to the requested operation, not a blanket recommendation to enable full access.

**Verify:** `scripts/cargo-test -p mxr --lib`, `scripts/cargo-test -p mxr --test cli_journey`, `scripts/cargo-test -p mxr-protocol --tests`, `scripts/cargo-test -p mxr-mcp --tests`, `cargo build -p mxr`, and `git diff --check` all pass after plan 011's safe runner gate.

## Done criteria

- [ ] Actual dispatcher decisions and fake-provider call counts are unchanged across the characterized matrix.
- [ ] Every new explanation derives from the real enforcement result/helpers, not a duplicate policy table.
- [ ] Existing status/doctor/MCP surfaces show the effective source/profile and safe denial reasons.
- [ ] Missing/denied profiles remain denied; no new source, tool, bypass, default, or permission is introduced.
- [ ] Excluded accounts and credential/config sentinels never appear in serialized summaries/errors.
- [ ] Legacy responses decode safely; diagnostics remain bounded and make no auth/provider side effects.
- [ ] Documentation distinguishes local policy from provider authority and OS sandboxing; examples are verified using synthetic state.
- [ ] Focused checks, final build, diff/scope review, and index/reviewer status are complete.

## STOP conditions and maintenance

Stop if accurate introspection requires a new authority bypass, the actual enforcement cannot be reused without a wider redesign, account resolution leaks denied identities, a config inspection prompts for credentials, or a gate fails twice. If a true policy bug is discovered, report it separately rather than hiding a permission change inside explanation work. New request variants must continue using the existing exhaustive classification; their explanation should follow that same decision automatically.
