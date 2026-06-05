# Writing docs for mxr

The principles below are the canonical rules for any documentation you
write or change in this repo — both the published user docs in
`site/src/content/docs/` and the internal docs in `docs/`. They exist
because we have one strict anti-goal:

> **No reader should finish a doc and think "I get what this is, but I
> don't know how to make it useful."**

If a page leaves a reader nodding and then unable to act, the page
failed. Everything below is in service of that one rule.

---

## The three non-negotiables

### 1. Every section ends with something runnable

Every section a reader can land on must hand them at least one
copy-pasteable `mxr` invocation they can run right now. Not a synopsis,
not a hypothetical — a real command that produces real output on a real
machine.

**Test:** delete every fenced code block from a section. If the section
still teaches something useful, the section is mislabeled — it's an
explainer, not a how-to. Either add code or move the prose to an
explanation page.

✅ Good:

```bash
# Pin a sidebar lens for threads you owe a reply on:
mxr saved add owed 'is:owed-reply'
```

❌ Bad:

> The `owed-reply` filter is a powerful way to find threads where the
> user is the bottleneck. It integrates with the saved-search system.

The bad version is descriptively true and operationally useless.

### 2. Document every surface

If it ships, it has a doc page:

- Every CLI command (`mxr <verb>`).
- Every flag of every command.
- Every config key (and its default, type, and what it controls).
- Every IPC request and response variant.
- Every saved-search operator (every `is:` / `has:` / field).
- Every TUI keybinding.

The mxr site enforces this mechanically: clap `--help` snapshots are
captured in CI, the CLI reference is regenerated from them, and the
build fails if a snapshot drifts. Don't add a flag without
re-snapshotting; don't hand-edit a generated reference page.

**Where the surfaces live:**

| Surface | Source of truth | Doc page |
|---|---|---|
| CLI commands & flags | `crates/daemon/src/cli/` | Auto-generated `site/src/content/docs/reference/cli/<cmd>.md` |
| CLI examples | `site/scripts/generate-cli-reference.mjs` `COMMAND_EXAMPLES` map | Same |
| Config keys | `crates/config/src/types.rs` | Hand-written `site/src/content/docs/reference/config.md` |
| IPC | `crates/protocol/src/types.rs` | Hand-written `site/src/content/docs/reference/json-output.md` + OpenAPI dump |
| Search operators | `mail-query` crate + mxr custom filters in `crates/search/src/lib.rs` | Hand-written `site/src/content/docs/guides/search.md` + `reference/cli/concepts.md` |
| Keybindings | `crates/tui/src/keys.rs` + `keys.toml` | Hand-written `site/src/content/docs/reference/keybindings.md` |

### 3. Recipes are mandatory where composition is possible

