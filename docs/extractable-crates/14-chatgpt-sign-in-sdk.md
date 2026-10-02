---
candidate: chatgpt-sign-in-sdk
status: preflight
decision: no-go
mxr_source: none (no seed code; mxr has no Sign in with ChatGPT integration at origin/main 3da0c119)
last_reviewed: 2026-10-02
revisit_when: |
  All three must hold: (1) OpenAI publishes data terms for ChatGPT plan
  usage requests that let mxr send mail content, so mxr can ship an
  in-tree provider; (2) that provider has run in a release for at least
  one cycle and become the seed; (3) a re-run of the ecosystem search
  still finds no maintained Rust crate, including in rig-core and
  async-openai. Also revisit if OpenAI drops the "preview" label or
  publishes a versioning policy for the flow.
---

> **Status: no-go (preflight only).** Rust has a gap today: no crate on
> crates.io implements Sign in with ChatGPT's plan-usage flow. The work
> clears the non-trivial bar, narrowly. The candidate fails the third bar
> test outright: mxr has no seed code, mxr can't consume the crate while
> the data terms are unpublished, and none of BK's other Rust projects
> calls an LLM. The API it would wrap is three days old and labelled
> "preview". Lesson 11 calls this shape a new project, not an extraction.
> Research inputs: [`docs/research/chatgpt-sign-in.md`](../research/chatgpt-sign-in.md).

# Sign in with ChatGPT SDK (working name, see naming)

> A Rust client for OpenAI's Sign in with ChatGPT (SIWC) with the
> "ChatGPT plan usage" permission. It covers loopback PKCE sign-in with
> dynamic client registration, ID-token verification, rotating refresh
> with serialized refreshes, pluggable credential storage, and the
> plan-usage error taxonomy. Launched at DevDay on 2026-09-29.

## Decision: no-go as an mxr extracted crate

| Bar test (lesson 10) | Verdict | One-line reason |
|---|---|---|
| 1. Real ecosystem gap | **Pass, fragile** | Zero SIWC crates on crates.io; npm already has two published implementations, and the big Rust LLM frameworks are the natural home for it |
| 2. Non-trivial work | **Pass, narrowly** | Happy path is an afternoon; a credible contract (multi-account registration, identity-checked reauth, cross-process refresh, error taxonomy) is ~2,000-2,800 source lines plus tests |
| 3. Production-credible seed | **Fail** | No seed in mxr, no consumer in mxr or BK's other Rust projects, and a "preview" API that could reshape v0.1 |

Two non-bar risks also apply: the spec is unstable (three days old,
"preview limitations" page, rollouts still pending), and the clean-room
and naming rules add process to any build.

## Rust has no SIWC crate today, but OpenAI's own Rust login code is not reusable and npm is already covered

Searched 2026-10-02. WebSearch, Exa and Brave were unavailable, so the
evidence comes from the crates.io API, crate tarballs on
static.crates.io, the GitHub API, the npm registry, PyPI and HN Algolia.

### crates.io: 364 recently updated related crates, none implementing SIWC

- Keyword queries returned ChatGPT-adjacent crates for "chatgpt oauth",
  "sign in with chatgpt", "codex auth", "openai pkce", "chatgpt plan",
  "chatgpt login", "chatgpt auth", "codex login", "openai oauth" and
  "chatgpt subscription". "siwc" returned nothing.
- To check for SIWC specifically, I downloaded every crate matching
  `chatgpt`, `codex`, `openai`, `oauth` or `responses` that was updated on
  or after launch day (2026-09-29): 364 crates. I grepped each for the
  protocol markers `dynamic_agent_client` and
  `chatgpt.tokens.use.direct`. **Zero hits.**
- The auth crates that do exist all implement the older, unsanctioned
  path: they reuse Codex CLI's first-party client ID
  (`app_EMoamEEZ73f0CkXaXp7hrann`) and often call
  `chatgpt.com/backend-api/codex`. The SIWC docs say
  not to: "do not point it at ChatGPT's `backend-api` endpoints". Examples
  with that client ID in the published source: `openai-auth` 1.0.0
  (16.7k downloads, MIT), `motosan-ai-oauth` 0.2.1 (42.8k), `codex-oauth`
  0.1.1, `vtcode-auth` 0.171.4, `rs_ai_oauth` 0.2.36, `llmshim` 0.23.0,
  `nanocodex-oai-api` 0.6.6 and `rig-core` 0.43.0.
