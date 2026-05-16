# Git mechanics

Subtree split, signed tags, branches, and the one signing footgun.

## `git subtree split` works and scales

```bash
git subtree split --prefix=crates/<crate> -b split/<crate>
```

`mail-threading` was 442 commits scanned, 2 retained for the prefix, ~30 seconds wall clock on mxr-sized history. For larger histories, budget proportionally.

The resulting branch is local-only until pushed. Safe to delete and re-run.

## Push the split branch as `main` of the new repo

```bash
gh repo create planetaryescape/<crate> --public --description "..."
git push git@github.com:planetaryescape/<crate>.git split/<crate>:main
git ls-remote git@github.com:planetaryescape/<crate>.git main
```

After successful push, delete the local split branch:

```bash
git branch -D split/<crate>
```

Then clone the new repo as a sibling directory for the standalone-prep phase:

```bash
cd ..
git clone git@github.com:planetaryescape/<crate>.git
cd <crate>
```

Don't work in a worktree of the original repo for this phase — the standalone is a separate repo, not a worktree.

## Signed annotated tags need `-a -m`

If the user has `commit.gpgsign = true` plus `tag.gpgsign = true`:

```bash
$ git tag v0.1.0
fatal: no tag message?
```

Fix: use annotated form with a message:

```bash
git tag -a v0.1.0 -m "v0.1.0

Initial release of <crate>. <one-line description>."
git push origin v0.1.0
```

Lightweight tags (`git tag v0.1.0` without `-a`) are unsigned. Signed = annotated only. Tag pushes trigger the publish workflow (see [`04-ci-and-publish-pattern.md`](./04-ci-and-publish-pattern.md)).

## Don't bypass GPG signing as a shortcut

Antipattern (don't):

```bash
git -c commit.gpgsign=false commit -m "..."
```

This was used once during the mail-threading run, requiring an amend to re-sign. The right move when you're worried signing might fail is to just try the commit — Cargo/git will tell you if the key is missing.

Recovery if you've already made an unsigned commit:

```bash
git commit --amend --no-edit
```

re-signs with the configured key. The mxr user has `gpg.format = ssh` with `signingkey = ~/.ssh/id_ed25519.pub`.

## Branch hygiene during multi-phase work

`mail-threading` shipped from `release-clean` which had 60+ unrelated dirty files. Consequences:

- `git status --short` was a wall of noise
- Every `git add` had to be explicitly scoped to the right paths
- `Cargo.lock` had to be `git add -p`'d to avoid leaking unrelated lockfile drift
- `cargo publish --dry-run` needed `--allow-dirty` even though our paths were clean

Better default: do extraction work on a dedicated branch from `main`.

```bash
git checkout main && git pull
git checkout -b extract/<crate>
```

The cost is ~30 seconds. Saves all the above.

## Commit boundaries during extraction

Three logical commits land on `mxr`:

1. **Phase 0 prep** — README polish, conformance fixtures, lib clean-ups in `crates/<crate>/`.
   ```
   feat: prepare <crate> for external publish
   ```

2. **Phase 6 cutover** — workspace dep flip, member removal, `rm -rf crates/<crate>`.
   ```
   chore: consume <crate> from crates.io
   ```

3. **Phase 7 docs** — update `docs/extractable-crates/`, `docs/extracted-crates/`, frontmatters, README table.
   ```
   docs: mark <crate> extraction complete
   ```

Don't fold these together. The cutover commit must be reverted-cleanly (rollback plan in the migration template). Mixing docs into it makes that messier.

## Subtree split history retention

The split preserves the history of commits that touched `crates/<prefix>/`. For `mail-threading` that was 2 commits. Other crates may bring more history.

If history is sensitive (force-pushes, fixup commits, internal-only context) consider:

```bash
git subtree split --prefix=crates/<crate> -b split/<crate>
git checkout split/<crate>
git reset --soft $(git rev-list --max-parents=0 HEAD)
git commit -m "feat: extract <crate> as a standalone crate"
```

This collapses to a single root commit. `mail-threading` kept its history intact — the right call for audit but a deliberate one.

## What to clean up after

```bash
git branch -D split/<crate>
```

Don't keep split branches hanging around in `mxr` — they confuse `git branch -vv` and serve no purpose post-push.
