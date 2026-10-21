# Rules should control classification without turning modes into folders

Proposal for BK, 9 October 2026. Research and design only; no product changes. Code observations refer to freshly fetched `origin/main` and the isolated `codex/classification-design` worktree at `163bf2d7301a4d9759761ecc0c5bf2eb462c332a` (v0.6.59). External sources were checked on 9 October 2026.

## Give classification a result type and expose it through rules

Reuse mxr's rule conditions and rule management. Add a typed `set_treatment` action and a daemon-owned classification resolver. A classification is the answer to how mxr should treat a message; a rule is one source of that answer. Manual corrections and built-in heuristics are other sources. This needs a domain contract, not a second general rule engine.

Keep three concepts distinct:

| Concept | User question | Example |
| --- | --- | --- |
| Mode | What kind of work am I doing? | Reading, To do |
| Category or saved view | What is this mail about? | Finance, School, Project Atlas |
| Rule | When should mxr make a decision or take an action? | Bank statements should go to Updates |

Let users define sorting rules for the existing modes. Let them create named groups through labels and saved searches. Do not expose an arbitrary sixth mode merely by adding a name to an enum: it would need a unit of work, actions and completion semantics. A Finance group can contain a conversation, a bill to pay and a receipt without forcing them into one workflow.

## The five modes already have different semantics

The following findings are verified in the named commit, rather than inferred from the UI names.

