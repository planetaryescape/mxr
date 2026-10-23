# 02: Every promise needs an owner, evidence and a release boundary

Use the [claim register](../../research/website-refresh-2026-10-23.md) before drafting public copy. The store, permission and workflow statements are code-backed; a successful code read is not a completed user demonstration. (D-WEB-018, D-WEB-019, D-WEB-022)

## Keep a small review record beside the planning docs

The research register is Markdown, not a new CMS or runtime schema. For a material claim changed during implementation, append these fields to its row or supporting paragraph:

| Field | Meaning |
|---|---|
| Claim | The exact assertion the site may make |
| Evidence | Repository ref, full SHA, path and lines; primary external URL if needed |
| Status | `code-backed`, `runtime-observed`, `main-only`, `branch-only`, `proposal`, or `unsettled` |
| Boundary | Sync completeness, account scope, provider support, model configuration or other necessary condition |
| Public owner | The canonical page explaining it |
| Proof | Fixture command, UI observation or recorded output, when applicable |
| Verified | Date and version/ref actually checked |

These fields are a writing/review convention. Do not build a claims database, new publishing system or generator merely to hold them. (D-WEB-023)

## The main promises connect a job to a mechanism

| Promise | Mechanism to explain | Necessary limit | Canonical owner |
|---|---|---|---|
| Work across mail history | Synced account history, local store and index | Only synced and permitted mail; initial backfill takes time | Why mxr; First sync |
| Compose your own workflow | Consistent search/read/draft/attachment/mutation capabilities | Interfaces differ in coverage; account scope follows each step | For agents; Automation contract |
| Keep human and agent work together | Shared daemon-backed draft and mailbox objects | Same runtime/workspace; linked provider drafts have provider-specific support | Linked drafts; Architecture |
| Choose how to use mail | Five modes and Now over common state | Modes are workflows; they are not five separate account stores | Email modes |
| Control agent authority | Identified agent/MCP profiles, account scopes and send/destructive gates | OS/sandbox boundaries still apply; ordinary CLI origin differs | Security & Privacy; For agents |
| Choose model use | Core mail, external agent, native model features and built-in assistant are separable | Cloud configurations transmit selected context; native calls need their own backend | LLM features |
| Work offline | Synced mail, local search and saved drafts | Sending/sync need the provider; uncached attachments need download | Why mxr; First sync |
| Inspect and recover changes | Preview, applicable activity and supported undo | No universal rollback; logging is optional and bounded | Automation contract; Activity log |

## Wording follows actual semantics

Use “rank subscription candidates by read rate” in exact examples. Explain `opened_count` as messages currently marked READ in that sender/account bucket. Do not promise to know how often someone actually opened a newsletter, time spent reading, or a cross-client behavioural history. `--rank` sorts the returned set; expose its `--limit` and check the candidate coverage before saying “all”.

Use “activity records supported actions with their client origin when enabled”, not “every action is logged”. A plain CLI request is attributed to `cli`; it does not become `agent` merely because an agent ran the command. `ClientKind` is a protocol/client identity, not proof of who was at the keyboard.

Use “preview changes and undo supported mutations” rather than a global promise that everything is reversible. Previewed send content needs the current review identity/revision; an already accepted delivery cannot be promised recall.

Use “one mailbox across clients”, not “all clients have the same permissions”. They share daemon truth; their authority and interface coverage can differ.

## Reader-facing pages keep one job

Tutorials take a newcomer to a result. How-to pages solve one task. Explanation pages establish a mental model and link to a runnable example. Reference pages expose exact mechanics. The homepage is an entry and demonstration surface; it does not need the whole command inventory.

Titles and descriptions name the actual job. A concept page may have a noun title when that is clearer; a how-to title should use the action. Do not mechanically re-title every reference page into an imperative. (D-WEB-022)

Unresolved questions: S1 settles release status and S2 settles the exact breadth of the first corpus demonstration.