Anything composable — search, mutations, compose, scheduling,
analytics, agents — gets a `## Recipes`, `## In real life`, or `## Use
…` section with real-world scenarios. Not "this is how the flag works"
(that's reference), but "this is an actual job you'd do, here's the
pipeline, here's what you get."

A recipe is **a problem statement plus the smallest pipeline that
solves it plus the output shape**. Filler recipes restate reference
material; real recipes solve a goal.

✅ Real recipe:

> **Find threads where you're the bottleneck, ranked by overdue:**
>
> ```bash
> mxr owed --format json \
>   | jq -r 'sort_by(-.overdue_score) | .[0:20] | .[]
>            | "\(.overdue_score | tostring | .[0:4])\t\(.counterparty_email)\t\(.subject)"'
> ```
>
> Top 20 threads, ranked by `waiting_days / expected_days`. Same set
> as `mxr search 'is:owed-reply'` — pick whichever surface fits your
> script.

❌ Filler recipe:

> ```bash
> mxr owed --format json
> ```
>
> Lists owed replies in JSON.

The filler restates `--help`. The real one ranks, slices, formats, and
tells the reader why the alternative surfaces exist.

---

## Page shape

Every guide follows the same skeleton. Reference pages have their own
skeleton (auto-generated). Explanation pages are looser but still close
with a "See also."

### Guide skeleton

```markdown
---
title: <Verb-leading, ≤6 words>
description: <What the reader can DO after reading. 8–15 words, action-leading.>
---

<One-paragraph promise: what this page solves. Show the verb early.>

## <Concept block — what the system actually does>

<Prose + a small diagram or table if the model isn't obvious.>

## <First task block — the most common thing>

```bash
mxr ... --flag value
```

What you get: <one sentence describing the output the reader will
actually see — not "JSON" but "a list of thread IDs with overdue
scores".>

## <Second task block>

…

## In real life

- **<Concrete scenario, headline-cased>:** `<one-line command>` — <one
  sentence on why this is the right tool for the moment>.
- **<Next scenario>:** …
- **<Next scenario>:** …

## Agent prompts that work

```text
"<Natural-language task description. Names the exact mxr commands the
agent should use. Includes a guardrail like 'don't mutate without
showing me a dry-run'.>"
```

## See also

- [<Related guide>](/guides/<slug>/)
- [<Related reference>](/reference/<slug>/)
- [<CLI page for the verb>](/reference/cli/<verb>/)
```

### Reference skeleton (auto-generated CLI pages)

Already enforced by `generate-cli-reference.mjs`. The author surface is
the `COMMAND_EXAMPLES` map. Use this shape:

```javascript
<command-name>: {
  use: '<One sentence: when you would actually reach for this. Inline-link related guides.>',
  examples: [
    "<simplest invocation that does something useful>",
    "<the --format json + jq variant>",
    "<the pipe-into-another-mxr-command variant>",   // when composable
    "<the --dry-run variant>",                        // when mutating
  ],
},
```

Three to four examples is the right number. One example is too few
(can't show composition), six is too many (we're restating reference).

### Recipe skeleton

```markdown
### <Question phrased as the user would ask it>

```bash
<pipeline ≤ 8 lines>
```

What you get: <one sentence on the output shape>.

<Optional one-paragraph caveat: when this works, when it doesn't.>
```

---

## Discipline rails

These keep the principles enforceable instead of vibes-based.

### Diátaxis quadrants

[Diátaxis](https://diataxis.fr/start-here/) is real and we follow it.
Every page is one of four kinds; mixing kinds is a review-blocker.

| Quadrant | Mxr location | Smell test |
|---|---|---|
| **Tutorial** (learning by doing) | `site/.../getting-started/` | "By the end, you've sent your first email." |
| **How-to** (goal-oriented) | `site/.../guides/` (most of them) | "How do I X?" Always at least one runnable block. |
| **Reference** (neutral facts) | `site/.../reference/` | Tables, schemas, exhaustive flag lists. Light prose. |
| **Explanation** (context, why) | `site/.../guides/why-mxr.md`, `architecture.md`, `security-and-privacy.md` | Prose-heavy. Diagrams. Decisions. No "Quickstart" section. |

A how-to is allowed to point at an explanation; an explanation is
allowed to point at a how-to. Neither is allowed to *be* the other.

### Voice and conventions

- **Second person, imperative.** "Press `b`." Not "the user may press
  `b`." Not "one presses `b`."
- **Command first, explanation second.** Show what to type before
  saying why.
- **No "click here" / "see above."** Link text names the destination:
  `[CLI overview](/reference/cli/)`, never `click [here](/reference/cli/)`.
- **No hypothetical placeholders alone.** `--to alice@example.com` is
  fine. `--to YOUR_EMAIL` alone is not. Pair placeholders with real
  values, or use a real address (`alice@example.com`,
  `bob@example.com`) consistently.
- **Mutations are dry-run first.** Every example that calls a mutating
  command shows `--dry-run` before `--yes`. Same discipline we expect
  from agents.
- **Write durable fit, not opponent-shaped claims.** Avoid claims that
  depend on the rest of the ecosystem staying still, such as "there is
  no maintained crate for X" or long competitor tables. Prefer what mxr
  or a package does, what contract it follows, what it refuses to own,
  and how the reader can verify it.
- **Name the version surface.** Source version, registry version, lockfile
  version, generated-doc version, and app-consumed dependency version
  can all differ. Say which one a doc is describing, then give a command
  that verifies it.

### Frontmatter

```yaml
---
title: <8 words max, verb-leading where possible>
description: <8–15 words, action-leading. This is the social card and the search-result snippet.>
---
```

Title goes in the sidebar and the page H1. Description does double duty
as the SEO meta description and the page lede. Both must promise
something the reader will be able to do.

### Code blocks

- Always language-tag (` ```bash`, ` ```toml`, ` ```text`, ` ```json`).
- Inline comments with `#` for shell and `//` for JS/TS — they teach
  the *why* of the flag.
- Keep most blocks ≤ 15 lines. Use heredocs only when the body
  legitimately spans multiple lines.
- Output blocks are shown verbatim, including empty-result cases. The
  reader needs to know what success looks like.

### Cross-linking

- **Glossary is canonical.** Every term gets a single definition in
  `site/src/content/docs/guides/glossary.md`. Other pages reference,
  never redefine.
- **Link from concept to verb, verb back to concept.** "See also"
  sections do the round-trip so readers never dead-end.
- **Use slugs, not paths.** `/guides/compose/`, not `compose.md`. The
  build resolves them.

### Callouts

Three flavors. Don't introduce new ones ad-hoc.

```markdown
:::tip[Two flags do most of the work]
`--format ids` and `--format json` are the universal composition primitives.
:::

:::note[Two equivalent forms]
For mxr-on-mxr chaining, every read command that takes a single ID
also accepts `--search QUERY`.
:::

:::caution[Don't run this on production yet]
The reset is irreversible. Always `--dry-run` first.
:::
```

`tip` for shortcuts. `note` for alternate forms or clarifications.
`caution` for "do this or you'll lose data."

### Auto-generated content

The CLI reference under `site/src/content/docs/reference/cli/` is
generated from `--help` snapshots. **Never hand-edit pages there.**

To change a CLI page:

1. Add or change the flag in `crates/daemon/src/cli/`.
2. Run the daemon test suite (`scripts/cargo-test -p mxr --test cli_help`).
3. Accept the new snapshot (`crates/daemon/tests/snapshots/cli_help__*.snap`).
4. Edit `site/scripts/generate-cli-reference.mjs` if the
   `COMMAND_EXAMPLES` map needs new examples.
5. `cd site && npm run generate`.

CI fails on snapshot drift. That's how we guarantee every flag is
documented.

---

## Anti-patterns

### "Lorem-ipsum" reference

Generated stubs like _"The foo method does foo things"_ or _"Returns
the result"_. Worse than no docs — it pollutes search results and
breaks reader trust. Either write a real sentence or use the
`description` field to inherit one.

### Explainer-only pages

A concept doc with zero `mxr` invocations is a smell. Either add a "Use
it like this" section, or move the concept to a section of an existing
guide and let the guide own the verbs.

### Stale screenshots and outputs

When a command's output format changes, every doc showing that output
becomes a lie. Regenerate outputs as part of the change that triggered
them. If a screenshot can't be regenerated automatically, prefer ASCII
tables.

### Time-sensitive package claims

"Latest release" and "mxr consumes version X" are different claims.
When writing about public crates, lead with the command that proves the
local app dependency, then check registry metadata separately:

```bash
cargo tree -p mxr-search -i mail-query
cargo info mail-query
```

Use the crates.io page or API when the exact newest public release matters;
`cargo info` can reflect the local Cargo index/cache.

Do not update mxr docs to say it consumes a newer crate until
`Cargo.toml` or `Cargo.lock` actually changes.

### Mode-mixing

A quickstart that detours into architecture. A reference page that
opens with "Why we chose Tantivy." A how-to that explains the daemon's
event loop. Pick the quadrant. Link to the others.

### Patching stale pages instead of deleting

If a feature is gone, the page is gone. Patching "Note: this no longer
works" is worse than 404 — it wastes the reader's time and leaves
broken links to it from elsewhere. Use the build's link-check to find
the orphans, fix or delete.

### Empty `## See also`

Every guide ends with cross-links. If you can't think of any, the
guide is probably orphaned and needs to be re-anchored before it
ships.

---

## Reviewer checklist

When reviewing a doc PR — or your own draft before opening one — scan
for these seven things:

- [ ] Frontmatter title is verb-leading; description is 8–15 words and
  promises a verb.
- [ ] First paragraph names the verb the reader will perform.
- [ ] Every `## ` section has at least one runnable fenced block.
- [ ] Mutations are shown `--dry-run` first.
- [ ] At least one "What you get" or "In real life" block grounds the
  page in concrete output.
- [ ] One Diátaxis quadrant per page (no mode-mixing).
- [ ] `## See also` links exist and resolve.

If a PR fails any of these, it's not ready. Send it back, don't merge
it with a "we'll fix it later."

---

## Where things live

| Concern | Path | Owner |
|---|---|---|
| User-facing published docs | `site/src/content/docs/` | Anyone shipping a user-visible feature |
| Internal / contributor docs | `docs/` | Same |
| Doc-site framework config | `site/astro.config.mjs` | Anyone adding a new section to the sidebar |
| CLI-reference generator | `site/scripts/generate-cli-reference.mjs` | Anyone adding a new CLI command |
| Doc validator | `site/scripts/validate-docs.mjs` | Run by CI; do not bypass |
| CLI help snapshots | `crates/daemon/tests/snapshots/cli_help__*.snap` | Regenerate with `cargo test cli_help` and accept |

The site builds with `cd site && npm run build`. The build itself runs
`npm run generate` (CLI reference), `npm run validate` (link/anchor
check), `astro build`, then `generate-llms-txt.mjs` to produce the
LLM-friendly site export. If the build doesn't pass, the docs aren't
ready.

---

## See also

- [Diátaxis](https://diataxis.fr/start-here/) — the four-quadrant
  documentation framework we follow.
- [Rust API Guidelines: Documentation](https://rust-lang.github.io/api-guidelines/documentation.html)
  — every public item has an example. We apply the same rule to every
  CLI verb and config key.
- [ripgrep GUIDE.md](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md)
  — exemplar of CLI narrative documentation alongside a man page.
- [`docs/blueprint/19-addendum-docs-site.md`](../blueprint/19-addendum-docs-site.md)
  — the decision record for the doc-site framework and structure.
- [`docs/README.md`](../README.md) — top-level index of internal docs.
- The existing site under [`site/src/content/docs/`](../../site/src/content/docs/)
  — every principle here is reified in one or more pages there. When
  in doubt, copy the shape of [`guides/recipes.md`](../../site/src/content/docs/guides/recipes.md)
  or [`guides/pre-send-safety.md`](../../site/src/content/docs/guides/pre-send-safety.md).
