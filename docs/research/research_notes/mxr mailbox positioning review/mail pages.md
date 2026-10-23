# Mail product positioning and homepage execution

## Is the proposed mxr homepage better than the previous page?

### Takeaway
The proposal is better at naming what mxr is and why an existing-agent user might install it; it is worse at getting that value across quickly. Keep the approved headline and category, but simplify the first proof around one mailbox and one concrete outcome. The old page's visual demo was stronger as a product-at-a-glance, although its “whole inbox” framing and analytics claims could be read as an email-client/AI pitch.

### Cited Findings
- At reviewed PR source `66f36c749ea693516defa7d2be672eff128936e5` (tree `84ae2f7c34837e84f9b0509fe018fb8b25ff4832`), `site/src/content/docs/index.mdx` opens with “Put your whole mail history to work,” labels mxr “A local mail runtime for you and your agents,” then spends a long sentence explaining Gmail connector, local sync, actors, operations, and ongoing provider sync. The adjacent synthetic subscriptions proof carries “Synthetic illustration,” limited-candidate and READ-state caveats. Its first screen then introduces additional account/store/client/network mechanics. This is a genuine category correction; it is also a high reading load before a visitor has seen a crisp result. [Reviewed PR source](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/site/src/content/docs/index.mdx)
- The old screenshot supplied by BK shows “Talk to your whole inbox” in large type, a compact benefit paragraph and a large TUI rules-view recording, followed by install and provider rows. That hierarchy gives the product more immediate visual presence. The paragraph claims searching all accounts, full-history analytics and voice-matched agent drafting, but the image itself shows rules, not those outcomes. So the old screen wins on visual concreteness, not on proving its headline's runtime benefits. [BK-supplied screenshot](/Users/bhekanik/.t3/userdata/attachments/42b4371f-ed49-4fba-a98c-1f81e4a96dd9-23dc0528-afa4-4fdc-9dc1-7a872512335d.png)
- The new synthetic subscription ranking is honest about illustration status, but it is a poor hero proof: S2 against published `mxr 0.6.156` on baseline `origin/main` `f39ae66578e3099e464967ab2f042e24b1ad8ec4` synced 55 fake messages/account and returned four subscription buckets/account, each with `message_count=1` and `opened_count=0`; thus there was no meaningful ranking variation. `opened_count` is current READ flags, and ranking sorts a limited returned set. [S2 proof](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/evidence/s2-runtime-proof.md)
- The canonical approved direction explicitly pairs “Put your whole mail history to work” with “A local mail runtime for you and your agents,” and BK's D-WEB-027 requires mailbox ownership on the user's computer plus continued provider sync and reusable daemon mail operations. [Positioning](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/04-positioning.md); [decision log](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/docs/website-refresh/blueprint/12-decisions.md)
- Official-page snapshot accessed 2026-10-23; audience, value, pricing, CTA and proof/hierarchy:

