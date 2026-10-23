# The site has runtime substance but inconsistent entry points

Audited on 2026-10-23. Main baseline: `origin/main` `f39ae66578e3099e464967ab2f042e24b1ad8ec4`. Feature reference: `origin/feat/classifier-assessment-r2` `2053f18ed406b981ad56639b64a29703b4fd9a7a`. These refs were freshly fetched. Their merge base was the main baseline. Main’s release manifest says `0.6.156`; the installed Homebrew binary reported `mxr 0.6.155`. The audit did not establish published artifact availability.

The root checkout was clean but behind main. Planning files live in a fresh worktree at `/tmp/mxr-docs-positioning-clones/mxr`, branch `docs/runtime-website-blueprint`. No mail account, real message, credential or live mailbox state was accessed. No demo profile was started.

## The earlier discussion already identified the missing abstraction

- [Mxr capabilities clarification](https://chatgpt.com/c/6a6cd1e9-4094-83eb-a112-4865c91c4cc3), 2026-07-31: BK distinguishes a runtime exposing composable capabilities from selected built-in AI features. His words: “Mxr handles the archaic stuff behind the scenes and exposes a modern api.” The conversation later corrects speculative claims about another product’s API/account support; those claims are rejected here.
- [App Landing Page Feedback](https://chatgpt.com/c/69c9346d-b2d8-8385-b8b1-3effa6e8c753): BK requires comprehensive docs/code retrieval and separates reusable daemon truth from client rendering. Historical implementation suggestions are not proof of current behaviour.
- [C++ philosophy vs C](https://chatgpt.com/c/6389466c-08a9-4236-a479-2c73262f6f3d): the relevant section concerns reusable parts and a daemon serving multiple surfaces. Unrelated speech-recognition and language-history discussion does not enter the blueprint.
- [Compare Rex and MXR](https://chatgpt.com/c/6ad71a53-4330-83eb-bd34-ee1c9b7e9845): corroborates the runtime positioning question. Assistant competitor descriptions are not treated as independent evidence.

The complete current request and explicit blueprint invocation supply the current product direction. Earlier conversations provide context, not authorisation to send messages, change another product or implement old suggestions. Private transcript exports remain in `/tmp/mxr-website-research/`; only task-relevant product conclusions are retained here.

## The public page and preview are different evidence surfaces

The public [homepage](https://mxr.sh/) returned the same “Talk to your whole inbox” opening as the pinned source. The [supplied preview](https://mxr-mail-git-feat-classifier-assessment-r2-planetaryescape.vercel.app/guides/email-modes/) redirected to Vercel login. Its deployed commit could not be identified. The fetched branch source supplies a useful code/content comparison, not a verified screenshot of that deployment.

T3 preview navigation succeeded, but snapshot inspection twice returned `Preview automation snapshot timed out after 15000ms.` Public text was read through web retrieval; no rendered visual audit is claimed.

## Claims have specific evidence and specific limits

| ID | Verified observation | Primary source | Status and consequence |
|---|---|---|---|
| F01 | The mailbox is persisted locally in SQLite, with reader/writer pools. | [crates/store/src/pool.rs#L45–L106](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/store/src/pool.rs#L45-L106) | Code-backed. This establishes storage, not completion of every account backfill. |
| F02 | Search parses and executes against local search state; lexical fallback is explicit. | [crates/daemon/src/handler/diagnostics/search_execute.rs#L29–L96](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/diagnostics/search_execute.rs#L29-L96) | Code-backed. Do not imply every search is semantic or every attachment is indexed. |
| F03 | Provider traits provide sync/send operations; configuration distinguishes Gmail, IMAP, Outlook and SMTP. | [crates/core/src/provider.rs#L9–L59](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/core/src/provider.rs#L9-L59) | Code-backed. Also see config types 724–789 and send trait 142–221; capability differences remain. |
| F04 | Subscriptions aggregate eligible messages by account and lowercase sender. opened_count counts READ flags; spam/trash are excluded. | [crates/store/src/message.rs#L1079–L1203](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/store/src/message.rs#L1079-L1203) | Code-backed. It is a read-state proxy, not a count of distinct opens. Buckets remain per-account. |
| F05 | Subscriptions rank is applied after ListSubscriptions returns a limited set. | [crates/daemon/src/commands/subscriptions.rs#L87–L130](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/commands/subscriptions.rs#L87-L130) | Code-backed. A default limited result is not a global ranking of every sender. |
| F06 | Global safety policy and identified agent/MCP profiles are checked before dispatch; profile send/destructive gates are separate. | [crates/daemon/src/handler/mod.rs#L574–L606](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mod.rs#L574-L606) | Code-backed. Detailed enforcement is at 2264–2345; non-agent/MCP client kinds return before profile checks. |
| F07 | The normal CLI uses Cli origin; MCP uses Mcp and calls daemon requests. | [crates/daemon/src/ipc_client.rs#L76–L96](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/ipc_client.rs#L76-L96) | Code-backed. MCP lib.rs 91–116. A shell-capable agent invoking ordinary CLI is not thereby an agent-profile request. |
| F08 | Activity is local recorder output with a client source, optional enablement, a kill switch and a bounded queue. | [crates/daemon/src/activity/mod.rs#L1–L132](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/activity/mod.rs#L1-L132) | Code-backed. Entries can be dropped under backpressure; some actions are not mapped. Not a complete immutable audit log. |
| F09 | Supported mutations write undo entries with expiry and can report undo_unavailable. | [crates/daemon/src/handler/mutations.rs#L633–L726](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mutations.rs#L633-L726) | Code-backed. No universal reversal or guarantee that a successful mutation has an undo record. |
| F10 | Send validates the stored draft and enforces draft safety at submission; reviewed identity/revision is part of the contract. | [crates/daemon/src/handler/mutations.rs#L3710–L3786](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mutations.rs#L3710-L3786) | Code-backed. Read together with 3496–3575. Does not promise perfect safety detection or recall after delivery. |
| F11 | Native model features default off; the default endpoint is local and cloud relationship-data permission defaults off. | [crates/config/src/types.rs#L350–L402](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/config/src/types.rs#L350-L402) | Code-backed. External agent output handling and per-feature/cloud fallback routing require separate disclosure. |
| F12 | Draft assistance gathers recipient-specific voice material from prior sent mail and conversation context. | [crates/daemon/src/handler/draft_voice.rs#L191–L265](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/draft_voice.rs#L191-L265) | Code-backed. Context/quality depends on available mail; voice_match.rs 24–67 reports low confidence with fewer than three samples. |
| F13 | Mode placement and mode completion use reusable rules and marks. | [crates/daemon/src/handler/mode_rules.rs#L1–L42](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mode_rules.rs#L1-L42) | Code-backed. Existing thread/mode membership is not the R2 stored-email-home implementation. |
| F14 | Reading has extracted items, edition state, later state and highlights over the mailbox. | [crates/daemon/src/handler/reading/mod.rs#L1–L50](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/reading/mod.rs#L1-L50) | Code-backed. A real mode-specific workflow, not just a provider label. |
| F15 | Archive scans local mail into record data; its handler serves records, sourced fields and documents. | [crates/daemon/src/handler/records.rs#L1–L85](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/records.rs#L1-L85) | Code-backed. Archive records and provider archive mutations are different subjects. |
| F16 | To do renders bands, state, action and field provenance for clients. | [crates/daemon/src/handler/todo_view.rs#L1–L85](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/todo_view.rs#L1-L85) | Code-backed. Do not promise every email task is detected or automatically resolved. |
| F17 | Updates composes a cut-based briefing from source histories and selected items. | [crates/daemon/src/handler/updates_digest.rs#L1015–L1063](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/updates_digest.rs#L1015-L1063) | Code-backed. Keep current deterministic-fact and source-evidence explanation; no need to call every briefing AI-generated. |
| F18 | The remote guide describes reading/replying from another computer against a selected host. | [site/src/content/docs/guides/remote-mailbox.md#L1–L75](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/src/content/docs/guides/remote-mailbox.md#L1-L75) | Docs-observed. Preserve as an advanced path; re-verify current commands before promoting remote support details. |
| F19 | The homepage opens “Talk to your whole inbox” and explains local history, with later agent/mode/surface sections. | [site/src/content/docs/index.mdx#L1–L24](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/src/content/docs/index.mdx#L1-L24) | Source and public text observed. The substance is partly present; the issue is first-screen category and coherent hierarchy. |
| F20 | The quick start says the TUI is what users will use most; curated llms.txt still leads with email client. | [site/src/content/docs/getting-started/quick-start.md#L1–L14](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/src/content/docs/getting-started/quick-start.md#L1-L14) | Source-observed. Also site/public/llms.txt 1–9. These conflict with equal surface legitimacy/runtime positioning. |
| F21 | Astro config owns description, social tags, redirects and a long manually grouped sidebar. | [site/astro.config.mjs#L1–L74](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/astro.config.mjs#L1-L74) | Source-observed. Keep current framework and routes; update grouped entry points together. |
| F22 | The social image source says every attachment stays local and includes an illustrative 0.04s result. | [site/scripts/generate-social-images.mjs#L1–L47](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/scripts/generate-social-images.mjs#L1-L47) | Source-observed. Correct the source; no benchmark was performed and attachments are not all pre-cached. |
| F23 | The machine-doc generator creates Markdown siblings/full corpus; curated llms.txt remains authored. | [site/scripts/generate-llms-txt.mjs#L1–L112](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/scripts/generate-llms-txt.mjs#L1-L112) | Source-observed. Check generated content rather than assuming raw MDX converts to clean prose. |
| F24 | The supplied feature branch adds a stored home/reason explanation to email modes. | [site/src/content/docs/guides/email-modes.md#L60–L82](https://github.com/planetaryescape/mxr/blob/2053f18ed406b981ad56639b64a29703b4fd9a7a/site/src/content/docs/guides/email-modes.md#L60-L82) | Branch-only at the feature SHA. No proof that this exact deployed preview or published binary contains it. |
| F25 | Docs checks run generation, script tests, selected validation and Astro build; they do not establish reader comprehension. | [site/package.json#L1–L21](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/site/package.json#L1-L21) | Source-observed. CI docs job is .github/workflows/ci.yml 586–604. validate-docs.mjs checks selected patterns, not all links or live examples. |

Supporting code for F03: [provider configuration](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/config/src/types.rs#L724-L789). F06: [profile enforcement](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mod.rs#L2264-L2345). F07: [MCP connection](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/mcp/src/lib.rs#L91-L116). F08: [activity submission after dispatch](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mod.rs#L512-L536).

## The connector distinction must survive current connector capabilities

Official sources checked on 2026-10-23:

- [OpenAI: connecting and managing app accounts](https://help.openai.com/en/articles/20001494-connecting-and-managing-app-accounts-in-chatgpt) says multiple connected accounts are available for some apps and availability varies. It does not establish that every Gmail connector supports multiple accounts, or that none does.
- [Anthropic: Google Workspace connectors](https://support.claude.com/en/articles/10166901-use-google-workspace-connectors) describes Gmail search/read, drafts, sending and organisation, plus approval controls and provider limits. A claim that connectors are inherently read-only or cannot prepare mail is unsupportable.

These sources do not establish connector cache implementation, one network request per question, full-corpus completeness, relative latency, or token cost for the exact Codex/Claude setup a visitor uses. No such benchmark was run. Keep the public comparison to mxr’s verified operating model. A connector may be the simpler fit when the user does not want local software and sync. No current Missive or Rex capability comparison is needed for the implementation.

## The request’s proposed claims are adopted, narrowed or held

| Proposed idea | Disposition | Evidence-led wording |
|---|---|---|
| Local mail runtime | Adopt | Put the category in the opening and prove a useful task. |
| One account and API round trip per connector question | Reject as a universal factual claim | State that mxr queries synced local state; describe named connector details only with current evidence. |
| Every account and whole history at once | Narrow | Connected, permitted, synced history; show coverage during first sync and in limited aggregate results. |
| Rank by how often you actually open newsletters | Narrow | READ-state rate among eligible messages in returned sender/account buckets; not distinct-open telemetry. |
| Shared mailbox | Adopt with definition | One user’s daemon-backed state across human and machine clients, not a team collaboration inbox. |
| Agent drafts appear for review | Adopt as code-backed; observe in implementation | Demonstrate the same saved draft ID/account/revision in the human client. |
| Every action is logged with client | Narrow | Supported actions have client origin when logging is enabled; recorder is bounded and origin is not human identity. |
| Daemon decides agent permissions | Adopt with boundary | Identified agent/MCP clients are gated; unrestricted same-user CLI/process access is outside that profile guarantee. |
| Dry runs and undo | Adopt with boundary | Operation-specific preview/review and supported undo with expiry; no universal recall. |
| Pre-send check | Adopt | Current draft is checked on submission; do not promise perfect detection. |
| Mail content is data, never instructions | Adopt | Keep the explicit agent rule, backed by daemon authority limits rather than a claim of complete injection prevention. |
| Stored home/reason from supplied preview | Hold | Branch evidence exists; release/deployment status remains an implementation gate. |

## Reuse stays inside mxr

No other BK repository contributes code. The existing `site/` framework, CSS, copy controls, media, generators and reference contracts are the reuse map. [Site architecture](../website-refresh/blueprint/01-site-architecture.md) assigns ownership. Historical examples from the blueprint skill are format references only; their code was not copied.

Existing reuse points, all at the pinned main SHA:

| Exact source path | Lines at baseline | Reuse intent |
|---|---:|---|
| `site/src/content/docs/index.mdx` | 454 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/astro.config.mjs` | 199 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/src/styles/custom.css` | 2101 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/public/install-copy.js` | 54 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/public/hero-cast.js` | 168 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/src/components/VideoGallery.astro` | 117 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/scripts/generate-cli-reference.mjs` | 866 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/scripts/dump-openapi-spec.sh` | 55 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/scripts/generate-llms-txt.mjs` | 112 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |
| `site/scripts/generate-social-images.mjs` | 52 | Retain the existing responsibility; adapt only the website surface named in the blueprint. |

## The next verification is a user journey

Before public implementation, re-fetch source and establish release status, replay a scoped synthetic local query, create a bounded-profile draft visible in a human client, then test whether a newcomer can explain the connector distinction. Full details and owners are in [open questions](../website-refresh/blueprint/13-open-questions.md).
