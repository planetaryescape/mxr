# Existing-agent alternatives and mxr's positioning

## What existing-agent users can already do

### Takeaway

Gmail access, agent compatibility, search, summaries, and reply drafting are table stakes. The defensible difference is the operating model: mxr syncs mail onto the user's computer, then lets their agent, scripts, and human client reuse daemon-backed mail work. Sell that distinction without implying connectors lack capabilities they document.

### Cited Findings

| Alternative | Position and audience | Features / CTA / proof | mxr trade-off |
|---|---|---|---|
| ChatGPT Gmail app/plugin | “Find and reference emails from your inbox.” ChatGPT users bringing Gmail into a conversation. | OpenAI shows finding mail, extracting details, drafting replies, and summarizing threads. Connect the account, ask for a task, review. Product page uses prompts and workflow illustrations. Multiple accounts are supported for some apps, not universally. | Hosted conversation convenience; no reviewed source establishes connector cache implementation, query coverage, request count, or latency. [Gmail app](https://openai.com/business/plugins/gmail/), [connected apps](https://help.openai.com/en/articles/11487775-connected-apps-in-chatgpt), [account support](https://help.openai.com/en/articles/20001494-connecting-and-managing-app-accounts-in-chatgpt) |
| Codex + Gmail plugin | Gmail plugin: “Find emails, pull out useful details, and draft replies.” Existing Codex users. | Plugins package skills and optional apps; connect Gmail and use it in a task. OpenAI's example shows Gmail + Calendar and a review step. Availability depends on workspace/app controls. | Don't imply Codex can't already access mail. Public docs don't establish a special local-mailbox model for Codex. [OpenAI plugin walkthrough](https://developers.openai.com/training/walkthroughs/plugins-and-skills), [connected apps](https://help.openai.com/en/articles/11487775-connected-apps-in-chatgpt) |
| Claude Workspace connector | “Connect your Gmail, Google Calendar, and Google Drive to Claude so you can search and send emails…” Claude users. | Natural-language search/read, drafts, send/reply/forward, labels/threads, saved drafts, attachment metadata (not content). Send asks approval by default; organization owners can change controls. Connect Google account; docs explain privacy/limits. | Ready in-chat access; connector data is stored by Anthropic with its associated chat. Docs say complex queries may require multiple API calls and performance can vary for large mailboxes. [Claude Workspace connectors](https://support.claude.com/en/articles/10166901-use-google-workspace-connectors) |
| Google Workspace CLI (`gws`) | “One CLI for all of Google Workspace — built for humans and AI agents.” Developers/agents using Google APIs. | Dynamic API CLI, JSON, agent skills, dry-run, Gmail send/reply/triage/watch helpers. Install and configure OAuth; setup may require a Google Cloud project and scopes. README says it is **not** an officially supported Google product. | Closest agent-automation substitute; not evidence of cross-provider mail runtime or packaged human client. [Repository and README](https://github.com/googleworkspace/cli) |
| notmuch | “Just an email system”; local global search and tags for terminal/scripting users and mail-client developers. | Indexes locally stored individual message files; user separately configures mail delivery/sync. CLI, library, and Emacs paths; proof is technical docs/examples. | Local search precedent, but it does not fetch or send mail. mxr can own the joined sync + provider operations + client runtime story, not local indexing alone. [notmuch](https://notmuchmail.org/), [getting started](https://github.com/notmuch/notmuch-wiki/blob/master/getting-started.mdwn), [receiving mail](https://notmuchmail.org/howto/) |

OpenAI says app actions and sync are app-dependent and some apps support multiple accounts; Anthropic documents mail write actions and approval. These sources do **not** substantiate blanket claims that connectors are one-account, uncached, read-only, slow, or unable to draft. `gws` is a CLI, not proof of a unified local mailbox. No verified pricing or review signals.

### Inferences

- The opposite-test claim is user-owned local state vs connected app convenience: a connector can credibly say “use Gmail in this chat”; mxr can say “keep a synced mailbox on your computer and reuse it across your tools.”
- Lead with a concrete job, then the mechanism. Stronger candidates than subscription READ rate: find a supplier's contract history; show mail storage by sender; find stale threads and prepare a follow-up for review. These are supported product jobs in the inspected main baseline, but require a replay before presented as observed output.
- Say that new clients reuse daemon mail operations. Don't promise no client work: each client still needs interaction design, errors, and permission handling.
- Keep external agents, mxr's optional native model features, and its built-in assistant distinct. This is not another model pitch.

### Gaps

No controlled connector comparison measured coverage, request counts, latency, or accuracy. A current Codex-specific Gmail capability table and feature-specific pricing were not established. No competitor user reviews/issues were sampled.

## Is the current homepage direction better than the previous one?

### Takeaway

The candidate is better positioned and worse edited. It names the runtime and answers the connector question, but the first screen now carries a long tagline, an illustrative READ-rate card, and many caveats before the recorded demo. Keep the approved headline/category and make the proof more compact, concrete, and verifiable.

### Cited Findings

- Baseline `origin/main` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4` opens “Talk to your whole inbox,” says history stays on the computer and in sync, offers “Get started” and “Try the demo,” and puts recorded CLI/TUI/agent material nearby. [Baseline homepage](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/src/content/docs/index.mdx)
- Candidate `HEAD` `66f36c749ea693516defa7d2be672eff128936e5`, branch `t3/agent-mail-positioning-review`, uses “Put your whole mail history to work” and “A local mail runtime for you and your agents.” It adds a Gmail-connector explanation, demo/agent/human routes, and a synthetic task card before the existing recordings. [Candidate homepage](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/site/src/content/docs/index.mdx)
- BK's supplied prior-homepage screenshot (local attachment path in closeout) shows a large TUI frame, short supporting copy, and two CTAs. It is not a new render of the candidate.
- The candidate card labels its result an illustration and says it is not observed output. S2's isolated fake-mail run on published `mxr 0.6.156` synced 55 messages/account and returned four subscription buckets/account; every bucket had one message and zero READ flags. The fixture therefore did not demonstrate useful ranking variation. Default scope is configured account; ranking sorts a limited candidate set. [S2 proof](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/evidence/s2-runtime-proof.md)
- At main SHA above, local store/search and common provider operations are code-backed; actual offline use depends on synced/cached content. [Positioning plan](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/04-positioning.md), [claim boundaries](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/02-claims-and-content.md)

### Inferences

- The candidate improves category clarity and routes; the old screen is more restrained and gives the real demo more space. A visitor should not have to parse architecture plus a disclaimer before seeing why the product helps.
- Keep both approved lines. Use one short supporting sentence: “Your synced mailbox stays on your computer. Your agent, scripts, and mxr can work from the same mail runtime.” Put sync/cache limits in a nearby caption or explainer.
- Replace the READ-rate illustration with a single synthetic task: query bounded local history, return verifiable evidence, let the agent propose a next step, and show where the person reviews it. Use captured output and say which account/history is in scope.
- Make “Try it with demo mail” primary, with agent and human paths secondary.

### Gaps

Only source and BK's older screenshot were available; no rendered candidate/mobile preview was inspected. S2 does not establish a compelling agent task or observed draft handoff. Reader comprehension remains unmeasured.

## What positioning should say and prove

### Takeaway

Preserve BK's approved headline, category, and working-mailbox idea. Explain local ownership alongside continued provider sync, then show how clients reuse daemon work. Do not turn the story into “AI email,” a connector takedown, or a READ-rate feature pitch.

### Cited Findings

- BK approved existing-agent users first, the working-mailbox concept, “Put your whole mail history to work,” and “A local mail runtime for you and your agents.” He added that the full synced mailbox is owned on the user's computer while remote sync continues, and daemon capabilities let additional clients reuse sync/provider/search/mail work. [Decisions](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/12-decisions.md), [positioning](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/04-positioning.md)
- “Whole history” must mean connected, permitted, synced content. Backfill takes time; cached bodies and attachment availability bound offline work. Sync, sends, and provider-backed changes still use the provider. [Claims and content](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/02-claims-and-content.md)
- The main-baseline audit shows agent/MCP profile enforcement is specific: ordinary shell CLI remains CLI-origin; activity is optional/bounded; undo is operation-specific. Shared mailbox state does not mean identical client authority. [Main-baseline audit](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/research/website-refresh-2026-10-23.md)

### Inferences

Suggested compact hero:

> **Put your whole mail history to work.**
> **A local mail runtime for you and your agents.**
> Your synced mailbox stays on your computer. Agents, scripts, and mxr's human clients reuse the same mail operations.
> **Try it with demo mail** · Use it with an agent · Use mxr yourself

Then show one synthetic workflow: provider sync → local mailbox → agent/script and mxr clients → reviewed result. Name CLI, MCP, daemon, and adapters after the reader sees the consequence. Proof should show exact input, bounded real output, scope, and review destination.

Cut claims that imply every message/attachment is offline, all accounts are searched by default, connectors universally lack sync/write actions, all clients have equal authority, remote access is automatic, every action is logged, every mutation is undoable, or new clients require no work. Do not describe `opened_count` as actual opens or ranking all senders. “Accurate,” “fast,” “works with AI,” and “better email” fail the opposite test. The differentiator is one synced local mailbox and common mail operations reused by the user's chosen agent, scripts, and mxr's human clients.

### Gaps and closeout

S2's identical one-message buckets are not persuasive READ-rate proof; select and replay a stronger synthetic job. Shared-draft handoff is not an observed journey yet. No provider-wide completeness/attachment matrix should be compressed into an unqualified homepage claim.

Research completed 2026-10-23. Sources: first-party OpenAI/Anthropic pages, Google Workspace CLI repo, notmuch docs, and local mxr artifacts; web pages were checked that day and may change. No mail/accounts/credentials, source/CSS, builds, tests, or deployments were touched. Code-simplifier and TypeScript review are N/A; no generated resources or child processes were created. Read-only source worktree `/Users/bhekanik/.t3/worktrees/mxr/t3-6badd8ca`, branch `t3/agent-mail-positioning-review`; note control worktree `/Users/bhekanik/.t3/worktrees/mxr/docs-runtime-website-control`, branch `docs/runtime-website-control`. Both at HEAD `66f36c749ea693516defa7d2be672eff128936e5`, tree `84ae2f7c34837e84f9b0509fe018fb8b25ff4832`. HTTPS fetch confirmed `origin/main` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4` (`v0.6.156`). Supplied screenshot: `/Users/bhekanik/.t3/userdata/attachments/42b4371f-ed49-4fba-a98c-1f81e4a96dd9-23dc0528-afa4-4fdc-9dc1-7a872512335d.png`. Only written resource: this note.
