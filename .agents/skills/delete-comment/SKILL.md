---
name: delete-comment
description: "Delete comments in a diff, paths or crate: sort each one as tautology, stale, excuse, decision record or keep, then delete, file an issue, or move the decision into the decision log, and lower the comment ratchet."
disable-model-invocation: true
---

# Delete comment

Code says what it does. A comment either repeats the code, goes stale, excuses a workaround, or records a decision the code cannot show. This skill removes the first three, moves the fourth into the decision log, and lowers the comment ratchet (`scripts/comment-ratchet.mjs`) so the removed comments stay gone.

Adapted from the `no-comments` skill and `comment-sicko` agent in [cursor/plugins pstack](https://github.com/cursor/plugins/tree/main/pstack) (MIT, Lauren Tan).

## Scope

Use the caller's paths, crate or diff. Otherwise use the current diff against `origin/main`, including the working tree. Print the counted comments in scope with `node scripts/comment-ratchet.mjs --list <path-prefix>...`. Doc comments (`///`, `//!`, `/** */`) in scope are in scope too, though the ratchet does not count them.

## Categories

Every comment in scope gets exactly one category:

| Category | Test | Action |
|---|---|---|
| Tautology | Repeats what the code, names or types already say. | Delete. |
| Stale | Names code, behaviour, tickets or plans that no longer match the code at the current SHA. | Delete. |
| Excuse | Justifies a hack, workaround, `TODO`, `for now`, `too risky`, a lint suppression or a surprise in our own code. | Delete, and file the fix in `docs/issues/`. |
| Decision record | Records a non-obvious choice and its reason that the code cannot express, and that a reader would otherwise reverse. | Move into `docs/blueprint/15-decision-log.md` as the next `D<n>` entry, then delete. |
| Keep | Matches an exception in [comment-sicko.md](references/comment-sicko.md). | Leave it. |

When a comment is unsure between keep and anything else, it is not a keep.

## Steps

1. Spawn one subagent per scope with the brief in [references/comment-sicko.md](references/comment-sicko.md) and the scope. Give each a disjoint set of files. It edits comments only and returns a report: each comment's `path:line`, category and one-line reason, every `MUST KILL` flag, and every constraint comment.
2. Inspect the report and the diff. Revert any change to code that is not a comment. Check each keep against the exception list; a keep without proof that it is about something we cannot change is deleted. Before accepting a thin `IMPORTANT` or `do not remove` kill or keep, trace its symbol: read the callers, and run `git log -S '<symbol>' --oneline` or `git blame` on the lines. An ambiguous kill stays deleted; an ambiguous keep is deleted. If a report breaks these rules, revert it and rerun that scope once with the failure named. A second bad report fails the run for that scope.
3. File every excuse and every `MUST KILL` you do not fix now as an issue in `docs/issues/`, one file per root cause, named for the problem. Each issue says where the workaround lives (`path` and symbol, not a line number), what it works around, and the smallest fix. Group many instances of one root cause into one issue.
4. Fix trivial `MUST KILL` flags in scope: rename, extract a named function or constant, or replace a magic value with a type. A fix that changes behaviour goes to an issue instead.
5. Move each decision record into the decision log: title, what was chosen, what was rejected and why, and the code location by path and symbol. Merge it into an existing `D<n>` entry when one already covers the decision.
6. Constraint comments (`do not remove`, `do not change wording`, `talk to X before changing`) that are not keeps: offer the cheapest in-scope encoding (a type, an assertion, a test or a CI check). In an interactive run, wait for approval. In an unattended run, encode only with the caller's prior approval. If approved, encode and then delete the comment. Otherwise delete it and file the constraint as an issue.
7. Run `node scripts/comment-ratchet.mjs --update`, then `node scripts/comment-ratchet.mjs`, which must pass. Then prove that only comments changed: `cargo fmt --all` and `cargo check -p <crate> --all-targets` for each touched crate, and `npm run lint` and `npm run typecheck` in each touched app. Run tests only for files where step 4 changed code.
8. Report: comments deleted per category, the ratchet count before and after, lines removed (`git diff --shortstat`), issues filed, decision log entries added, `MUST KILL` fixes, constraint encodings offered and made, reverted reports and reruns, and anything left open.