| Product | Position and offer | CTA / proof / visible hierarchy |
|---|---|---|
| Superhuman | “The most productive inbox ever made”; productivity teams; AI, collaboration, calendar. $25 Starter or $33 Business/member/month billed annually; custom Enterprise. | Get Started / sales; company logos and attributed testimonials; plans then feature matrix. [Plans](https://superhuman.com/plans/mail) |
| Shortwave | “Supercharge your productivity…”; AI inbox users; AI search, filters, summaries, attachment analysis. $30/$45/$120 per seat/month (Business/Premier/Max). | 14-day free trial; pricing page leads with trial and tier matrix; no testimonial proof captured. [Pricing](https://www.shortwave.com/pricing/) |
| Thunderbird | Free, open-source, cross-platform local client; broad email users, extensions and ownership/control. Free; community-funded. | Download; FAQ is feature/value prose, no plan or testimonials shown. [FAQ](https://support.mozilla.org/en-US/kb/thunderbird-faq) |
| Mimestream | “Made for Mac. Optimized for Gmail”; Mac Gmail users; Gmail-specific features and device-local storage. 14-day no-card trial, then $49.99/year or $4.99/month individual. | Download Free Trial / Watch Video; headline, product visuals and feature sections. [Home](https://mimestream.com/); [pricing](https://mimestream.com/pricing) |
| Resend (aspirational developer-tool reference) | “Email for developers”; transactional/marketing email developers; SDKs, test mode, delivery tooling. Price not checked. | Get started / Documentation; code and supported tools near top, followed by test-mode demonstration and customer logos. [Home](https://resend.com/) |
- Anecdotal sample only: an Oct. 2026 thread distinguishes client from provider and asks about local/cloud storage; a March 2026 thread wants searchable history across accounts/devices; a Feb. 2026 thread asks about caching/proxying. These are questions to answer, not prevalence or proof mxr is more private. [Oct.](https://www.reddit.com/r/emailprivacy/comments/1xbgnie/thunderbird_as_email_for_privacy_security/); [March](https://www.reddit.com/r/Thunderbird/comments/1s5bdzd/thunderbird_as_a_universal_email_archive_and/); [Feb.](https://www.reddit.com/r/emailprivacy/comments/1r9us37/is_the_email_client_important_in_terms_of_privacy/)

### Inferences
- Inference: local storage alone is not distinctive (Thunderbird/Mimestream), and AI productivity is crowded (Shortwave/Superhuman). mxr's operating model is synced history, reusable daemon operations, and human/agent clients sharing one mailbox. No universal first/only claim is supported.
- Sequence: approved headline; category; one short sentence that synced mail lives on this computer while the provider keeps syncing; one visible task/result; try synthetic mail as primary CTA, with existing-agent and human routes legible. Explain daemon reuse after the benefit. Keep install available, not required to understand the proposition.
- Proof needs provenance and useful variation: either captured synthetic output or a clearly labeled illustration. A dated local search followed by human review, or scoped multi-account search with coverage, is stronger than the current S2 subscription ranking. Claim draft handoff only after observing the same saved draft in web/TUI.
- Keep first-screen limits to “synced mail” and ongoing provider sync; place incomplete coverage, attachment cache, offline details, profile authority, optional activity, operation-specific undo and model/endpoint boundaries in relevant guides. Do not imply universal connector limits, zero cloud transmission, or actual opens from READ flags.
- Conversion: explain category → reproduce one synthetic result → choose agent or human path. Demo replies are canned. State that mailbox access isn't needed for synthetic mail and remote hosted agents don't automatically reach a local daemon.

### Gaps
- I did not obtain a rendered screenshot of the PR page. The local source and its existing design comp establish authored hierarchy; the native T3 browser opened Superhuman, but screenshot snapshot timed out at 15 seconds. Competitor visual observations therefore describe text/hierarchy from official pages, not independently measured pixel layout. The previously supplied Vercel preview was auth-gated in earlier audit.
- No conversion analytics, comprehension study, testimonials, authorized logos for mxr, or benchmark exists in this evidence. Do not claim the proposal converts better; “better” here means more accurate positioning, while conversion remains untested.
- Pricing may change; all figures are snapshots accessed 2026-10-23. Thunderbird had no paid app tier in the official FAQ; optional services may have separate terms. Resend pricing and detailed billing were not captured.

## How should positioning, copy, and proof sell today's local mail runtime?

### Takeaway
Keep the approved positioning. Make local mailbox ownership and continued sync the plain-language mechanism; show that an agent/script/client reuses mxr's mail operations and the same daemon-backed state. Sell a practical new capability over the visitor's existing connector, not an AI email app or client replacement.

### Cited Findings
- Candidate source `66f36c7…` and baseline `origin/main` `f39ae66…` show the shift from email-client/AI benefit claims to local mailbox, sync, common operations and synthetic trial. [Candidate](https://github.com/planetaryescape/mxr/blob/66f36c749ea693516defa7d2be672eff128936e5/site/src/content/docs/index.mdx); [baseline](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/src/content/docs/index.mdx)
- Review material at baseline says connector claims must be bounded: official ChatGPT and Anthropic help pages describe varying account support and Gmail operations including search/read/draft/send or controls, so mxr should explain its own local runtime instead of claiming connectors are uniformly single-account, uncached, read-only, or slower. [ChatGPT app account guide](https://help.openai.com/en/articles/20001494-connecting-and-managing-app-accounts-in-chatgpt); [Anthropic Workspace connector](https://support.claude.com/en/articles/10166901-use-google-workspace-connectors)

### Inferences
- Keep headline/category; reduce the supporting copy to the owned local copy, ongoing provider sync, common operations and same mailbox for human review. A visual flow can show provider → synced local mailbox/index → agent/script/human using it. State that permissions and interfaces differ in the trust guide; “one mailbox” means one person's daemon-backed state, not a team inbox.
- Must appear: what mxr is; what lives locally; sync continues; the benefit over a direct connector; a task/result; synthetic-mail CTA; agent and human routes. Put offline/attachment detail, profiles/scopes, activity, preview/undo, send gates and adapter inventory in task guides.
- Say “synced history,” not an immediately complete remote mailbox. If using subscription ranking, call it current READ flags, limited to eligible returned buckets—not actual opens or all newsletters. Distinguish external agents, built-in assistant and optional native model features.
- Proof chain: dataset/scope → exact query/action → dated captured result → next action. Label illustrations before the result. Choose a legible task with actual variation.

### Gaps
- Community evidence is a three-thread purposive sample from Reddit, not a representative study. It surfaced distinctions/questions around local copy, provider-hosted mail, multiple account history and privacy boundaries, but not reliable demand sizing or agent-specific unmet needs.
- No measured setup completion, time saved, latency, token reduction, feature adoption or conversion is available. Results remain a positioning hypothesis until a reader can explain the connector/runtime difference and complete the synthetic task.

## Research record and closeout

### Takeaway
This is a bounded review of positioning and homepage execution only. No implementation or external state change was performed.

### Cited Findings
- Worktree path `/Users/bhekanik/.t3/worktrees/mxr/t3-584b0357`; branch `t3/mailbox-positioning-review`; HEAD `66f36c749ea693516defa7d2be672eff128936e5`; tree `84ae2f7c34837e84f9b0509fe018fb8b25ff4832`. Fetched `main` over HTTPS after SSH public-key denial; `origin/main` was `f39ae66578e3099e464967ab2f042e24b1ad8ec4`, tree `9ecf26304579bd14e0fc5e8cc63db8435a1f99df`. Branch worktree was clean before note creation.
- Checked primary pages on 2026-10-23: Superhuman, Shortwave, Thunderbird, Mimestream, Resend; three selected Reddit discussions linked inline. T3 preview opened Superhuman but snapshot timed out. About 18 minutes and 11 focused web calls, within budget. No code build/test; code-simplifier and TS review N/A. No mail, implementation, commits, pushes, PR, deploy, or deletion. Owns only this note.

### Inferences
- Main user decision: keep the new positioning and simplify its execution; don't call the candidate conversion-better without user evidence.

### Gaps
- File size at closeout: 12,716 bytes (1,526 words). No research/build process was started in background; unrelated long-running desktop/agent processes were visible on the shared host and left untouched.
