# Plan 012: Make account setup use supported authorization paths

> Execute in a clean worktree after plan 011. Run each verification gate before proceeding. All commands below are **future executor commands, not commands run during plan authoring**. Update only this plan's row in `plans/README.md` unless the dispatching reviewer owns the index. Do not connect real mail or publish a tutorial without the explicit live-verification gate below.

## Status

- Priority: P1
- Effort: L
- Risk: MED. provider-specific flow selection must preserve Outlook device authorization and existing credential storage.
- Depends on: `plans/011-safe-verification-and-ci.md`
- Category: bug, dx, docs
- Planned at: fetched `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04.
- Verification during planning: source inspection and official Google documentation; no provider authorization or executable test run.

## Why this matters

The setup menu omits supported Outlook accounts. Gmail's SSH/no-TTY path and the TUI select Google's limited-input device flow, which does not support the Gmail scopes mxr requests. The setup guide also treats external OAuth Testing mode as a lasting personal configuration without explaining its seven-day refresh-token expiry. Fix this existing first-run journey before adding providers or a shared-client verification project.

## Current state

- `crates/daemon/src/commands/setup.rs:70` offers `Demo inbox`, `Gmail account`, `IMAP/SMTP account`, and `LLM features only`. `imap_smtp_from_wizard` asks separately for IMAP and SMTP credentials.
- `crates/provider-gmail/src/auth.rs:100` requests Gmail readonly, modify, and labels scopes. `AuthFlow::auto_detect` currently contains:

```rust
if no_tty || no_display || ssh_session {
    Self::Device
} else {
    Self::Installed
}
```

- `crates/tui/src/account_workflow.rs:43` creates `StartAuthSession` with `flow: mxr_protocol::AuthFlowData::Device` for every account.
- `crates/daemon/src/handler/auth_sessions.rs:102` maps Gmail `Auto`/`Installed` to installed flow and `Device` to device flow. `SessionInstalledDelegate` exposes the URL through `AuthSessionData.auth_url`.
- `crates/daemon/src/commands/accounts.rs:395` selects flow, then `authorize_and_save_account` starts, waits for, and completes the existing daemon auth session. `print_auth_session_prompt` opens every received URL automatically.
- `apps/web/src/features/accounts/api.ts` already selects provider-specific flow; its sibling `api.test.ts` proves Gmail uses `auto` and Outlook uses `device`. Reuse that distinction; do not regress the web client.
- `crates/daemon/tests/accounts_repair_cli.rs` isolates directories and daemon addresses, disables keychain access, and proves credentials can be repaired without a running daemon.
- `site/src/content/docs/getting-started/gmail-setup.md:103` recommends device flow for SSH; line 115 recommends a TV client. Both recommendations must be replaced.

## Existing lessons and provider constraints

Carry these principles from the user's notes, without copying private examples:

- **First Run Is the Launch Surface:** setup ends with tested account state or a repairable error; BYOC remains the deliberate default when a shared client is unverified.
- **Credential Prompts Are Side Effects:** keep auth interaction in the initiating client; no prompt-capable credential lookup in daemon startup, status, or retry loops. Preserve disk-first lazy credential access and repair without daemon startup.
- **Refresh Token Rotation Is Shared State:** do not introduce another refresh owner, replace token storage, or purge credentials on transient errors while changing authorization routing.
- **Local Identity Registration Is Not Provider Authorization:** saving an account or registering an alias does not prove provider send authorization or delivery.

Official references, checked 2026-09-04:

- [Google limited-input device flow](https://developers.google.com/identity/protocols/oauth2/limited-input-device): supported scopes exclude Gmail; changing Desktop credentials to TV credentials does not solve this.
- [Google OAuth token expiration](https://developers.google.com/identity/protocols/oauth2): external projects in Testing receive seven-day refresh tokens, subject to the documented basic-identity scope exception; mxr requests Gmail scopes.
- [Google installed-app OAuth](https://developers.google.com/identity/protocols/oauth2/native-app): verify the current loopback rules before implementation. Do not use deprecated out-of-band copy/paste authorization.

## Scope and drift check

Only modify these paths:

```text
crates/provider-gmail/src/auth.rs
crates/daemon/src/commands/setup.rs
crates/daemon/src/commands/accounts.rs
crates/daemon/src/handler/auth_sessions.rs
crates/daemon/src/handler/account_config.rs
crates/tui/src/account_workflow.rs
crates/tui/src/runner/tests/accounts_and_delivery.rs
crates/daemon/tests/accounts_repair_cli.rs
crates/daemon/tests/cli_help.rs
site/src/content/docs/getting-started/gmail-setup.md
site/src/content/docs/guides/accounts.md
README.md
plans/README.md
```

No changes to credential persistence, refresh ownership, provider scopes, Outlook adapter, global setup architecture, protocol enums, or bundled-client distribution. Keep `Device` in the shared protocol for Outlook. Do not add server discovery or a new OAuth broker.

Run `git fetch origin main`, then `git rev-parse HEAD origin/main`. Work from updated remote plus completed prerequisite commits; never reset or switch the user's active checkout. Run:

```bash
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/provider-gmail/src/auth.rs crates/daemon/src/commands/setup.rs crates/daemon/src/commands/accounts.rs crates/daemon/src/handler/auth_sessions.rs crates/daemon/src/handler/account_config.rs crates/tui/src/account_workflow.rs crates/tui/src/runner/tests/accounts_and_delivery.rs crates/daemon/tests/accounts_repair_cli.rs crates/daemon/tests/cli_help.rs site/src/content/docs/getting-started/gmail-setup.md site/src/content/docs/guides/accounts.md README.md
```

Reconcile prerequisite changes; stop on unrelated changes to the described flow. Use branch `codex/012-account-setup-and-gmail-auth`. Do not commit, push, or open a PR unless dispatched to do so. Authorized commits use `fix: use supported Gmail authorization flows`, user author, no attribution footer.

## Steps and verification

### 1. Define provider-specific flow and prompt behavior

Factor pure selection helpers in the existing account workflow files. Gmail uses installed-loopback authorization regardless of stdin/display/SSH signals. Outlook keeps its supported device flow. Environment signals decide whether to open a local browser or display remote instructions, not which provider grant is legal. Explicit Gmail `Device` requests must fail early with an actionable error before token/device network calls; preserve the shared enum for older clients and Outlook.

Add tests named with `account_setup_polish` covering Gmail local/SSH/no-TTY, Outlook local/SSH, daemon `Auto`, explicit unsupported Gmail device requests, and no browser launch from remote/noninteractive presentation. Follow the existing web account API tests' matrix and TUI fake IPC tests.

**Verify:** `scripts/cargo-test -p mxr --lib account_setup_polish` and `scripts/cargo-test -p mxr-tui --tests account_setup_polish` must each run at least one test and pass. `scripts/cargo-test -p mxr-provider-gmail --tests` must pass existing auth/storage tests. Do not accept a zero-test filtered run.

### 2. Make the existing loopback session usable over SSH

Use `AuthSessionData.auth_url` and the installed-flow delegate already present. Parse the authorization URL with `url::Url`; extract and validate its `redirect_uri`. Only a loopback HTTP host and an explicit actual callback port qualify. Do not guess a port or expose the listener on a public interface.

For SSH, print a second-terminal forwarding instruction using that exact port: `ssh -N -L 127.0.0.1:<port>:127.0.0.1:<port> <your-ssh-host>`. The user keeps the remote authorization session running, starts the forward on their browser machine, then opens the original authorization URL there. `<your-ssh-host>` is a documented placeholder, not a hostname inferred from `SSH_CONNECTION`. Match the actual redirect host/address family: if the dependency uses IPv6, provide a verified matching forward or stop. Never claim forwarding is configured automatically. Keep cancellation and existing timeout behavior.

Write unit tests for valid IPv4/localhost callback URLs, missing/invalid redirect URI, non-loopback hosts, and remote browser suppression. Do not print or log tokens, authorization codes, credentials, or raw provider responses in diagnostics.

**Verify:** `scripts/cargo-test -p mxr --lib account_setup_polish` passes. A local fake callback test must verify the displayed port reaches the listener; it proves wiring, not Google authorization. If the dependency does not expose a routable loopback callback before the user opens the URL, stop and report instead of inventing an alternative flow.

### 3. Complete the provider menu using existing operations

Add Outlook personal and Microsoft 365 work/school entries to `run_interactive_setup`, routing into the same `AccountsAction::Add` variants already used by `mxr accounts add outlook` and `outlook-work`. Avoid copying OAuth/token handling into setup. Offer explicit IMAP-credential reuse for SMTP while retaining distinct credentials and authentication-free server configurations. Keep validation and first-sync reporting separate: authenticated, saved, and sync complete are different states.

For new Gmail setup, make Desktop BYOC the explicit default in the wizard and interactive account command. Merely compiling a bundled client ID/secret does not establish that Google has verified it. Preserve the existing bundled-client option as an explicit choice and preserve already configured accounts; do not change credential distribution or add verification infrastructure. Test the default with bundled credentials both available and absent, using a pure selection helper rather than real secrets.

Test the provider menu data and emitted account requests through extracted pure helpers; no TTY scripting or real credentials are needed. Preserve existing repair commands and failure messages.

**Verify:** `scripts/cargo-test -p mxr --lib account_setup_polish`, `scripts/cargo-test -p mxr --test accounts_repair_cli`, and `scripts/cargo-test -p mxr --test cli_help` pass. Repair tests must still show no daemon/socket startup and no keychain prompt.

### 4. Correct the guide around observed behavior

Remove the recommendation to obtain TV credentials for Gmail and the claim that auto-selected device flow works anywhere. Explain Desktop BYOC, local loopback, the exact remote route, seven-day Testing-mode expiry, and explicit reauthorization. Explain Production publishing and Google's verification rules precisely; do not claim that a status switch universally removes all verification requirements or guarantees perpetual refresh tokens.

Before claiming runnable SSH/Google instructions are verified, execute the complete route on an explicitly authorized disposable Google account and actual SSH target. The operator supplies these; never reuse stored personal credentials implicitly. Record redacted success/failure evidence. If live validation is unavailable, finish and hand off the tested implementation and accurate provider-policy corrections. Mark the exact Google/SSH walkthrough as pending live verification, and retain proposed instructions as unverified. This outstanding walkthrough must not block useful tested code. No mail needs sending for OAuth validation.

**Verify:** the recorded route obtains a Gmail access token and performs the existing read-only account test without device/OOB flow; an invalid grant produces one repair action. Re-run the docs' exact commands before presenting them as verified. Final `cargo build -p mxr` and `git diff --check` exit 0.

## Done criteria

- [ ] Gmail local, SSH, no-TTY, CLI, and TUI paths use supported installed authorization; explicit legacy device requests fail before network side effects.
- [ ] Outlook retains device authorization and appears in the setup menu.
- [ ] New Gmail setup defaults to Desktop BYOC; bundled credentials remain an explicit choice and existing accounts retain their configuration.
- [ ] Remote instructions use the actual loopback callback and never open a remote browser automatically.
- [ ] Existing repair-without-daemon tests pass; credential storage/refresh authority are unchanged.
- [ ] Testing-mode expiry is explained accurately; no TV-client or OOB workaround is recommended.
- [ ] All future verification gates pass; provider/SSH evidence is recorded or the publication gate is explicitly blocked.
- [ ] Only scoped files changed; build and diff checks pass; reviewer/index records outcome.

## STOP conditions and maintenance

Stop if Google changes supported grant/scopes, the installed-flow library cannot expose a reachable callback, a shared-client verification project becomes necessary, credential storage/refresh ownership must change, or a verification fails twice. Do not loosen authorization or copy tokens between accounts to bypass a failure. Reviewers should check provider distinctions, callback address family, prompt ownership, and claims about token lifetime. Full automatic server discovery remains outside this plan.
