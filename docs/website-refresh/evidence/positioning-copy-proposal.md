# The category improved; the first screen became harder to read

Draft checkpoint, 23 October 2026. This proposal has not changed website source or received copy approval. Independent editorial review did not run: both T3 Claude attempts returned "Claude API rate limit reached. Try again later."

The reader already uses an agent and wants to know why running mxr would be useful alongside a mail connector. The opening should establish local ownership and shared use, then show a bounded result. Installation and the synthetic-mail route should remain visible. The completion check is a reader being able to explain what stays on their computer, what still syncs with the provider, and why their agent, scripts and human clients use the same runtime.

## Keep the approved headline and category

Headline: "Put your whole mail history to work."

Category: "A local mail runtime for you and your agents."

BK approved existing-agent users first and the working-mailbox concept. Owning synced mail locally and reusing the daemon's deep mail operations across clients are required meanings. These choices remain fixed.

## Lead with the mailbox people get

Current website support at candidate `66f36c749ea693516defa7d2be672eff128936e5`:

> A Gmail connector gives your agent access to Gmail. mxr syncs connected mail into a local mailbox, where you, your scripts, and your agents can reuse its mail operations while your provider keeps syncing.

Root proposal:

> mxr keeps a synced mailbox on your computer. Search your history, analyse it with a script, or let your agent work with it. You, your tools and mxr's own clients all use the same local mailbox.

This puts ownership and reader actions before architecture vocabulary. It keeps the sync condition and shared local state. It gives up naming Gmail in the first sentence and does not try to explain network or permission boundaries inside the hero. The fit comparison and first offline example must carry those details nearby.

The research writer's alternative is preserved in the [assessment](<../../research/reports/mxr mailbox positioning review.md>). Root would cut its repeated sync explanation and "build a report or workflow" in favour of a specific action. The synthetic proof should show the actual action rather than asking the hero paragraph to list every possibility.

## Answer the connector question directly after the opening

Proposed fit comparison:

> A connector may be all you need to use Gmail inside your agent. Choose mxr when you also want the synced mailbox on your computer, available to your own tools and to clients you can use yourself.

This is a choice about the combined operating model. It makes no universal claim about connector account limits, caching, round trips, sending or speed. Local mail clients and agent connectors already provide parts of this model; neither local storage nor agent access alone is a unique claim.

## Explain why more clients can share the mail work

Proposed explanation below the first result:

> The daemon handles sync, storage, search and mail operations. The web app, terminal interface, CLI and agent interfaces call it. Build another client without rebuilding the mail engine.

Provider support and interface coverage still differ. This describes reuse, not measured development cost or effortless client implementation. A new client still needs its interaction, error and authority design.

## Give the product more room than the caveats

Keep the approved category, headline, short support, existing demo CTA and install command. Restore a prominent product view using current typography and colours. Replace the synthetic subscription ranking with a captured, non-mutating task using explicit account scope. A CLI capture can stand alone; label it as CLI evidence. Only claim a matching agent result after replaying the same task through an actual agent interface and comparing message identity.

Keep "Try it with demo mail" as the primary CTA. Proposed secondary labels are "Use it with your agent" and "See how people use mxr". Preserve the existing destinations unless a verified route change is necessary. No new widget or client is needed for this slice.

The existing S2 run returned four sender buckets per account, all with one message and zero READ flags. It is valid runtime evidence but weak ranking proof. S3 has not established a shared draft handoff. Do not use either as a captured human-agent review story. Keep five modes and Now in the following human-use section.

## Source and review status

Root refreshed `origin/main` over HTTPS on 23 October 2026. It remains `f39ae66578e3099e464967ab2f042e24b1ad8ec4`; candidate HEAD remains `66f36c749ea693516defa7d2be672eff128936e5`. Root reread local SQLite setup at `crates/store/src/pool.rs:45-106`, search execution at `crates/daemon/src/handler/diagnostics/search_execute.rs:29-96`, CLI origin at `crates/daemon/src/ipc_client.rs:76-96`, and MCP daemon connection at `crates/mcp/src/lib.rs:91-116`, all from the fetched main ref. The existing [claim register](../../research/website-refresh-2026-10-23.md) supplies the wider provider, mutation and client provenance.

Runtime evidence remains [S2](s2-runtime-proof.md). Local availability covers synced content; initial backfill takes time, uncached attachments require downloads, and sync, sending and supported provider changes use the network. Clients share state but can have different permissions. External agents can send supplied context to their configured model services.

The two research notes and final assessment are complete. Root adopts the category-versus-execution distinction and rejects the research writer's repeated provider-sync sentence in the hero. Shared-draft proof and conversion improvement remain unverified. No website source, CSS, deployment, real mailbox or account settings changed during this assessment. No new executable code means TypeScript review, code simplification and application tests are not applicable; the prior candidate's build results remain limited to that candidate.

Next action: obtain the required independent editorial opinion when Claude is available, using this raw proposal, the research assessment and canonical evidence. Then show the current and proposed support to BK for the scoped copy checkpoint before visual production. Existing admin-merge authority remains valid; do not ask for GitHub review or merge permission again.

Unresolved questions: whether this support paragraph is the final wording; which replayed synthetic task earns the first product view; whether an agent replay is available for the same result. No dependent website rung starts before rung 1 deploys.
