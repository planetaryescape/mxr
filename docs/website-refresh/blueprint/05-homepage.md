# 05: The first screen must connect a useful result to the local runtime

Rewrite `site/src/content/docs/index.mdx` around the hierarchy in [03](03-design-concept.md), preserving its working install-copy behaviour and useful media infrastructure. The present homepage already has strong details; move and correct them rather than discarding them. (D-WEB-010, D-WEB-012, D-WEB-013)

## The opening answers four questions without interaction

| Question | Content | Location and treatment |
|---|---|---|
| What is it? | “A local mail runtime for you and your agents.” | Visible category line, ordinary text, not a small decorative badge |
| What can I do? | Approved: “Put your whole mail history to work.” | Main headline; “synced history” condition in supporting copy |
| Why not my Gmail connector? | The bounded first paragraph from [04](04-positioning.md) | First supporting paragraph, no click required |
| How do I try it? | Install command and demo/agent links | Opening area; safe trial is the primary narrative continuation |

Supporting copy to refine under the approved direction:

> Sync connected Gmail, Outlook and IMAP accounts into a local mailbox. Query your synced history, build workflows with consistent mail operations, and let your agent prepare work you can review in mxr.

The connector paragraph and this support text must be edited together to avoid repeating the same definition twice. The final opening should stay compact: one category, one headline, one short explanatory paragraph that includes the connector answer, then actions. These are draft words, not a requirement to preserve a count of lines.

Installation remains immediately usable: `brew install planetaryescape/mxr/mxr`, with a link to other supported methods. “Try the demo” lands on the demo section of the quick start, not an unexplained install page. During rung 1, “Use my agent” lands on the existing For agents guide. Rung 2 switches it to the new agent tutorial, whose prerequisites include the binary. No homepage interaction starts a real account connection. (D-WEB-014, D-WEB-015)

## One task proves the story before the feature tour

Use the subscription example first if S2 produces a clear capture. It demonstrates a local aggregate without needing a native LLM call:

```bash
mxr subscriptions --account personal --limit 200 --rank --format json
mxr subscriptions --account work --limit 200 --rank --format json
```

The S2 capture confirms that an omitted account resolves to the configured default. Use one command per account when showing combined results. Captured fixture rows each have one eligible message and zero READ flags; the original illustrative comp values are not observed output. The two-account 55-message results are preserved in [S2](../evidence/s2-runtime-proof.md).

The proof has three steps, with all content readable without JavaScript:

1. Local evidence: a captured, scoped result showing account, message count, READ-derived count and archived-unread count. Disclose the returned candidate limit. Caption it “Synthetic demo mailbox; read state, not tracking opens.”
2. Agent work: the agent identifies candidates and explains its selection using those aggregate fields. Do not imply mxr itself made a model call to compute the counts.
3. Human review: show the proposed unsubscribe/archive selection before any application. In the published capture, stop at preview unless the demo explicitly runs a reversible synthetic action and verifies it.

If a second, small proof is feasible, show the same draft moving from an agent into a real mxr client. This is a separate workflow with its own S3 acceptance check, not a fabricated combined transcript. Keep the first proof legible on a phone; expandable command details are secondary.

The task control is click-driven and keyboard-accessible. Selecting a step highlights the corresponding evidence, runtime or review boundary in the diagram. It must communicate a mechanism; do not add tabs solely to hide prose. (D-WEB-005, D-WEB-012, D-WEB-019)

## The remaining sections answer the next objections in order

| Order | Section job | Exact content requirement | Continue to |
|---|---|---|---|
| 1 | Establish product and trial | Opening and one complete task proof | Quick start / agent quick start |
| 2 | Explain ownership and reusable mail capabilities | Your synced mailbox is on your computer; provider accounts → local runtime → human clients, CLI/scripts, MCP/agent. Clients reuse daemon operations; distinguish skill as instructions for CLI use | Why mxr / Architecture |
| 3 | Show daily use | Now above five modes; one job/action each; one actual human workflow capture | Your day / Email modes |
| 4 | Explain extensibility | Three goal-shaped workflows: subscription review, relationship-informed draft, invoice attachment filing | For agents / Recipes |
| 5 | Make control visible | Bounded profiles, review, pre-send checks, supported undo, local activity, model disclosure | Security & Privacy / Automation contract |
| 6 | Explain local ownership | Synced history usable offline, attachment caveat, provider still receives sync/send/mutations | First sync / Backup |
| 7 | Finish installation | Other supported install methods, direct human/agent next links, release link | Installation |

Do not turn this into seven equal card grids. Use the existing typography/rules/columns language with one dominant object per section. A reader should encounter an answer and a next step, not an encyclopaedia.

## Correct misleading details on touched content

The current agent section says a hosted model call is the only network wait after sync. Replace that with the narrower statement that local search and cached mail reads need no provider request; sync, sends, uncached attachments, remote articles and configured model endpoints can still use the network.

The current surfaces section says all surfaces use the same permissions. Replace that with common daemon state and operation-specific client authority. Plain CLI is not the same profile as MCP. Remove synthetic timings from any retained social/capture narrative unless measured and labelled. Replace touched emoji attachment icons with the site's icon convention.

The mode section must keep all five modes and Now current. Hold the R2 stored-home explanation until S1 verifies its published status. Preserve the distinction between Archive records and archiving provider mail.

## The homepage ends in a real outcome

A visitor can repeat the local subscription query in demo mail, explain why a runtime is more than an agent Gmail connection, and find the relevant human or agent continuation. Record the exact installed version and captured results during implementation. The current planning task has not run this demo.

Unresolved questions: Q1–Q3 are approved. S1 release identity is verified at v0.6.156/f39ae6657; S2 runtime proof is recorded; S4 comprehension remains unobserved. S3 belongs to the later draft handoff.
