# Plan 011: Make verification safe and run every affected check

> Executor: read this file before changing code. Run each verification and record its result. Update this plan's status in `plans/README.md` only after review and integration. Do not run the current `scripts/cargo-test` before removing its global process cleanup.
>
> Drift check: fetch `origin main`, record the new SHA, and compare `git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..origin/main -- scripts/cargo-test scripts/ci_workflow_test.sh .github/workflows/ci.yml`. Inspect changed code before proceeding. Use an isolated worktree from fetched main; never reset the user's checkout.

## Status

- Priority: P1
- Effort: M
- Risk: MED
- Depends on: none
- Category: tests
- Planned at: `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04
- Execution: TODO. Commands below are future verification gates, not results obtained during planning.

## Why this matters

The canonical test wrapper can kill unrelated cargo processes. CI also treats overlapping Rust/web paths as exclusive, so a bridge or protocol change can skip the SPA unit and browser checks. Finally, an early dependency audit failure prevents later web checks from running, leaving their status unknown. Fix these before dispatching the other plans.

## Current state

`scripts/cargo-test:29` collects processes across the machine. Its parent check does not establish ownership:

```bash
pids=$(pgrep -f 'cargo test|cargo run|cargo build' 2>/dev/null || true)
```

`scripts/cargo-test:46` kills the selected processes:

```bash
kill -9 "${stale[@]}" 2>/dev/null || true
```

The wrapper then pins `CARGO_TARGET_DIR` to this checkout's `target-cli/` and uses `exec cargo test "$@"`. Preserve those useful behaviors.

In `.github/workflows/ci.yml:65`, the broad Rust case precedes the web cases and terminates with `;;`:

```bash
Cargo.toml|Cargo.lock|rust-toolchain*|build.rs|crates/*|src/*|.sqlx/*)
  rust=true
  ;;
```

The later `apps/web/*|...|crates/web/*|...|crates/protocol/*` case cannot set `web=true` for paths already matched. The empty/zero base fallback sets Rust and docs, but omits web. `scripts/ci_workflow_test.sh` currently checks gitleaks and SQLx dependency strings; it does not execute the classifier. Model fixture-based tests on `scripts/release_change_scope_test.sh`, which creates its own temporary Git repository. Preserve release classification behavior.

`ci.yml:294` runs these sequentially:

```yaml
- run: npm ci
- run: npm audit --audit-level=moderate
- run: npm run typecheck
- run: npm run lint
- run: npm run test
```

Node 24 is the workflow runtime. `.github/workflows/openapi-conformance.yml` already checks the bridge separately. This plan repairs the omitted SPA checks; it does not claim all bridge checks are absent.

Vault lessons: **Serial Gates Hide the Failures Behind Them** says independent checks need independent outcomes. **Release Gates Encode Product Contracts** says deterministic checks should block on their own failures; absence of optional live credentials should not invalidate a coherent BYOC build.

## Scope

Modify only:

- `scripts/cargo-test`
- `scripts/cargo_test_wrapper_test.sh`, create
- `scripts/ci_change_scope.sh`, create
- `scripts/ci_change_scope_test.sh`, create
- `scripts/ci_workflow_test.sh`
- `.github/workflows/ci.yml`
- `plans/README.md`, status only

Do not change release workflows, branch protection names, dependency versions, application code, test budgets, or credential/signing requirements. No generic process supervisor or new CI service.

## Steps

### 1. Remove machine-wide cleanup from the wrapper

Delete `reap_zombies` and its invocation. Keep per-checkout target directory selection, argument forwarding, the workspace warning, and `exec`. Cargo owns its build lock. A lock wait is not evidence that another process may be terminated. If the operator needs to stop a previous command, they must stop the identified invocation they own.

Add `scripts/cargo_test_wrapper_test.sh`. Its fake `cargo` records arguments and target directory and returns a configurable exit code. Put fake `pgrep` and `ps` first in PATH; record any invocation and return no PIDs. Never allow the old script to inspect real processes during a regression test. Assert no process inspection, exact argument boundaries including spaces, preservation of exit code 17, checkout-local target directory, and the existing workspace warning. Tests must clean up only their own temporary directory and children.

Verify: `bash -n scripts/cargo-test scripts/cargo_test_wrapper_test.sh` and `bash scripts/cargo_test_wrapper_test.sh` both exit 0. Run these before any cargo wrapper command.

### 2. Make change categories additive and test the actual classifier

Extract the CI change classification into `scripts/ci_change_scope.sh`. Give it explicit event/base/head inputs, validate refs, and print the existing `rust`, `web`, `docs`, `expensive` booleans. The workflow must call this script; do not leave a second inline implementation.

Use independent matches so a file can select both Rust and web. Preserve current Rust-only and site-only decisions. Changes to `crates/web/` or `crates/protocol/` select Rust and web. Changes to this classifier, its tests, or `ci.yml` select the checks they can affect. Manual dispatch selects all and expensive. Missing/zero base selects all ordinary checks conservatively; a failed Git diff/ref lookup must fail visibly rather than produce an empty success. Keep expensive work opt-in. Use NUL-delimited Git paths, or explicitly reject unsupported path input rather than silently losing a filename.

Create `scripts/ci_change_scope_test.sh` using temporary Git commits like the release scope tests. Cover Rust-only, app-only, bridge-only, protocol-only, site-only, mixed paths, deletions, renamed paths, spaces in filenames, workflow changes, zero/missing base, invalid refs, and dispatch. Assert all four outputs, not only the expected true field.

Verify: `bash scripts/ci_change_scope_test.sh` exits 0. `bash scripts/release_change_scope_test.sh` still exits 0. Add the new tests to the existing unconditional script-check job so a broken filter cannot skip its own test.

### 3. Let web checks report independent failures

Keep Node 24 and `npm ci` as the shared prerequisite. Give the install step an ID. Run audit, typecheck, lint, and tests independently once installation succeeds, including when an earlier check failed. Use explicit step conditions such as `!cancelled() && steps.install.outcome == 'success'`; retain their normal failure outcomes so the job remains failed. Do not use `continue-on-error` to turn failures green. If installation fails, dependent checks must show skipped, not passed.

Extend `scripts/ci_workflow_test.sh` to validate that the executed workflow uses the tested classifier, includes its tests, and preserves independent web checks. Retain its existing gitleaks and SQLx assertions. Review the generated job conditions as well as the shell behavior. If a workflow YAML parser is already available, use it; do not add a new application dependency for a string check.

Verify: `bash scripts/ci_workflow_test.sh` exits 0. Under Node 24, run `npm ci` in `apps/web`, then run `npm audit --audit-level=moderate`, `npm run typecheck`, `npm run lint`, and `npm run test` as separate commands and record each outcome. A known audit failure is an audit failure, never proof the tests failed or passed. When a PR run is authorized, verify its actual checks show the expected jobs; local static checks alone do not prove GitHub scheduling.

### 4. Hand off a safe verification baseline

Verify `scripts/cargo-test -p mxr-protocol --tests` and `cargo build -p mxr` exit 0. Review `git diff --check` and the changed-file list. Record local results and eventual CI run URL separately. Do not push, dispatch CI, or open a PR without authorization.

## Done criteria

- [ ] The wrapper performs no process discovery or termination; wrapper regression tests pass.
- [ ] Bridge/protocol edits select both Rust and web in the same classifier CI executes.
- [ ] Missing base cannot silently omit web; filter tests run independently of the filter.
- [ ] A failed audit does not hide the typecheck, lint, or test outcomes.
- [ ] Existing release script tests, protocol tests, and build pass.
- [ ] Only in-scope files changed and the index records review/integration evidence.

## STOP conditions

Stop and report if the fetched implementation already fixes this, a required status-check name must change, an out-of-scope release behavior must change, or a focused verification fails twice after a reasonable correction. Do not fix an audit failure by lowering its severity threshold, killing processes, or upgrading unrelated packages.

## Maintenance notes

Branch: `codex/011-safe-verification-and-ci`. Use conventional commits without attribution footers. Load `.agents/skills/mxr-development/SKILL.md` before implementation. Reviewers should scrutinize the script-to-workflow wiring and false/skip outcomes, not just green fixture tests. This plan is the prerequisite for every remaining polish plan.
