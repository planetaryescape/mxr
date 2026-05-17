# Mail Sync Protocol (MSP)

> A wire protocol between mail clients and per-provider adapter
> binaries. **DAP, but for email.** Inspired by the
> [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/specification).
>
> Initiative status: **early — spike complete, alignment in progress.**

Every mail client reinvents sync. mxr does. So does every Rust mail
client, every Swift mail client, every JavaScript webmail. Each one
wraps Gmail's REST API, IMAP's UID matrix, Microsoft Graph's deltas
independently. The collective person-years of duplicated provider
plumbing are enormous.

DAP showed a way out for debugging: one wire spec, per-language
adapters, every editor speaks one shape. MSP is the same shape for
mail: one wire spec, per-provider adapters, every client speaks one
shape.

This directory holds the spec, the alignment audit (mxr → spec gap
list), a draft blog post pitching the proposal to the broader
community, the verdict from the spike that started the work, and
the roadmap of where we are.

## Why this directory exists

MSP is a multi-month effort that touches mxr's internal
architecture and (if we pursue it externally) creates a separate
spec/repo lifecycle. It's not a single extraction candidate — it's
an initiative with its own roadmap, its own decision gates, and its
own public-facing story.

Consolidating everything under `docs/msp/` so:

- A reader (or you, in six months) can find the whole picture in
  one place.
- The roadmap tracks progress without getting lost across multiple
  doc trees.
- External readers (if/when we publish) have one canonical entry
  point.

## What lives here

| File | What it is |
|---|---|
| [`README.md`](./README.md) | This file. |
| [`ROADMAP.md`](./ROADMAP.md) | **Where we are right now.** Six steps from the spike verdict, each with status. |
| [`spec.md`](./spec.md) | The MSP v0.1 draft (~10 pages, 9 sections). |
| [`mxr-alignment.md`](./mxr-alignment.md) | Gap analysis: mxr today vs MSP. ~10 days of refactor work classified cheap/medium/expensive. |
| [`blog-post-draft.md`](./blog-post-draft.md) | "Mail needs a DAP" — ~2000 word proposal post. Not yet published. |
| [`spike-verdict.md`](./spike-verdict.md) | The original spike outcome and decision to pursue. |

## Related but not here

These live elsewhere because they generalise beyond MSP:

- **[`../extracted-crates/lessons/12-protocol-first-design.md`](../extracted-crates/lessons/12-protocol-first-design.md)**
  — the meta-lesson about protocol-first design as an architectural
  forcing function. Applies to any future "should this be a wire
  protocol?" question, not just MSP.

- **[`../extractable-crates/07-sync-engine.md`](../extractable-crates/07-sync-engine.md)**
  — the original "extract sync as a Rust library" candidate doc.
  Now reframed; the frontmatter points at `docs/msp/`.

## Reading order

If you're starting cold:

1. **[`ROADMAP.md`](./ROADMAP.md)** — what's the current step? what's
   the next gate?
2. **[`blog-post-draft.md`](./blog-post-draft.md)** — the elevator
   pitch. ~10 minutes. Best framing for "what is MSP and why."
3. **[`spec.md`](./spec.md)** — the actual protocol design.
   ~30 minutes if you read it carefully. Section 2 (Abstract model)
   is the most important.
4. **[`mxr-alignment.md`](./mxr-alignment.md)** — how this maps to
   mxr code today. Useful if you want to understand the cost of the
   refactor work mentioned in the roadmap.
5. **[`spike-verdict.md`](./spike-verdict.md)** — historical record
   of how we got here. Useful for "why did we choose this path?"

If you're picking up where the last session left off:

1. Re-read **[`ROADMAP.md`](./ROADMAP.md)**'s "Current focus" section.
2. Skim the changelog at the bottom of the roadmap for what changed
   recently.
3. Continue from there.

## Status snapshot

The initiative is currently in **Step 1 — land spike artifacts** of
the 6-step roadmap. Step 2 (mxr alignment Phase A, the "cheap wins"
refactor) is queued and ready to start.

Nothing has been published externally yet. The blog post is a draft
sitting in this directory; the spec is at v0.1 and has not been
shared outside mxr.

See [`ROADMAP.md`](./ROADMAP.md) for the canonical, up-to-date
status.

## How to contribute

Right now: not open for contributions. The initiative is in its
internal-alignment phase. If you're reading this from outside the
mxr project and want to engage, hold the thought until the blog
post lands (post Step 4 of the roadmap); we'll set up issues or a
working channel then.

If you're contributing inside mxr: pick up the next unblocked task
from the roadmap. Update the status table when you finish a step.
The blog post and spec are working documents — propose changes via
PR.

## License

The spec (`spec.md`) is CC BY 4.0 when published. Other documents
in this directory are MIT OR Apache-2.0 like the rest of the mxr
repo until/unless we move them to a dedicated MSP repo.
