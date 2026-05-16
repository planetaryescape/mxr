---
candidate: compose
status: wont-do
decision: wont-do
mxr_source: crates/compose/
last_reviewed: 2026-05-16
---

> **Status: won't-do.** Fails the publishing bar
> ([`docs/extracted-crates/lessons/10-publishing-bar.md`](../../extracted-crates/lessons/10-publishing-bar.md)):
> the `edit` crate already covers `$EDITOR` spawning. mxr's value-add
> here is the frontmatter convention, which is mxr-specific product
> policy. `mxr-compose` stays internal.

# `mxr-compose` — **Skip**

> $EDITOR-based draft composition. Spawn the user's editor with a
> frontmatter-decorated template, parse the result back into structured
> message fields. Includes reply/forward context, signature insertion,
> draft file persistence.

## Decision: **Skip**

The mxr-specific bits (frontmatter DSL, reply/forward context wiring,
signature semantics) are tightly bound to mxr's draft schema. The
generic bit (spawn `$EDITOR` on a temp file, parse the result) is a
hundred lines that every CLI tool re-implements and doesn't need a
crate.

## What mxr has today

**Source:** `crates/compose/`

```rust
pub enum ComposeKind { New, Reply, ReplyAll, Forward }
pub struct ComposeSignature { /* signature handling */ }

pub fn create_draft_file(/* ... */) -> Result<PathBuf>;
pub fn parse_draft_file(path: &Path) -> Result<ParsedDraft>;
```

Capabilities:

- Generate frontmatter-decorated template (mxr's YAML-ish DSL)
- Inject reply/forward context (quoted body, attribution line)
- Embed signature
- Spawn `$EDITOR` (or configured editor)
- Parse the saved file back into structured fields

Coupled to `mxr-core` (types), `mxr-mail-parse` (reply parsing),
`mxr-outbound` (rendering).

## Ecosystem state

| Area | Status |
|---|---|
| `$EDITOR` spawning (generic CLI helpers) | `edit` crate (~150K dl, healthy), used by `git`, many tools. Covers this completely. |
| Email compose flow | None published, but the surface is small enough that everyone rolls their own |

## Why this isn't worth publishing

### The generic part is solved

The `edit` crate already covers "spawn `$EDITOR` on a tempfile, return
the resulting bytes". That's the generic piece. There's nothing to add.

### The email-specific part is mxr-shaped

The valuable parts of `mxr-compose` are:

- **Frontmatter DSL** — mxr's specific YAML conventions for headers,
  attachments, in-reply-to. Other tools would invent their own.
- **Reply context wiring** — depends on the message-parser output shape,
  the quote-stripping conventions, and the signature handling, all of
  which differ between projects.
- **Draft persistence** — mxr stores drafts in a specific layout under
  the app data dir. Other tools have their own conventions.

None of these generalise without significant rework, and the reworked
version would have approximately zero adopters (no other Rust mail
client is asking).

### Audience

CLI mail tools in Rust are rare. The biggest player (`himalaya`) has
its own compose flow. There is no second mover queuing up to adopt
a "compose-in-editor" library.

## What we'd be doing

Publishing a wrapper around `edit` with mxr-specific frontmatter
semantics. Users would (correctly) ask "why not just use `edit` plus a
few helper functions". They'd be right.

## What to do instead

`mxr-compose` stays as an internal workspace crate. If we ever want to
simplify, replace the `$EDITOR` spawning code with the `edit` crate as
a dependency. That removes ~50 lines of subprocess-management code and
adds one dep. Pure win.

## When to re-evaluate

- If the frontmatter DSL becomes interesting enough to other tooling
  (e.g. it grows into a generic "structured-input-from-editor" library
  with broad applicability), reconsider. Currently it's email-specific.
- If a "compose-in-editor" pattern crystallises across multiple Rust
  mail tools, there might be room. Not visible today.

## Naming

Not applicable.

## TL;DR

Generic part is covered by `edit`. Email-specific part is mxr-shaped.
Nothing extractable in between.
