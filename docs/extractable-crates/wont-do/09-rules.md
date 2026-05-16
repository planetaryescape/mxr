---
candidate: rules
status: wont-do
decision: wont-do
mxr_source: crates/rules/
last_reviewed: 2026-05-16
---

> **Status: won't-do.** Fails the publishing bar
> ([`docs/extracted-crates/lessons/10-publishing-bar.md`](../../extracted-crates/lessons/10-publishing-bar.md)):
> the natural rival is Sieve (RFC 5228) — a non-Sieve email-rule DSL
> competes confusingly with the established standard. mxr's actions
> (`Snooze`, `ReplyLater`) are also product-shaped, not policy-neutral.
> If anything in this space is ever worth publishing it's a Sieve
> interpreter (~6 weeks), not a custom DSL adapter. `mxr-rules` stays
> internal.

# `mxr-rules` — **Defer**

> Declarative mail-rule engine. Evaluates conditions (glob patterns,
> regex, field matching) and executes actions (archive, label, snooze,
> mark read, forward). Supports dry-run, evaluation context, snooze
> durations.

## Decision: **Defer**

Reasonable code, modest ecosystem opportunity, weak differentiation.
Could plausibly become a published crate, but the priority is below
Tier 1 and Tier 2 picks. Park it.

## What mxr has today

**Source:** `crates/rules/`

```rust
pub struct Rule { /* id, name, conditions, actions */ }
pub struct RuleEngine { /* evaluation context */ }
pub struct DryRunResult { /* preview of actions that would run */ }
pub struct EvaluationResult { /* actions actually executed */ }

pub enum Conditions { /* glob, regex, header, sender, subject, list, label */ }
pub enum RuleAction { Archive, Label, Snooze(SnoozeDuration), MarkRead, /* ... */ }
pub enum SnoozeDuration { /* hours, days, until-date, until-condition */ }
```

A clean condition/action evaluator. Well-encapsulated, tested. Coupled
to `mxr-core` for message flag types and timestamps.

Inside mxr, this powers the "rules" feature that auto-archives,
auto-labels, and auto-snoozes incoming messages.

## Ecosystem state

| Area | Status |
|---|---|
| Generic rule engines (Rust) | Several: `rlsa`, `expression`, `evalexpr`, `rhai` (scripting), `boolean_expression` |
| Email-specific rule engines (Rust) | None published |
| Sieve interpreters (RFC 5228) | None in Rust (some scattered partial impls) |
| Sieve interpreters (other langs) | Mature in C (libsieve), Python (pysieve), Java (jsieve) |

There is a **medium** ecosystem opportunity: an email-specific
rule engine in Rust doesn't exist. But our `Conditions` and `RuleAction`
enums embed mxr-specific action verbs (Snooze, archive, certain mxr
flag semantics). Generic users would have different actions.

## Why this is "defer" not "ship"

Three reasons:

### 1. Mxr-flavoured actions limit reusability

Our `RuleAction::Snooze`, `RuleAction::ReplyLater`, and similar actions
are mxr concepts. A generic mail-rules crate would either:

- Strip these and expose only universal actions (Archive, Label, MarkRead,
  Delete, Forward) — at which point we're shipping a smaller library
  with less code reuse for ourselves.
- Generalise via a custom-action trait — significant API design work.
- Ship them as-is, baking mxr concepts into a "generic" library — bad.

### 2. The natural rival is Sieve, not us

The right way to win this niche is to implement **Sieve (RFC 5228)** —
the IETF standard mail filtering language. A Sieve interpreter would
serve every server- and client-side mail filtering use case in Rust.

Our rule engine is not a Sieve interpreter; it has its own ad-hoc DSL.
Publishing it as `mail-rules` would compete confusingly with whatever
Sieve crate eventually appears.

### 3. Demand signal is quiet

No clear pull from the Rust ecosystem for a "mail rules engine that
isn't Sieve". The Stalwart server uses Sieve. Most server-side filtering
projects use Sieve. Building a non-Sieve alternative is a marketing
problem as much as a code problem.

## What would change the decision

Trigger conditions to move this to Tier 1 or Tier 2:

1. **We implement Sieve.** If mxr's rule engine evolves to interpret or
   compile to Sieve, the result would be the first credible Rust Sieve
   crate — a much stronger ecosystem position.

2. **External adopters appear.** If another Rust mail client wants to
   embed a rule engine and our shape works for them, the audience
   question is settled.

3. **We refactor the action types.** If during normal mxr work we
   factor mxr-specific actions behind a trait (for reasons unrelated to
   extraction), the publication cost drops.

Until one of those fires, leave it.

## Proposed shape (if/when we do it)

Two viable directions:

### Option A — Generic engine, custom actions

```rust
pub trait Action: Send + Sync {
    async fn execute(&self, message: &Message) -> Result<ActionOutcome>;
}

pub struct Rule<A: Action> {
    pub conditions: Conditions,
    pub action: A,
}

pub struct RuleEngine<A: Action> { /* ... */ }
```

User-defined action types. We ship the matching engine. mxr keeps its
own action types.

### Option B — Sieve interpreter

Pivot from "publish what we have" to "build a Sieve interpreter and
publish that". Bigger work; much bigger payoff. Probably the right
answer if we ever do anything here.

## Estimated effort

- Option A: **3–5 days** plus an indefinite tail of "make the API not
  awkward".
- Option B: **3–6 weeks** for a usable Sieve subset. Many months for
  full RFC 5228 + the extensions (RFC 5230 vacation, RFC 5232 imap4flags,
  RFC 5233 subaddress, RFC 5435 notify, etc.). Massive scope.

## Naming

If Option A: `mail-rules` or `mail-filter`.
If Option B: `sieve` (likely taken) or `rsieve` or `sieve-rs`.

## TL;DR

Decent code, weak position. Don't publish as-is. If we ever invest here,
the right move is Sieve, not "another rule DSL".