- `rig-core` (3.19M downloads) ships a `providers::chatgpt` module on
  the backend-api path and has an open issue for app-provided OAuth
  token storage (0xPlaygrounds/rig#2481, 2026-09-10). It is the likeliest
  Rust home for SIWC support. `async-openai` (8.8M downloads, last release
  2026-09-28) already streams the Responses API. Per the SIWC docs, any
  OpenAI client works for inference once it sends the access token as
  the bearer key.

### GitHub: 22 Rust repos already hand-roll SIWC, none as a published library

GitHub code search for `dynamic_agent_client` restricted to Rust found
22 repos inside three days of launch. Examples: `gethamster/horde`
(Apache-2.0), `everruns/yolop` (MIT), `leynier/alera` (MIT),
`ldclabs/anda-bot` (Apache-2.0), `xuzhougeng/wisp-science` (AGPL-3.0),
`solpbc/solstone-journal` (AGPL-3.0) and `ntindle/don-a-token`
(Apache-2.0, created 2026-10-02). Two of them already have crate-shaped
code: `solstone-core-chatgpt-auth` and `don-a-token-core`. Neither is
on crates.io (both return 404).

This cuts both ways. Everyone re-implements the flow, which is the
classic sign of a library gap. A two-person project could extract
and publish its crate any day, though, and the gap would close before
ours shipped.

### openai/codex: `codex-login` exists, is Apache-2.0, and is not reusable

Checked at openai/codex `ca466061d64f` (main, 2026-10-02):

- `codex-rs/login` is the `codex-login` crate. It is Apache-2.0 through
  the workspace, `version = "0.0.0"`, and **not on crates.io** (404). No
  `codex-*` crate is published by OpenAI. `codex-app-server-protocol` on
  crates.io is a third-party republish from `namastexlabs/codex`.
- It depends on 13 internal workspace crates (`codex-config`,
  `codex-keyring-store`, `codex-protocol`, `codex-otel`,
  `codex-agent-identity` and others), so it can't be taken as a dependency.
- It implements Codex's own first-party login: a fixed `CLIENT_ID`,
  `auth.json`, device-code login and gateway auth. GitHub code search finds no
  `dynamic_agent_client` or `chatgpt.tokens.use.direct` anywhere in
  openai/codex. It does not implement SIWC.
- Apache-2.0 would allow borrowing generic pieces (PKCE, loopback server)
  with attribution. The SIWC-specific parts would still have to be
  written.

### Other languages: npm is covered, Python is served by the official SDK, OpenAI's DevKit is Node-only and unpublished

- **OpenAI DevKit.** `openai/sign-in-with-chatgpt-devkit` ships
  `@siwc/local` and `@siwc/react` as local workspaces only. The README says
  "Both packages are local workspaces in this repository", and
  `registry.npmjs.org/@siwc/local` has no published versions. It is Node
  and React only, under a noncommercial licence (see below). OpenAI
  publishes no Rust, Go, Swift or Python DevKit.
- **npm, published.** `@earendil-works/pi-ai` 1.0.0 (MIT) contains
  `dist/auth/oauth/openai-chatgpt.js` with the SIWC markers.
  `@tanstack/ai-openai` 0.26.0 (MIT, published 2026-10-02) contains
  `dist/esm/siwc.js`. `@reforma/siwc-bridge` 0.0.1 covers remote callback
  bridging.
- **Python.** The SIWC docs show the official `openai` SDK with the
  access token passed as `api_key`. No SIWC auth package is on PyPI under
  `siwc`, `sign-in-with-chatgpt` or `chatgpt-plan` (all 404).
  `ftnext/sign-in-with-chatgpt-py` (Apache-2.0, 2026-10-01) is an
  unpublished repo.
- **Ruby.** `ajaynomics/omniauth-openai` (2026-10-02) is an unofficial
  OmniAuth strategy.
- **Demand.** HN Algolia since launch shows launch stories and
  discussion of the feature, but no requests for a Rust SDK.

## The contract is mostly auth policy, about 2,000-2,800 source lines

### Inference doesn't need a new client

The docs' Python example is the official SDK with
`api_key=ACCESS_TOKEN`. Inference is standard
`POST /v1/responses` with `store: false, stream: true`, plus
`GET /v1/models`. A Rust SDK should not ship its own Responses client.
It should provide:

- a request-policy check that rejects the 15 unsupported body fields,
  `role: "system"` items, `previous_response_id` over HTTP, and the
  unsupported tools;
- a stream-outcome classifier for `response.failed` and `response.incomplete`;
- a thin adapter for `async-openai` or `reqwest-eventsource`.

That keeps the crate out of the crowded `async-openai`, `genai` and `rig`
space, which is why [`wont-do/12-llm.md`](./wont-do/12-llm.md) failed.

### Component estimate

| Component | Spec anchor | Est. source lines |
|---|---|---|
| Authorize URL builder: state, nonce, PKCE S256, `resource`, `agent_name_hint` on first registration only, `id_token_hint`/`login_hint` on reauth | SIWC sign-in step 2; RFC 7636; RFC 8707 | 150 |
| Loopback callback server: `127.0.0.1` only, fixed `/auth/callback` path, port fallback, timeout and cancel, `error=access_denied` | SIWC step 2-3; RFC 8252 section 7.3 | 300 |
| Registration model: issued `oaiapp_...` client ID from the callback, reject a mismatched client ID on reauth, never save `dynamic_agent_client` | SIWC step 3; Accounts and sessions | 200 |
| Code exchange, refresh, revoke via OIDC discovery | SIWC steps 3, token reference; RFC 6749; RFC 7009 | 300 |
| ID-token verification: JWKS fetch and cache with key rotation, iss, aud = issued client ID, exp, nonce, `sub` match on reauth; scope check for `chatgpt.tokens.use.direct` | SIWC step 4; OIDC Core 3.1.3.7 | 250 |
| Host ID: persisted `urn:uuid:` v4 or `urn:ietf:params:oauth:jwk-thumbprint:` | SIWC overview; RFC 9562; RFC 7638; RFC 9278 | 100 |
| `CredentialStore` trait, atomic 0600 file backend, optional `keyring` backend, multi-account records keyed by issued client ID | SIWC step 5; Accounts and sessions | 400 |
| Refresh coordinator: in-process mutex plus cross-process file lock, honour `earliest_refresh_at`, write the rotated refresh token before use, terminal vs transient errors | Accounts and sessions ("serialize refreshes"); RFC 9700 section 4.14 | 300 |
| Error taxonomy: `{"detail":...}` pre-stream bodies (401/403/503), nine structured `subscription_sharing_*` and `chatpass_v2_*` codes, the 429 that can arrive mid-stream, refresh error codes | Errors and recovery | 300 |
| Request-policy check and stream-outcome classifier (above) | Preview limitations; Models and inference | 250 |
| Headless: credential export/import for self-hosted VMs | Self-hosted VMs | 100 |
| **Total source** | | **~2,650** |
| Tests: mock authorization server (wiremock), signed-JWT fixtures, cross-process refresh race, mid-stream 429 | | ~1,500 |

As a cross-check, existing in-app Rust implementations run 49 KB
(`everruns/yolop`, one file) to 202 KB (`solstone-core-chatgpt-auth`,
16 files with tests), or about 1,400 to 5,800 lines. I counted file
sizes only and read none of their code; see the clean-room section.

### Lesson 10's bar: passes, but on policy rather than algorithm

"Implement in an afternoon from the spec" is true for the happy path:
one account, one process, ignore reauth. Dozens of projects did that in
three days. It is false for the credible contract. The traps are
policy traps, the same kind that made `mail-query` non-trivial:

- registration is distinct from reauthorization;
- the issued client ID must be bound to the verified `sub`;
- a mismatched callback client ID must be rejected;
- refresh must be serialized across processes, because a reused refresh
  token is terminal (`refresh_token_reused`);
- a 429 must never be turned into a reset time;
- a disconnect is never signalled, so revocation shows up only as a
  terminal refresh error.

Each component anchors to a written spec, either the SIWC docs or an
OAuth RFC. That satisfies lesson 11 rule 1.

The headline requirements in the brief all hold. Self-registration is
`dynamic_agent_client`, run once per account per host. Headless
handling is documented as "complete OAuth locally, copy the credential"
because no device-code flow exists for SIWC.

## The spec is three days old and labelled preview, so a v0.1 could be reshaped within weeks

- The inference constraints live on a page titled
  **"Preview limitations"**, with the heading "Preview behavior and
  limitations".
- Availability language is temporal: "**At launch**, ChatGPT plan usage
  is available to open-source projects..." (cookbook) and "Sign in with
  ChatGPT is **currently** available to selected commercial partners
  through a limited trial" (quickstart).
- One rollout is still pending in the docs themselves:
  "Request consent with `force_reconsent=true` only after OpenAI confirms
  deployment for your integration; the existing OAuth `prompt=consent`
  mechanism remains supported before that rollout."
- The host-ID design signals a likely change: "A public-key-derived host
  ID is currently an identifier only. OpenAI does not verify possession
  of the private key as part of this flow." If OpenAI starts verifying
  possession, the host-ID API becomes a key-management API.
- I found no versioning, changelog or deprecation statement in
  `developers.openai.com/siwc/llms-full.txt`. That is a snapshot of
  2026-10-02, 1,386 lines, sha256 prefix `14884eaed93be740`.
- Precedent: OpenAI has just moved builders off one auth path. The Codex
  app-server docs now say "we recommend migrating to Sign in with
  ChatGPT", and that "App-server authentication has never been
  permitted for commercial or hosted services".
- The DevKit's eight commits all landed between 2026-09-29 01:42Z and
  16:08Z. There have been none since, so it gives no signal yet on how
  often the protocol changes.

A published crate would have to say "tracks a preview API, expect
breaking 0.x releases". That is honest, but it is the "v0.1.0 users
outgrow in their first month" outcome that lesson 10 warns against.

## Legal and naming: avoid OpenAI marks in the crate name and build clean-room from the public docs

### Brand rules forbid "GPT" in product and developer names, and "ChatGPT" contains it

From https://openai.com/brand/ (read through `r.jina.ai` on 2026-10-02,
because openai.com returns 403 to curl; check against the live page
before relying on it):

