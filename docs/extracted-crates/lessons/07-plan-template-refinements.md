# Plan template refinements

What to change in the migration plan when copying [`02-mail-threading-external-repo.md`](../implementation/02-mail-threading-external-repo.md) as the next crate's template.

That doc is ~90% reusable. Below are the specific edits to apply for `list-unsubscribe`, `gmail-query`, or any future extraction.

## Mechanical name swaps

```bash
sed \
  -e 's/mail-threading/<crate>/g' \
  -e 's|planetaryescape/mail-threading|planetaryescape/<crate>|g' \
  -e 's|crates/mail-threading|crates/<crate>|g' \
  docs/extracted-crates/implementation/02-mail-threading-external-repo.md \
  > docs/extracted-crates/implementation/<NN>-<crate>-external-repo.md
```

Verify nothing in the candidate's actual API names collides with `mail-threading`.

## Add a Phase -1 section

`mail-threading`'s plan started at Phase 0 (in-tree validation). Next plan should start at Phase -1 (preflight), which captures the checks from [`01-preflight-checks.md`](./01-preflight-checks.md):

```markdown
## Phase -1: Preflight checks

Run before writing or executing this plan.

- [ ] `cargo search <crate>` confirms name is available on crates.io
- [ ] `rg "use mxr_" crates/<crate>/src` is empty (no internal coupling)
- [ ] No `publish = false` in `crates/<crate>/Cargo.toml` (or remove it)
- [ ] MSRV chosen explicitly for this crate (may differ from workspace 1.88)
- [ ] `categories = [...]` slugs verified against https://crates.io/category_slugs
- [ ] `cargo publish --dry-run -p <crate> --allow-dirty` passes from `mxr/`
- [ ] Working on dedicated branch from `main` (not a dirty release branch)
```

If any box doesn't tick, fix it before continuing.

## Merge Phase 4 and Phase 5 — tag is publish

The original plan ordered:

1. Phase 4: `cargo publish` locally
2. Phase 5: `git tag v0.1.0 && git push origin v0.1.0`

With the CI-driven publish pattern (see [`04-ci-and-publish-pattern.md`](./04-ci-and-publish-pattern.md)), these collapse:

```markdown
## Phase 4: Publish via CI on tag push

Prereqs:

- Repo secret `CARGO_REGISTRY_TOKEN` set (see lessons/04 for walkthrough).
- `.github/workflows/publish.yml` present.

Trigger the publish:

```bash
git tag -a v0.1.0 -m "v0.1.0

Initial release of <crate>. <one-line summary>."
git push origin v0.1.0
```

Watch:

```bash
gh run watch --repo planetaryescape/<crate> $(gh run list --repo planetaryescape/<crate> --limit 1 --json databaseId --jq '.[0].databaseId')
```

Verify:

```bash
cargo search <crate> --limit 3
curl -sI https://docs.rs/<crate>/0.1.0 | head -1   # 302 = built
```

(Phase 5 absorbed — the tag *is* the publish.)
```

Delete the original Phase 5 section. Renumber what follows.

## Replace `cargo update -p` with `cargo check -p`

In Phase 6 cutover, the plan said:

```bash
cargo update -p mail-threading
```

This fails with `package ID specification did not match any packages` because the lockfile still has the in-tree entry. Replace with:

```bash
cargo check -p <consumer-crate>
```

Any build command re-resolves and picks up the registry version. `cargo check` is the cheapest.

## Add `git add -p Cargo.lock` warning

Add a callout in Phase 6 cutover for dirty-branch users:

```markdown
> **Note:** if your branch has other uncommitted work, `Cargo.lock` will
> have collateral changes outside the `<crate>` hunk. Use `git add -p Cargo.lock`
> to stage only the source/checksum lines for `<crate>`. On a clean branch this
> warning doesn't apply.
```

## Add signed-tag form

Update the Phase 5 (or merged Phase 4) tag command to use the signed annotated form:

```bash
# wrong — fails if commit.gpgsign + tag.gpgsign are set
git tag v0.1.0

# right
git tag -a v0.1.0 -m "v0.1.0\n\n<release notes>"
```

The plan currently has the wrong form.

## Add docs.rs verify step

The plan's verification block lists crates.io but not docs.rs. Add:

```bash
curl -sI https://docs.rs/<crate>/0.1.0 | head -1
# expect: HTTP/2 302 (redirects to /<crate>/0.1.0/<crate_underscore>/)
```

docs.rs builds within 1–10 minutes of publish. If the redirect isn't there after 30 minutes, check https://docs.rs/crate/<crate>/0.1.0/builds for compile errors.

## Drop the `gh repo create --source=.` alternate

The plan presented two options for `gh repo create`:

```bash
# option 1 (awkward from mxr/)
gh repo create planetaryescape/<crate> --public --source=. --remote=<crate> --push=false

# option 2 (cleaner)
gh repo create planetaryescape/<crate> --public
```

Option 1 was awkward — it adds a remote to `mxr` named after the crate, which is confusing. Option 2 is what we actually used. Drop option 1.

## Add description on repo create

The plan's `gh repo create` doesn't set a description. Add `--description`:

```bash
gh repo create planetaryescape/<crate> --public --description "<one-line summary>"
```

This shows up on the GitHub repo card and in search. Worth setting.

## Add a workflow security note

When adding `.github/workflows/publish.yml`, the GitHub Actions security hook will warn about injection. The pattern we use (env vars, not inline `${{ secrets.X }}` in `run:`) is correct. The warning is informational — proceed.

## Rollback plan tweaks

The original rollback section assumed local publish. With CI publish:

```markdown
### If the publish workflow fails

- Workflow re-runs are idempotent; click "Re-run failed jobs" in GitHub Actions.
- If the failure is a code issue, fix it on main, push, then push a new tag (e.g. v0.1.1) — do not retag v0.1.0.
- If `cargo publish` succeeded but a later step failed: the crate is live. Cannot unpublish. Proceed to Phase 6.

### If the tag was pushed but CI didn't trigger

Trigger manually:

```bash
gh workflow run publish.yml --repo planetaryescape/<crate>
```
```

## Update the final checklist

Tick boxes that the template carries as `[ ]`. The shipped doc should land at `[x]` everywhere.

Also extend the checklist with:

```markdown
- [ ] `.github/workflows/publish.yml` present
- [ ] `CARGO_REGISTRY_TOKEN` secret set in the standalone repo
- [ ] `v0.1.0` tag pushed and publish workflow green
```

These weren't in the original because the original assumed local publish.

## Estimate budget

`mail-threading` actuals (excluding waiting for user secret setup):

- Phase 0 (in-tree validation): ~5 min
- Phase 1 (repo create + push): ~3 min
- Phase 2 (manifest + licenses): ~5 min
- Phase 3 (CI + dry-run): ~5 min
- Phase 4 (tag + publish via CI): ~2 min
- Phase 6 (mxr cutover): ~5 min
- Phase 7 (docs cleanup): ~10 min

Total: ~35 minutes of active work, plus CI wall-clock (~40 sec per workflow run) and the user's secret-setup time.

Next crate should be similar or faster — the template + reusable artifacts eliminate most of the unknowns.
