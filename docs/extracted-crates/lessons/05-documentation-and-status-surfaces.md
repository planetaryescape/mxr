# Documentation and status surfaces

Where to update what when shipping a crate. The two-doc structure, the three status surfaces, and the planning-doc preservation rule.

## The two-doc structure

Every extractable crate has two parallel docs:

```text
docs/extractable-crates/<NN>-<topic>.md
  → why this should exist, ecosystem analysis, API sketch, dual-publish strategy

docs/extracted-crates/implementation/<NN>-<topic>.md
  → in-tree implementation plan: target layout, public contract, tests, milestones
```

After publishing, move the candidate doc to `docs/extractable-crates/done/`
so the active candidate list stays readable.

For `mail-threading`, those are `done/02-jwz-threading.md` and
`01-jwz-threading.md` respectively.

Once the crate is split to its own repo, a *third* doc joins them:

```text
docs/extracted-crates/implementation/<NN>-<crate>-external-repo.md
  → the migration runbook (Phases 0–7)
```

For `mail-threading`, that's `02-mail-threading-external-repo.md`. This doc is the executable plan — it's what an agent or human follows step-by-step.

Keep all three. They serve different audiences:

- Audit (why we extracted): `extractable-crates/` or
  `extractable-crates/done/` after publish
- Architecture (what we built): `extracted-crates/implementation/<NN>-<topic>`
- Process (how we shipped it): `extracted-crates/implementation/<NN>-<topic>-external-repo`

## Three surfaces to update on ship

When a crate goes from "in-repo" to "published":

1. **Frontmatter** of the candidate doc
   ```yaml
   ---
   candidate: <crate>
   status: published
   decision: shipped
   external_repo: https://github.com/planetaryescape/<crate>
   crates_io: https://crates.io/crates/<crate>
   last_reviewed: <date>
   ---
   ```

2. **Body status section** — replace the in-progress narrative with a "Status: Shipped" block linking to the external repo and crates.io.

3. **Index README table row** in `docs/extractable-crates/README.md` — flip the Decision cell from "Tier 1 — ship" to "Shipped", append links.

Reader-priority order: 3 → 2 → 1. The table is what skimmers see first. Update it last (so it's right) but verify it first when reviewing the diff.

## Status banner at top of planning docs

Don't delete the planning docs after shipping. Add a banner instead:

```markdown
> **Status: complete.** This document captured the in-repo phase. The crate
> was subsequently extracted to its own repository at
> [`planetaryescape/<crate>`](https://github.com/planetaryescape/<crate>) and
> published to crates.io. See
> [`<NN>-<crate>-external-repo.md`](./<NN>-<crate>-external-repo.md) for the
> extraction phase. Kept here as historical context.
```

Reasons:

- the plan is auditable artifact for "why did we do it that way"
- the next extraction's plan template is a copy-paste of the previous one — keeping it makes the template easier to find
- removed-context tends to be reinvented poorly

## Tick the checklists

The migration plan ends with a final checklist. Tick it:

```diff
-- [ ] `planetaryescape/<crate>` exists.
++ [x] `planetaryescape/<crate>` exists.
```

Future maintainers reading the doc can see which boxes are real-shipped vs aspirational-template.

## Frontmatter field conventions

Schema for completed extractions:

```yaml
---
candidate: <crate>
source_doc: ../../extractable-crates/<NN>-<topic>.md
status: complete-extracted-and-published
external_repo: https://github.com/planetaryescape/<crate>
crates_io: https://crates.io/crates/<crate>
last_reviewed: YYYY-MM-DD
---
```

Schema for still-in-progress:

```yaml
---
candidate: <crate>
source_doc: ../../extractable-crates/<NN>-<topic>.md
status: planned | in-progress | implemented-in-repo
last_reviewed: YYYY-MM-DD
---
```

Keep `last_reviewed` accurate — stale dates are how docs go from "trust" to "verify against code first".

## What the `lessons/` directory is for

This directory (`docs/extracted-crates/lessons/`) is the place to capture knowledge that:

- applies to *future* extractions, not the one we just did
- isn't naturally in any one crate's planning doc
- is sharper after the experience than before

It is **not**:

- a recap of what we did (that's the migration doc)
- a status report (that's the README table)
- a per-crate reflection (that's the candidate doc frontmatter)

If a lesson is genuinely crate-specific, it belongs in that crate's body or post-mortem, not here.

## When to re-prune

After ~3 more extractions, audit this directory:

- Are the same lessons repeated in multiple files? Consolidate.
- Has any lesson become wrong? Update or delete.
- Are file names still discoverable? Rename if not.

`mail-threading` is the founder corpus. Don't rewrite it. Add files when the next crate teaches something new.