- "We do not permit model names in app titles because there is concern
  that it confuses end users. It also triggers our enforcement
  mechanisms."
- "we do not permit our GPT brand to be used in app, product, developer
  or company names".
- "Do not feature our Marks more prominently than your own company's name
  or marks." "We may terminate permission to use our Marks at any time".
- The page's "API Developers" and "Non-partnerships" language sections
  are collapsed and did not render through the proxy. I could not read
  them.

The Terms of Use say: "You may only use our name and logo in accordance
with our Brand Guidelines." DevKit licence section 5: "This License grants
no rights to OpenAI names, trademarks, logos, or branded visual assets".

A crate name containing "ChatGPT" or "OpenAI" is the product name in
crates.io terms. Crates like `async-openai`, `openai-api-rs`,
`chatgpt_rs` and `chatgpt` exist, which shows tolerance, not permission.
The safe pattern is a mark-free crate name, with "Sign in with ChatGPT"
used only descriptively in the description and README, plus an
"unofficial, not affiliated with OpenAI" line. That matches how BK's
`chatgpt-cli` README already disclaims.

### No approval is documented for third-party SDKs, but the app registration must carry the consuming app's name

- The docs document no review step for open-source apps. They register
  themselves at first sign-in. Nothing in the docs mentions SDKs or
  libraries.
