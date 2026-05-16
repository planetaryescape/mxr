---
candidate: humanizer
status: wont-do
decision: wont-do
mxr_source: crates/humanizer/
last_reviewed: 2026-05-16
---

> **Status: won't-do.** Fails the publishing bar
> ([`docs/extracted-crates/lessons/10-publishing-bar.md`](../../extracted-crates/lessons/10-publishing-bar.md)):
> no demand signal, no clear brand, audience too niche. `mxr-humanizer`
> stays internal.

# `mxr-humanizer` — **Skip**

> Deterministic text-analysis heuristics that score for AI-writing
> patterns (inflated symbolism, vague attribution, rule-of-three abuse,
> promotional vocabulary, em-dash overuse, etc.).

## Decision: **Skip**

There is *technically* an unfilled niche here — no Rust crate does this
specifically. But the niche is so narrow and the demand signal so quiet
that publishing would create a crate nobody finds and nobody adopts.
The work is more useful as a private mxr capability than as a public
library.

## What mxr has today

**Source:** `crates/humanizer/`

Public API:

```rust
pub struct HumanizerReport { /* category breakdowns, hits, score */ }
pub enum HumanizerCategory { /* enum of pattern families */ }
pub struct HumanizerHit { /* category, span, snippet, severity */ }

pub fn score(text: &str) -> HumanizerReport;
pub fn writing_constraints() -> Vec<&'static str>;
```

Detects:

- Inflated symbolism ("not just X but Y", "stands as a testament")
- Promotional vocabulary ("seamlessly", "robust", "powerful", "elegant")
- Superficial `-ing` analyses ("highlighting", "underscoring")
- Vague attribution ("many believe", "it is widely held")
- Em-dash overuse
- Rule of three (`A, B, and C` triplets in close proximity)
- Conjunctive-phrase overuse ("furthermore", "moreover", "additionally")
- AI vocabulary (a tracked list)
- Negative parallelisms ("it's not X, it's Y")

Heuristics-only. No ML. Inputs and outputs are plain strings. Coupled
only to `mxr-core` for trivial types.

It is good code. It works. Inside mxr it powers the pre-send safety
check that flags drafts that read like an LLM wrote them.

## Ecosystem state

| Area | Status |
|---|---|
| AI-writing detection (Rust) | Nothing focused. A few generic NLP toolkits exist but none specifically target the "Wikipedia signs of AI writing" corpus |
| AI-writing detection (Python) | Several academic detectors, mostly ML-based |
| Text-analysis crates (Rust) | `whatlang`, `lingua-rs`, `text-splitter` — none cover this |

Technically a gap. But:

- **No public demand signal.** No reddit threads, no GitHub issues, no
  "I wish there was a Rust crate for…" posts identifying this need.
- **The market is fuzzy.** Who uses an AI-writing detector? Editors,
  teachers, content moderators. Their tooling is usually Python/web-based,
  not Rust.
- **Heuristics ≠ classifier.** Real-world AI-writing detection is mostly
  done with trained classifiers. A heuristics-only crate looks dated next
  to GPTZero and similar tools.

## Why ours doesn't justify a separate crate

- **Audience uncertain.** No clear adopters.
- **Branding cost.** Establishing credibility in a fuzzy space requires
  marketing effort disproportionate to the code.
- **Internal value is the win.** The mxr safety check is a useful
  *product* feature. Making it a library doesn't make the product better.
- **Heuristic list is opinionated.** Our patterns reflect the
  Wikipedia-sourced "signs of AI writing" corpus and our own taste. Other
  users may have very different opinions. A public crate would face
  endless "why don't you include / exclude X" issues.

## What we'd be doing

Publishing creates a crate that:

- Few find (no search keyword aligns)
- Few adopt (no obvious use case in the Rust ecosystem)
- We maintain forever
- Doesn't strengthen mxr externally

That's a net loss.

## What to do instead

Keep `crates/humanizer/` as an internal mxr capability. Use the
heuristics inside mxr's pre-send safety. If the safety feature attracts
interest as a *product* feature, the path forward is not "publish a
heuristics crate" but "build a CLI like `vale` for AI-writing detection".
That's a separate project, not a library.

## When to re-evaluate

- If a public demand signal emerges (issues filed against mxr asking for
  the heuristics as a library, blog posts asking for it, an
  open-source editor wanting to integrate it), reconsider.
- If the heuristic list grows into something genuinely opinionated and
  defensible, consider it as a *standalone tool* (CLI like `vale`) rather
  than a library.

## Naming

Not applicable. If we ever did ship something here, it would more likely
be a CLI named `vale`-for-AI than a library.

## TL;DR

Real ecosystem gap, no real audience. Keep it private. Use it inside mxr
where it earns its keep.
