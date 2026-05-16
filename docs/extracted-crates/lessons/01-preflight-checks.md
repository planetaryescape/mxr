# Preflight checks (Phase -1)

Before writing a migration plan or picking a target version, run these. They are cheap and surface every blocker that would otherwise be discovered mid-extraction.

`mail-threading` was lucky-clean on most of these by accident. The next crate may not be.

## Name availability

```bash
cargo search <candidate-name> --limit 5
```

If the bare name is taken — even by a stale unscoped npm package — pick a new name **before** writing the plan. `mail-threading` (npm, 2017) was already squatted; the Rust slot happened to be free.

Likely-taken Tier 1 candidate names to verify early:

- `list-unsubscribe`
- `gmail-query`
- `format-flowed`

If taken, candidates: `<name>-rs`, `<scope>-<name>` (e.g. `planetaryescape-list-unsubscribe`), or a domain-anchored rename.

## Internal `mxr-*` imports audit

```bash
rg "use mxr_" crates/<target>/src
rg "mxr-" crates/<target>/Cargo.toml
```

Any hit = pre-extraction refactor work. `mail-threading` was clean (only `chrono` + std). Most workspace crates aren't.

Common offenders: `mxr-core` types, `mxr-store` SQL, `mxr-protocol` IPC types. Those have to move out, behind a trait, or be re-typed before the split.

## `publish = false` flag

```bash
grep "publish" crates/<target>/Cargo.toml
```

`mail-threading` didn't have it. Most `mxr-*` crates do. The standalone manifest must drop the flag (omit it — Cargo's default is publishable).

## MSRV is a product decision, not inheritance

Workspace MSRV is 1.88 (driven by the app). The standalone crate may want lower.

Lower MSRV = wider downstream adoption. Worth it for:

- string parsing crates (`list-unsubscribe`, `format-flowed`)
- algorithm crates (`gmail-query`)

Higher MSRV is fine when:

- you need a recent stabilization (let-chains, GATs, etc.)
- the crate is upstream of newer codepaths anyway

Decide explicitly per crate. Don't blindly copy 1.88.

## Categories must be valid crates.io slugs

The original lesson cited `https://crates.io/category_slugs` (an HTML
endpoint). As of 2026-05 it returns 404 — confirmed during the
`list-unsubscribe` extraction. Use the JSON API instead:

```bash
curl -s "https://crates.io/api/v1/categories?per_page=100" \
  | python3 -c "import sys,json; print('\n'.join(c['slug'] for c in json.load(sys.stdin)['categories']))"
```

Pipe through `grep -i email` (or whatever).

`mail-threading` used `["algorithms", "email"]`; `list-unsubscribe` uses
`["email", "parser-implementations"]`. Both valid. Bad slugs reject
publish at the dry-run gate.

Update the candidate doc's intended `categories = [...]` before the plan locks them in.

## Pre-extraction dry-run from the workspace

```bash
cargo publish --dry-run -p <target> --allow-dirty
```

Cheapest sanity check on the standalone manifest. Catches:

- missing `description`
- missing `license`
- missing `repository`
- excluded files that shouldn't be
- included files that shouldn't be (look at `Packaged N files, K KiB`)
- dependency version specs that are workspace-only

`mail-threading` passed this from `mxr/` even before the subtree split. If it doesn't, fix the in-tree manifest first.

## Spec anchor check

```bash
rg -n "RFC|spec|standard|grammar|ABNF|conformance" docs/extractable-crates/<candidate>.md
```

No anchor = raise the bar. README cannot honestly explain coverage without one. See lesson 3 in the README playbook.

## Consumer count

```bash
rg "use <crate_name>" crates -l
```

If more than one mxr crate imports it, the cutover commit will touch all of them. Plan for that — `mail-threading` was only consumed by `mxr-sync`, which made Phase 6 tiny.

## Branch posture

```bash
git status --short
git log --oneline origin/main..HEAD
```

If the working tree has 30+ unrelated dirty files, the extraction will fight `Cargo.lock` collateral every step. **Do the work on a dedicated branch from `main`.** `mail-threading` happened on `release-clean` and cost ~10 minutes in `git add -p` gymnastics.

Recommended:

```bash
git checkout main && git pull
git checkout -b extract/<crate>
```

## Output

A green preflight produces these answers for the plan doc:

- chosen crate name (verified available)
- decided MSRV
- decided categories (slug-validated)
- consumer crates that need cutover
- list of `mxr-*` deps to scrub before split (if any)
- dedicated branch name
