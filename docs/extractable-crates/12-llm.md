---
candidate: llm
status: skip
decision: skip
mxr_source: crates/llm/
last_reviewed: 2026-05-15
---

# `mxr-llm` — **Skip**

> LLM provider trait + OpenAI-compatible HTTP client. Wraps Ollama, LM
> Studio, OpenAI, Groq, OpenRouter behind a single `LlmProvider` trait.

## Decision: **Skip**

The Rust LLM-client space is well-served. Multiple maintained crates
already cover this. Publishing another one would fragment a crowded
field while offering nothing differentiating.

## What mxr has today

**Source:** `crates/llm/`

```rust
pub trait LlmProvider {
    async fn chat(&self, messages: Vec<ChatMessage>, feature: LlmFeature)
        -> Result<String>;
    // ...
}

pub struct ChatMessage { pub role: ChatRole, pub content: String }
pub enum ChatRole { System, User, Assistant }

pub enum LlmFeature {
    Summarize, DraftAssist, Triage, /* mxr-specific feature tags */
}
```

A trait abstraction with an OpenAI-compatible HTTP client implementation
that talks to Ollama, LM Studio, OpenAI, Groq, OpenRouter. Includes a
`DemoLlmProvider` for offline testing.

The trait is solid. The HTTP client works. Nothing wrong with the code.

## Ecosystem state

| Crate | Maturity | Coverage |
|---|---|---|
| [`async-openai`](https://crates.io/crates/async-openai) | Healthy, ~600K dl | OpenAI + compatibles |
| [`genai`](https://crates.io/crates/genai) | Active | Multi-provider abstraction |
| [`rig`](https://crates.io/crates/rig-core) | Growing | Multi-provider + agents |
| [`llm`](https://crates.io/crates/llm) | Active | Multi-provider |
| [`langchain-rust`](https://crates.io/crates/langchain-rust) | Active | Chains + providers |
| [`ollama-rs`](https://crates.io/crates/ollama-rs) | Active | Ollama-specific |

This is a **crowded, healthy** space. Any new entrant needs to bring
something differentiating.

## Why ours doesn't differentiate

- **OpenAI-compatible HTTP is commodity.** Every crate above does it.
- **Our `LlmFeature` enum is mxr-specific.** `Summarize`, `DraftAssist`,
  `Triage` are email-flavoured concepts. A general LLM crate has no
  business shipping these.
- **No unique provider coverage.** We support a subset of what `genai`
  and `rig` already support.
- **No unique features.** No prompt caching abstractions, no streaming
  helpers beyond standard, no tool-use orchestration, no agent loop.

Stripped of the mxr-specific feature enum, what's left is a thin
OpenAI-compatible client — exactly the third or fourth such crate the
Rust ecosystem has.

## What we'd be doing

Publishing a fourth OpenAI-compatible client is noise. We'd accumulate
"why not use `async-openai`?" / "why not use `genai`?" issues forever.

## What to do instead

Two paths inside mxr:

1. **Keep `mxr-llm` as a workspace adapter.** It encapsulates mxr's
   feature-tagging concept (`LlmFeature`) on top of a chosen upstream
   client. Fine to stay private.

2. **Migrate to an existing crate underneath.** If `genai` or `rig`
   already covers our needs, replace our HTTP client with theirs and
   keep `mxr-llm` as a thin mxr-flavoured wrapper around it. Reduces
   our maintenance surface.

Either way, no public crate to maintain.

## What we *could* publish (but probably shouldn't)

The mxr-specific concept of "feature-tagged LLM calls" — where each
call declares which product feature it belongs to so callers can route,
rate-limit, log, or budget per feature — is mildly interesting. But:

- It's a 50-line abstraction.
- It's not obviously useful outside agent-style apps.
- Nobody is asking for it.

Not worth a crate.

## When to re-evaluate

- If we develop a *materially* differentiated capability — for instance,
  a smart cost-budgeting middleware, a feature-aware streaming router, a
  cross-provider tool-call normaliser — and that capability is broadly
  useful, reconsider then. Not before.

## Naming

Not applicable.

## TL;DR

Crowded space. We add nothing new. Use an existing crate underneath if
you want; keep our abstraction private.