- `agent_name_hint` must be "your app's actual name ... used
  consistently across installations". An SDK must therefore require the
  consuming app to supply its name and must never default to the
  crate's own name. Otherwise every app using the crate would register
  under the SDK's name.
- The UI rules ("Continue with ChatGPT" label, "Using ChatGPT plan",
  "Manage usage") bind the app, not a headless library. The crate should
  expose the Manage-usage URL and the states, and leave the buttons to
  the app.

### The DevKit licence makes clean-room provenance matter for a published crate

The licence ("Sign-in with ChatGPT DevKit Noncommercial License v1.0",
read from the repo's LICENSE) grants rights "solely for Noncommercial
Purposes". It defines noncommercial to exclude work "by or for a
business, employer, or client ... whether or not a fee is currently
charged". Derived works must carry the same licence.

It also says: "Independently authored software does not become a
Modified Work merely by calling, linking to, or communicating with the
Work through an interface."

Why this matters more for a published crate than for mxr-internal code:

- A published `MIT OR Apache-2.0` crate invites commercial downstream
  use.
- If DevKit expression leaked in (structure, naming, error-mapping
  tables, code), those downstream users would be infringing on our
  say-so.
- Protocol facts from the public docs carry no such restriction.

The current exposure:

- The research worker cloned the DevKit at `f723814abdcc` and read its
  source. The research doc carries at least one DevKit-derived detail:
  "The DevKit does the same in `packages/local/src/index.ts` lines
  99-102", which describes a 60-second refresh skew.
- That fact is generic OAuth practice and the docs' `earliest_refresh_at`
  covers the same ground. But the research doc is now a contaminated
  input for an implementer.
- This preflight read only the DevKit's LICENSE, README and commit
  titles, no source.
- Third-party SIWC implementations are also off limits. Some may derive
  from the DevKit, and two Rust ones are AGPL-3.0 (`wisp-science`,
  `solstone-journal`).

How to keep a build clean:

1. A fresh implementer (a new agent session) gets only:
   - a pinned snapshot of `developers.openai.com/siwc/llms-full.txt`,
     with date and sha256 recorded in the repo;
   - the RFCs listed in the component table and OIDC Core;
   - a requirements list written from those docs.
2. The implementer does not get the research doc's "How it would plug
   into mxr" section, the DevKit clone, or any third-party SIWC code. A
   scrubbed brief that cites docs URLs only is fine.
3. Every component cites its spec anchor in rustdoc (lesson 11 rule 1).
   A `PROVENANCE.md` in the repo records the inputs.
4. Generic OAuth helpers may come from Apache-2.0 `codex-login`, with
   attribution in NOTICE, but nothing SIWC-specific from it, because it
   has none.

### Name availability: every candidate is free except `allowance`

Checked against the crates.io API on 2026-10-02. crates.io treats `-`
and `_` as the same name.

| Name | Available | Notes |
|---|---|---|
| `siwc` | yes | OpenAI's own abbreviation, so it carries domain weight. On npm, `siwc` is "Sign-In with Conflux" and other packages use it for Cardano and CSD, so it is ambiguous outside Rust. Not an OpenAI mark as far as I could find. |
| `sign-in-with-chatgpt` | yes | Most findable. Contains "ChatGPT" (and so "GPT") in a product name, which the brand guidelines forbid. Avoid. |
| `chatgpt-plan`, `chatgpt-signin`, `openai-siwc` | yes | Same mark problem. Avoid. |
| `tokenshare` | yes | Mirrors the docs' own `token-sharing-open-source` path. Mark-free and descriptive enough. |
| `stipend` | yes | Playful: the user's plan funds the app. Mark-free, but needs the description to do the findability work. |
| `allowance` | **no** | Taken (0.10.0, 2022, unrelated). |

If the crate ever ships, the lesson 10 naming rule points to `stipend`
or `tokenshare` as the name, with a description that leads with "Sign
in with ChatGPT (SIWC)" for search.

## mxr can't be the seed or the first consumer, and BK has no other Rust consumer

- **No seed.** origin/main `3da0c119` has no SIWC code. The research doc
  sketches a `ChatGptPlanProvider`, but it is a plan, not code. Lesson
  11's ratio is undefined: with zero seed, every line is new.
- **mxr can't consume it.** Every LLM call site outside `crates/llm` at
  `3da0c119` sends mail content or mail-derived data:
  - `archive_ask`, `briefing`, `commitments_extract`,
    `decisions_extract`, `draft_compose`, `draft_eval`, `draft_refine`,
    `humanizer`, `promises`, `safety_llm`, `summarize`, `thread_gist`
    and `triage` in `crates/daemon/src/handler/`;
  - `crates/deliveries/src/extract.rs`;
  - `crates/relationship/src/{commitments,summary}.rs`.

  Even `humanizer.rs` rewrites drafts that "may be LLM output influenced
  by inbound mail" and adds voice context from history. The research doc
  found no published data terms for plan-usage requests, so mxr's
  processor rule blocks every one of these. mxr has no non-mail,
  on-demand LLM feature that could use the crate today.
- **No other Rust consumer.** BK's Rust repos under
  `~/code/planetaryescape` are `lazydap`, `lazydap-learn`, `ms-todo`,
  `list-unsubscribe`, `mail-query`, `mail-threading`, `mailbox-formats`
  and `mxr`. None except mxr calls an LLM API. `chatgpt-cli` is
  TypeScript on Bun and uses private web-app APIs. If it adopted SIWC,
  `pi-ai` or `@tanstack/ai-openai` would serve it, not a Rust crate.
- **An example CLI is not a consumer.** It would exercise the API, but
  it adds no production pressure, which is what lesson 10 test 3
  demands.

## Where it fails the publishing bar

Test 3 fails, on all three counts:

- **No production-credible seed.** There is nothing to extract, so the
  v0.1 would be written from scratch against a preview spec.
- **No consumer.** Nobody would run the code in anger before it went
  public, so its first real users would find its first real bugs.
- **New project, not extraction.** This is the lesson 11 case taken to
  its limit (10x the seed, here infinitely more). By that lesson's own
  wording, "the extraction question is premature."

Test 1 passes today but is the most perishable pass in this directory:
- a 3.2M-download framework (`rig-core`) already has a ChatGPT provider
  and an open token-storage issue;
- two Rust projects have SIWC crates one `cargo publish` away;
- npm closed its gap within three days.

Test 2 passes on policy complexity, not algorithmic depth.

## Recommendation: no-go; revisit only once mxr has an in-tree provider that works

Don't build or publish this crate now.

The path that could turn this into a real extraction later:

1. OpenAI publishes data terms that cover plan-usage requests, which is
   the research doc's blocker 1.
2. mxr then builds the provider in-tree, clean-room from the docs, as a
   module of `crates/llm` plus the daemon auth session.
3. It ships and runs for at least one release cycle.
4. Re-run this preflight. mxr's provider would then be a credible seed,
   mxr would be the first consumer, and the spec will have had time to
   settle.

If BK wants to spend effort on SIWC in Rust before then, contributing
to `rig-core` is the more useful move than a new crate. rig already
has the provider slot, the users, and an open storage-trait issue, and
the maintenance stays with them. The same clean-room rules apply.

If BK wants a standalone crate anyway, as a side project and not an
mxr extraction, make that call consciously:

- it is a new project with no consumer;
- it tracks a preview API, so expect churn;
- it must be built clean-room under a mark-free name.

## Open questions for BK

1. Is the goal an mxr extraction (this doc's bar), or a standalone side
   project where you accept no seed and no consumer? The answer changes
   the verdict.
2. Would you rather contribute SIWC support to `rig-core`? If so, the
   next step is to check rig's appetite on issue #2481 before writing
   anything.
3. Do you want to ask OpenAI in writing which data terms govern
   `chatgpt.tokens.use.direct` requests? This is already open in the
   research doc and gates everything mxr-side.
4. Name: are you fine with a mark-free name (`stipend`, `tokenshare` or
   `siwc`)? Or do you want to ask partnercomms@openai.com about
   `sign-in-with-chatgpt`?
5. Clean room: should the research doc's DevKit-derived line
   (`packages/local/src/index.ts` lines 99-102) be removed or quarantined,
   so the doc can be handed to a future implementer?
6. Numbering: this file is `08-*` at the root while
   `wont-do/08-outbound.md` already uses 08. Renumber to `14-*`?