| Finding | Evidence |
| --- | --- |
| Mode membership is computed and may overlap; the five values are fixed. | [protocol/modes.rs, lines 1–39](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/protocol/src/types/modes.rs#L1-L39) |
| Messages, Updates and Reading map to sender treatment; To do and Archive do not. | [mail_kind.rs, lines 166–174](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/daemon/src/handler/mail_kind.rs#L166-L174) |
| A move to To do creates a task; a move to Archive files a record. | [arrivals.rs, lines 991–1065](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/daemon/src/handler/arrivals.rs#L991-L1065) |
| Explicit message moves and sender decisions already precede heuristics. | [mail_kind.rs, lines 387–404](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/daemon/src/handler/mail_kind.rs#L387-L404) |
| The rules language already has composable conditions, but no classification action. | [conditions](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/rules/src/condition.rs#L4-L56), [actions](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/rules/src/action.rs#L4-L26) |
| The engine collects every matching rule's actions, rather than selecting a winning classification. | [engine.rs, lines 34–71](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/rules/src/engine.rs#L34-L71) |
| The existing `archive` rule action removes provider INBOX membership. It does not invoke record filing. | [loops.rs, lines 1480–1514](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/daemon/src/loops.rs#L1480-L1514) |
| Unsaved rule previews search up to 50 messages; Apply now runs browser-side action interpretation. | [RuleEditorRoute.tsx, lines 105–157](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/apps/web/src/features/rules/RuleEditorRoute.tsx#L105-L157) |
| Saved searches already have stable IDs, names, optional account scope, queries and position. | [core/types.rs, lines 1978–1991](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/core/src/types.rs#L1978-L1991) |

Current sender corrections have a further nuance: a later explicit sender move can supersede earlier message moves, and a later message move can override it again ([regression test](https://github.com/planetaryescape/mxr/blob/163bf2d7301a4d9759761ecc0c5bf2eb462c332a/crates/daemon/src/handler/tests/arrivals.rs#L494)). Re-evaluating an automatic rule must not masquerade as a new manual correction and acquire that power.

## Use familiar correction UX, then reveal the rule

Start from an email, with three distinct choices: change this email; change this sender's sorting; create a rule for similar mail. A one-email correction stays immediate and undoable. Creating a rule opens an editor prefilled from the selected email; the user chooses which facts matter instead of accepting an opaque learned policy.

Example:

> When sender is statements@example.bank and subject contains “statement”\
> Treat as Updates\
> Account: Personal\
> Apply to new arrivals; optionally include existing matching mail.

The action picker should use verbs that describe the consequence:

| Choice | Meaning |
| --- | --- |
| Treat as Messages / Updates / Reading | Change the base treatment; this alone does not create a task or record. |
| Create a to-do | Create a durable task linked to the email; preview its title and any due-date source. |
| File in Archive | Create or associate a durable record; expose any inbox consequence from the existing mode lifecycle. |
| Apply label | Add a topical category using the existing provider-backed label operation. |
| Archive email | Existing mailbox action, described as removing the email from the provider inbox. |

These can compose where meaningful. “Updates, and create a to-do” is a useful instruction. “Move to one of five mutually exclusive buckets” is not an accurate representation of mxr.

Preview must show the current and proposed treatment, other modes that continue to hold the email, tasks/records to be created or reused, provider mutations, winning rule, overridden matches, and personal corrections that will be preserved. Show full impact counts only after a complete scan; label samples and incomplete results honestly. Default new rules to future arrivals, with existing mail as an explicit, bounded selection. Keep current sender-wide actions' retroactive scope visible rather than silently changing it.

Every classified email should offer “Why here?” with the effective correction/rule/fallback and a route to edit it. In Rules, use a Sorting filter over the same rule list. Do not introduce a second competing settings system.

## Reuse the rule schema and distinguish effects in the daemon

Proposed API shape, not an existing endpoint contract. Keep the existing condition AST and `actions` array. Add account scope and optimistic revisions to rule writes; add a typed action rather than a string such as `move:Reading`.

```json
{
  "name": "Bank statements",
  "enabled": true,
  "priority": 100,
  "scope": { "account_ids": ["personal-account-id"] },
  "conditions": {
    "type": "and",
    "conditions": [
      { "type": "field", "field": "from", "pattern": { "type": "exact", "value": "statements@example.bank" } },
      { "type": "field", "field": "subject", "pattern": { "type": "contains", "value": "statement" } }
    ]
  },
  "actions": [
    { "type": "set_treatment", "treatment": "updates" }
  ]
}
```

The account value is illustrative. `set_treatment` accepts only `messages`, `updates` or `reading`. Future typed `create_todo` and `file_record` actions use the same rule surface but call the corresponding domain operations. Do not overload the existing `archive` action, change its meaning or invent `set_treatment: todo`.

Evaluate predicates once against an immutable input snapshot. Preserve per-action rule provenance rather than flattening it away. Resolve classification proposals separately from executing ordinary actions. The highest-priority matching classification proposal wins, with a documented stable ID tie-break; matching label or mailbox actions retain the existing all-matches behavior. A lower-priority rule can therefore have its classification shadowed while its label action still runs, and preview must say so. Prefer explicit ordering controls to inferring that a regex is “more specific.”

For the first implementation, preserve current effective manual message/sender corrections ahead of new automatic classification rules, then use built-in heuristics as fallback. Retain the existing chronology of explicit sender versus message corrections. A conflicting “Always this sender” preference must appear as a blocking preference in preview, with an explicit replacement path; never claim that changing numeric rule priority overrides it. Do not silently migrate sender screening/acceptance while changing sorting. If this proves cumbersome, integrate the routing portion of sender preferences into ordered rules in a separate migration with parity tests.

The resolver must respect blocked, spam, trash and existing eligibility boundaries; a sorting rule cannot silently restore excluded mail. Missing body data is pending, not evidence that a body predicate is false. Classification predicates should use source facts, not their own derived treatment, to avoid circular evaluation. A body fetch can finish classification later without rerunning provider mutations or shell hooks.

A daemon classification result should identify message/account, effective treatment, source type and ID, source revision, and shadowed or protected decisions. Resolve actual mode membership from this result plus tasks, records and done marks. Persist rule-derived decisions/provenance where needed for reproducibility and history, not a second authoritative array of mode memberships. Keep derived decisions separate from manual correction records.

### Preview and apply must use the same planner

Add typed daemon IPC requests, with CLI JSON/JSONL and thin web adapters. Proposed operations under the existing `/api/v1/platform/rules` namespace:

- `POST /preview`: candidate rule, expected rule-set revision, account scope and bounded history selection. Return resolved changes, exclusions, pending inputs, complete-or-partial counts, samples and a preview token.
- `POST /commit`: preview token, idempotency key and expected revisions. Atomically save the rule and enqueue any explicitly selected historical application. Return rule revision and a job ID when history work is asynchronous.
- Read job progress/results through the daemon, with per-message applied/skipped/failed outcomes. Extend the explanation surface with classification provenance.

Bind the token to the candidate, selected messages and relevant revisions. Reject a stale rule definition or changed manual correction with a conflict requiring a fresh preview; do not expand the historical selection between preview and apply. Future arrivals naturally fall outside that selection and use the newly enabled rule. A large history job rechecks per-message versions and reports skipped changes rather than overwriting concurrent user work.

Creating or editing a rule affects future arrivals by default. Historical reclassification is explicit. Disabling/deleting stops future use; previously applied results retain their recorded revision until the user requests a previewed reclassification. Explain that distinction in the UI. Reclassification runs classification effects only; it must not replay unrelated trash, forwarding or shell actions.

Rule-derived task creation needs durable execution identity across retries and rule revisions. Reuse existing domain deduplication and add an effect ledger where necessary: completing/dismissing a task must not allow a replay to recreate it. Disabling a rule must not delete tasks or records, and automation must not overwrite user edits. Keep automatic task creation, as BK requested; do not add automatic task closing.

Expose supported action capabilities. Older editors must show unsupported actions read-only rather than round-tripping them through a lossy string form. Keep old rules and their existing side effects compatible; this proposal does not require converting every rule to a new DSL.

## Custom categories should compose with modes

For “Finance”, “School” or “Project Atlas”, reuse saved searches for dynamic groups and existing labels when explicit tagging is wanted. Put these under a separate Views section; a message may appear in several. A group can span modes and remain useful after an individual task is completed.

Provider labels are not the same as mxr-local classification metadata: today the label action issues a provider mutation. The UI must disclose this. A search-only view can group mail locally without such a write. If the requirement includes arbitrary manual categories shared across accounts but never written to providers, that would justify a small local category entity with stable IDs and message associations. Do not pretend current provider labels already provide that contract, or add that new primitive before the requirement is established.

Keep the RFC message untouched. JMAP explicitly separates mailbox/keyword metadata from parsed message content; this supports keeping mxr classification in local metadata. Export it through mxr APIs. Provider label/keyword mappings, if wanted, should be explicit adapters rather than implicit synchronization of the five modes.

## Comparable products support the separation, not one universal model

| Source | Verified behavior | Design implication for mxr |
| --- | --- | --- |
| [Gmail filters](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.settings.filters) and [labels](https://developers.google.com/workspace/gmail/api/guides/labels) | Filters have criteria and actions and operate on messages; labels are separate many-to-many metadata. | A then-action can produce classification, but the classification must exist independently of the rule. |
| [HEY Paper Trail](https://help.hey.com/article/787-paper-trail) | Users choose a sender destination or move one email; sender changes also move existing mail. | Expose the correction scope and retroactive effects. |
| [Shortwave Splits](https://www.shortwave.com/docs/guides/customize-your-shortwave-settings/) | Search-based views compose labels and senders; the view itself is not a label. | Custom topical views need not become workflow modes. |
| [Fastmail Sieve](https://www.fastmail.help/hc/en-us/articles/360060591373-How-to-use-Sieve) | Conditions cause actions; explicit stop controls whether later rules act. | Resolve competing classification outcomes deliberately rather than inheriting accidental last-write wins. |
| [JMAP RFC 8621 §4.1.1](https://datatracker.ietf.org/doc/html/rfc8621#section-4.1.1) | Mailbox membership and keywords are metadata, separate from parsed message content. | Local classification does not require rewriting an email header. |

These are product and protocol precedents, not evidence that mxr users want every feature those products offer. Broader source notes are in `custom-classification-sources.md`.

## Ship one complete sorting rule before more automation

The user-visible completion check is: create a pattern rule, preview its exact effect, receive matching mail in the intended treatment, inspect why it landed there, correct it, and see that correction survive sync and rule replay. The same behavior must work through the daemon and CLI JSON as through the web UI.

1. Add typed classification proposals, account scope, deterministic resolution and provenance. Test conflicts, current correction chronology, missing inputs and protected mail.
2. Implement daemon candidate preview and bounded apply; use it from CLI and web. Test preview/apply parity, concurrent edits and old-client compatibility.
3. Ship the contextual rule editor and explanations. Test one sender sending both newsletters and urgent replies; two accounts with different desired sorting; and multiple matching rules.
4. Add explicit to-do and record rule actions using domain operations with replay/completion protection. Verify one email can remain in its treatment while gaining a task/record, and rule changes do not destroy those objects.
5. Expose existing saved searches as named views where needed. Add a new local category store only if existing searches/provider labels cannot satisfy the confirmed scope.

Do not require an LLM for deterministic matching. A later natural-language authoring helper could propose a typed rule for review using the same preview; it must not become a separate hidden classifier.

## Unresolved questions are about scope, not whether to reuse rules

- Does “own classifications” include manually assigned, local-only categories across accounts, or is custom sorting plus named views sufficient? The recommended initial scope is custom sorting and existing view primitives.
- Should task/record rule actions ship in the first release or immediately after the complete sorting slice? The target API supports them as distinct operations; sorting should establish trustworthy preview and provenance first.

No implementation has been started. The next action is to settle that product scope, then implement the first sorting slice in a dedicated worktree against a fresh remote main.
