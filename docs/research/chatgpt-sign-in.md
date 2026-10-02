# Sign in with ChatGPT: the mechanics fit mxr, the data terms don't yet

Researched 2026-10-02 for the email-modes decision in
`docs/blueprint/22-email-modes.md`, which adopts this path "only if its data
terms cover third parties' mail the way API terms do". mxr code references are
at `3da0c119` (origin/main; this checkout's `crates/` is identical to it).

## The bottom line: it launched as BK described, but nothing OpenAI has published puts this traffic under API data terms

The feature is real and matches BK's description. OpenAI launched "Sign in
with ChatGPT" at DevDay on 2026-09-29. It has an optional "ChatGPT plan usage"
permission that lets a local, open-source app send Responses API requests
billed to the user's Plus or Pro plan. No API key, client secret or hosted
server is needed.

Four things differ from "spend their ChatGPT subscription on the app's model
calls":

- Plan usage draws on the user's **Codex / ChatGPT Work** allowance, the same
  pool their coding agents use, not on ChatGPT chat messages.
- Only Plus and Pro can use plan usage. Business, Enterprise and Free are not
  listed as eligible.
- The no-approval self-registration path is for **open-source and locally run
  apps**. Commercial and hosted apps join a waitlist.
- None of the developer docs, help articles or policies I could reach says
  whether requests on this path are used for training or under what retention
  terms, and Plus and Pro have no DPA. The nearest published rule, for Codex
  on a personal plan, puts that traffic under the consumer "Improve the model
  for everyone" setting.

Verdict for mxr: the OAuth and daemon mechanics fit well. The privacy rule
blocks the feature for any mail content until OpenAI states in writing which
data terms apply. Background classification is also a poor use of a shared
coding allowance (see the last two sections).

## How this was researched

WebSearch had no budget left in this session, Exa returned 401 and Brave
returned an invalid-key error, so no search engine was used. Sources were
found and read like this:

- HN Algolia API (`hn.algolia.com/api/v1/search_by_date`) for launch stories
  and dates.
- `curl` on the Markdown twins of developers.openai.com and learn.chatgpt.com
  pages, including the single-file export
  `https://developers.openai.com/siwc/llms-full.txt`.
- `git clone` of `openai/sign-in-with-chatgpt-devkit` at `f723814abdcc`
  (committed 2026-09-29 09:08 -0700).
- help.openai.com and openai.com return 403 to curl, WebFetch and headless
  agent-browser (Cloudflare challenge). They were read through the `r.jina.ai`
  reader proxy on 2026-10-02. The quotes from those pages come from the proxy
  rendering, so check them against the live page before relying on them
  externally.

## Evidence

### 1. What launched: SIWC identity plus an optional "ChatGPT plan usage" permission, Plus and Pro only, self-serve for open source

Launch date and announcement:

- HN stories on 2026-09-29: "DevDay 2026 Recap" (openai.com/index/devday-2026-recap/,
  17:07Z), "Sign in with ChatGPT" (developers.openai.com/siwc, 18:17Z), "OpenAI
  Releases Sign in with ChatGPT DevKit" (20:19Z).
- DevDay recap, https://openai.com/index/devday-2026-recap/ : "We're making it
  easier to use your ChatGPT plan with other tools. Use your allowance across
  16 partners including Cognition's Devin, Notion, Vercel, T3, OpenClaw, and
  Dactyl, and control how much each can use. Eligible OpenAI usage counts
  toward your plan limits."
- Stratechery covered it on 2026-09-30 ("OpenAI Dev Day, Dot and OpenAI's
  Product Transition, Sign In With ChatGPT"). The analysis is paywalled and
  adds nothing technical.

Eligible plans:

- https://developers.openai.com/siwc/quickstart : "Eligible ChatGPT Plus
  and Pro users can use their ChatGPT plan for AI requests in participating apps
  and manage app usage and access in ChatGPT settings."
- https://help.openai.com/en/articles/20001542 : "Anyone can sign in to
  participating apps and sites with ChatGPT, but the option to use your ChatGPT
  plan is only available with Plus and Pro."
- The same help article also says: "All users can connect their account with
  supported open source tools. If you are a Plus or Pro user, you can also
  connect your ChatGPT account with eligible commercial tools." This conflicts
  with the line above. My reading is that "connect" here means identity only.
  That reading is not confirmed.

Who can integrate:

- Quickstart: "Sign in with ChatGPT is currently available to selected
  commercial partners through a limited trial. ChatGPT plan usage is available
  to all open-source partners and selected private clients."
- https://developers.openai.com/cookbook/articles/sign-in-with-chatgpt :
  "**At launch**, ChatGPT plan usage is available to open-source projects,
  personal projects that run locally, and selected private apps."
- https://developers.openai.com/siwc/token-sharing-open-source : "These docs
  explain ChatGPT plan usage for open-source and locally hosted apps. If you're
  interested in offering it in a paid or remotely hosted app, complete the
  interest form."
- https://developers.openai.com/siwc/request-client-id : "Sign in with ChatGPT
  is currently offered to a select group of commercial partners. To join the
  waitlist, complete the Sign in with ChatGPT interest form."

Regions:

- https://help.openai.com/en/articles/20001410-sign-in-with-chatgpt : "Sign in
  with ChatGPT is available globally to authenticated ChatGPT users, including
  users in Enterprise organizations." That line is about identity sign-in.
- For plan usage, the errors page lists a `403` meaning "A policy or permission
  check, such as the permitted serving region, prevented admission." No region
  list is published.

The Codex app-server auth path is no longer the sanctioned route:

- https://learn.chatgpt.com/docs/llms-full.txt (Codex app-server "Auth
  endpoints"): "If you've built a local or open-source application using Codex
  app-server authentication, you can continue using it, though we recommend
  migrating to Sign in with ChatGPT ... App-server authentication has never
  been permitted for commercial or hosted services."

### 2. Mechanics: OIDC authorization code plus PKCE, loopback redirect, dynamic client registration, no secret

All of the following is from
https://developers.openai.com/siwc/token-sharing-open-source/sign-in unless
noted.

The flow:

- "Start first-time registration with `client_id=dynamic_agent_client`, include
  the host's `ext_agent_host_id`, and set `agent_name_hint` to your app's actual
  name ... This direct flow needs neither a client secret nor a partner API key."
- Authorize endpoint: `https://auth.openai.com/api/accounts/authorize`. Token
  endpoint: `https://auth.openai.com/api/accounts/oauth/token`. PKCE uses
  `code_challenge_method=S256`.
- Redirect: "Use an HTTP loopback callback on `127.0.0.1` from initial
  registration onward, for example `http://127.0.0.1:1455/auth/callback` ...
  only the port may vary ... Do not substitute with `localhost`."
- Scopes: "Identity scopes: `openid profile email`. ChatGPT plan usage scopes:
  `offline_access resource.invoke chatgpt.tokens.use.direct`." Resource:
  `https://api.openai.com/v1`.
- The callback for a new registration returns the issued `client_id`
  (`oaiapp_...`). The app saves it per account and reuses it. It is bound "to
  the authenticated user and the workspace selected during registration"
  (overview page).
- "Check the token response's granted scopes for `chatgpt.tokens.use.direct`
  before proceeding to inference. A valid ID token alone does not authorize
  ChatGPT plan usage."
- Host ID: "Generate and persist a stable `ext_agent_host_id` for each host
  ... Use an opaque value, not an email, user ID, or other user-identifying
  value." The accepted formats are `urn:ietf:params:oauth:jwk-thumbprint:…`
  (recommended), `urn:uuid:` with a UUIDv4, or `did:key:`.

No device-code flow is documented for SIWC. For machines without a browser,
https://developers.openai.com/siwc/token-sharing-open-source/self-hosted-vms
says: "A `127.0.0.1` callback reaches the computer running the browser, not
the remote VM. Complete OAuth locally ..." and then transfer the credential
file over SSH.

Tokens, from https://developers.openai.com/siwc/token-sharing-open-source/token-reference :

- "Access tokens are valid for one hour (`expires_in: 3600`)."
- "Refresh tokens are valid for 30 days. Each successful refresh returns a
  replacement refresh token with a fresh 30-day lifetime."
- The token response also carries `earliest_refresh_at`. The access token is a
  JWT with `aud: https://api.openai.com/v1` and opaque
  `encrypted_auth_metadata`.

Refresh and revocation, from
https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions :

- "POST form-encoded `grant_type=refresh_token`, the issued `client_id` ...
  the saved `refresh_token`, and `resource=https://api.openai.com/v1` ... omit
  `scope` to retain the grant."
- "Store and use the latest replacement, and serialize refreshes for the same
  session so two processes do not race a rotating token." Refresh errors
  include `refresh_token_reused`.
- Sign-out revokes the refresh token at the `revocation_endpoint` from
  `https://auth.openai.com/.well-known/openid-configuration`.
- "OpenAI does not currently notify your tool when a user disconnects the app
  in ChatGPT settings" (errors page).

Endpoints and request shape, from
https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
and `/preview-limitations`:

- Only `POST https://api.openai.com/v1/responses` and `GET /v1/models` are
  covered. Calling any other route returns
  `subscription_sharing_route_not_supported`.
- "**Set `store` to `false` and `stream` to `true` on each HTTP inference
  request in this flow.**"
- "Unsupported fields: Omit `background`, `conversation`,
  `max_output_tokens`, `max_tool_calls`, `metadata`, `moderation`,
  `multi_agent`, `prompt`, `prompt_cache_retention`, `safety_identifier`,
  `temperature`, `top_logprobs`, `top_p`, `truncation`, and `user`."
- "explicit `{type: "message", role: "system"}` items are rejected." System
  guidance goes in `instructions`.
- The Files upload API is unsupported, which in practice also rules out the
  Batch API.
- Models come per account from `/v1/models` (`visibility == "list"`). The
  example slug is `gpt-6.1-sol`.

SDKs:

- The official DevKit is Node only: `@siwc/local` covers OAuth, storage,
  models and streaming, and `@siwc/react` covers the UI. Credentials are
  encrypted through an app-supplied provider (Electron `safeStorage` backed by
  Keychain in the example), with an interprocess lock and atomic writes.
- The DevKit is under the "Sign-in with ChatGPT DevKit Noncommercial License
  v1.0", which grants rights "solely for Noncommercial Purposes" and requires
  derived works to use the same licence. mxr is `MIT OR Apache-2.0`
  (`Cargo.toml` line 158), so DevKit code can't be ported into mxr. The docs
  describe the protocol well enough to implement from scratch.
- Python works by passing the access token as `api_key` (models-and-inference
  page).
- There is no Rust SDK.

How it relates to Codex CLI:

- Codex's own "Sign in with ChatGPT" is a separate, first-party client. It
  caches login "in a plaintext file at `~/.codex/auth.json` or in your
  OS-specific credential store", refreshes automatically, and offers
  `codex login --device-auth` (beta). Source: learn.chatgpt.com docs export.
- SIWC tokens can drive `codex app-server` through a custom `model_providers`
  entry with `env_key="ACCESS_TOKEN"` and `wire_api="responses"`
  (https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server).
- Both draw on the same Codex / Work allowance.

### 3. Limits: it shares the user's Codex allowance, Plus has a 5-hour cap shared across all apps, and a limit is a hard 429 with no fallback

- help 20001542: "When you turn this on, eligible AI requests count toward the
  ChatGPT Work and Codex usage included in your plan. You can set a weekly
  usage limit for each app".
- help 20001542: "An app's weekly usage limit controls the percentage of your
  overall weekly ChatGPT usage that app can consume. The app can reach its
  limit while you still have ChatGPT usage remaining."
- profiles-and-sessions: "For ChatGPT Plus users, the five-hour usage limit is
  shared across all apps where they use their ChatGPT plan, including private
  and open-source clients ... The five-hour usage limit does not apply for Pro
  users."
- Credits after the limit are opt-in. help 20001542: "This is off by default"
  and "An app must have its usage limit set to 100% to be able to use credits".
- The error for an exhausted limit
  (https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery):
  `subscription_sharing_usage_limit_exceeded`, HTTP 429: "Pause new requests
  that use the user's ChatGPT plan and link to ChatGPT settings → Usage. Do not
  assume the entire plan is empty or infer a reset time from this code alone".
- Models-and-inference: the same error "can arrive as `response.failed` ...
  after streaming has begun".
- Errors before a stream opens may be `{"detail":"..."}` rather than the
  standard error object (401, 403 or 503).
- "ChatGPT plan usage errors stop inference. OpenAI does not silently switch
  the request to another billing path."
- No requests-per-minute or tokens-per-minute figures are published for this
  path. HN users on 2026-09-29 report not knowing how many requests a 5-hour
  or weekly window holds (https://news.ycombinator.com/item?id=49897190).

### 4. Data use: the SIWC docs are silent, and the nearest published rules are consumer terms with no DPA for Plus or Pro

What OpenAI says about the SIWC path itself:

- help 20001542: "Using your ChatGPT plan does not by itself give the app or
  site access to your ChatGPT conversations or memories, and it does not share
  an OpenAI API key."
- help 20001410, "Privacy and data OpenAI collects", lists only
  authentication, authorization and security data. It does not mention prompt
  content.
- The developer docs require `store: false`. That turns off application-state
  storage for the response. Nothing in the docs ties it to training or
  abuse-monitoring retention.
- In all of `siwc/llms-full.txt`, the two help articles and the cookbook,
  there is no statement about training on, or retaining, the content of
  plan-usage requests.

The nearest rules OpenAI has published:

- Codex on a personal plan follows consumer controls.
  https://help.openai.com/en/articles/7730893-data-controls-faq : "If you use
  Codex on a personal ChatGPT plan, Improve the model for everyone also applies
  to your Codex tasks."
- The learn.chatgpt.com Codex auth docs: "When you sign in with ChatGPT, Codex
  usage follows your ChatGPT workspace permissions ... and ChatGPT Enterprise
  retention and residency settings. With an API key, usage follows your API
  organization's retention and data-sharing settings instead."
- Consumer Terms of Use (https://openai.com/policies/row-terms-of-use/,
  effective 2026-01-01), which "apply to your use of ChatGPT ... and OpenAI's
  other services for individuals": "We may use Content to provide, maintain,
  develop, and improve our Services" and "If you do not want us to use your
  Content to train our models, you can opt out".
- Privacy policy (https://openai.com/policies/row-privacy-policy/): "we may use
  Content you provide us to improve our Services, for example to train the
  models that power ChatGPT."
- Business and API terms are the opposite.
  https://openai.com/enterprise-privacy/ covers "inputs and outputs from
  ChatGPT Business, ChatGPT Enterprise, ChatGPT for Healthcare, ChatGPT Edu,
  ChatGPT for Teachers and our API Platform" and says "We do not train our
  models on your data by default".
- API data handling (https://developers.openai.com/api/docs/guides/your-data):
  "data sent to the OpenAI API is not used to train or improve OpenAI models
  (unless you explicitly opt in)". `/v1/responses` keeps abuse-monitoring logs
  for 30 days.
- DPA: "we are able to execute a Data Processing Addendum (DPA) with customers
  for their use of ChatGPT Business, ChatGPT Enterprise, and the API"
  (enterprise-privacy). Plus and Pro are not in that list.

### 5. Developer requirements: no review for open source, but strict branding and UI rules and no published SIWC developer terms

- Registration: open-source apps register themselves at first sign-in through
  `dynamic_agent_client`. No application or review step is documented for
  them. Commercial, hosted and paid apps go through the interest form.
- Branding, from https://developers.openai.com/siwc/ui-ux-guidelines and the
  quickstart:
  - Label the button "**Continue with ChatGPT**" and use approved OpenAI
    branding.
  - Show a first-sign-in modal: "You're using your ChatGPT plan" with a "Got
    it" button.
  - Show "**Using ChatGPT plan** near the composer or model selector" with a
    "**Manage usage**" link to `https://chatgpt.com/settings/usage`.
  - On a usage-limit error, make "Manage usage" the primary action.
  - "Your app must clearly show which of its plans support ChatGPT plan usage."
- Errors-and-recovery suggests offering a fallback the user chooses: "offer a
  clear choice: enable ChatGPT plan usage or configure another supported
  billing path, such as the user's own API key." Using it alongside a BYOK
  provider is therefore anticipated, not forbidden.
- I found no SIWC-specific developer terms. The consumer Terms forbid
  "Automatically or programmatically extract data or Output" and "circumvent
  any rate limits". How those apply to an officially sanctioned programmatic
  path is not stated.

### 6. Background batch use: neither forbidden nor described

- No page forbids unattended or background requests.
- No page describes them either. Every example and guideline assumes
  interactive use: a composer indicator, a model picker, a paste action, a
  coding harness.

## Inference

### The data-terms gap blocks mxr's privacy rule

mxr's rule is that mail content, which carries third parties' personal data,
goes only to a processor the user chose, under terms that cover it. Under
Plus or Pro, this path offers:

- no DPA;
- no published statement that requests are excluded from training;
- Codex-on-personal-plan precedent that puts the same allowance under the
  consumer training toggle.

The likeliest reading is that SIWC requests from Plus and Pro accounts fall
under consumer data controls: used for training unless the user has turned
off "Improve the model for everyone", with OpenAI acting as a controller, not
a processor. That reading is not confirmed. Either way, the bar in
`22-email-modes.md` ("adopted only if its data terms cover third parties' mail
the way API terms do") is **not met** on today's evidence.

The opt-out toggle isn't a fix. It is consent-style and user-controlled, there
is still no Art. 28 contract, and mxr can't verify the toggle's state.

### Background classification is the worst fit for this allowance

- It spends the allowance the user's coding agents use. On Plus it also
  spends the 5-hour window shared with every other SIWC app, so a sync
  backlog of hundreds of messages could lock BK out of Codex mid-afternoon.
- The route rejects `max_output_tokens` and `temperature`, so mxr can't bound
  or steady a classifier's output on this path.
- Streaming is required and there is no batch endpoint.
- The 429 gives no reset time, so a daemon can only back off blindly.

Interactive, user-initiated features (draft, summarize, ask) match how OpenAI
frames the product much better.

### The mechanics suit a local daemon well

- Loopback PKCE needs no server and no secret.
- The daemon is the natural single owner of a rotating refresh token, which
  is exactly what "serialize refreshes" asks for.
- A 30-day sliding refresh survives normal use. A daemon that is off for more
  than 30 days means signing in again.

## How it would plug into mxr

Paths are at `3da0c119`.

### A new `ChatGptPlanProvider` in `crates/llm`, with tokens owned by the daemon

`crates/llm` "deliberately depends on no internal crates"
(`docs/blueprint/01-architecture.md` line 265), so the provider can't read
the keychain itself. The plan:

- **`crates/llm/src/chatgpt_plan.rs` (new).** `ChatGptPlanProvider`
  implements `LlmProvider` (`crates/llm/src/lib.rs` line 215). It takes an
  `Arc<dyn AccessTokenSource>` (a new trait in the same crate) that the daemon
  implements.
  - `complete()` POSTs `/v1/responses` with `store: false, stream: true`.
  - It moves `ChatRole::System` messages into `instructions`.
  - It drops `max_tokens` and `temperature`; `CompletionRequest` carries both
    today (lib.rs lines 173-177).
  - It parses SSE until `response.completed`.
  - `base_url()` returns `Some("https://api.openai.com/v1")`, so the existing
    `is_loopback_endpoint` (`crates/llm/src/endpoint.rs`) correctly reports it
    as cloud. `llm_endpoint_is_local`, `relationship_data_allowed` and
    `GistPolicy::pin` (`crates/daemon/src/handler/thread_gist.rs` line 151)
    then need no change to label it Cloud.
- **Error mapping.**
  - `subscription_sharing_usage_limit_exceeded` needs a new
    `LlmError::PlanLimitReached` that carries the Manage-usage URL. The
    existing `RateLimited { retry_after_secs }` would invent a reset time,
    which the docs say not to do.
  - The background breaker (`crates/llm/src/background.rs`) should treat that
    error as a long pause, not a failure streak.
  - `subscription_sharing_user_not_eligible` and `chatpass_v2_*` map to a
    terminal "not available for this account". 401 maps to `Unauthorized`.
- **Rust crates.** `reqwest` is already present; mxr already has it.
  - SSE: a small parser, or `eventsource-stream`.
  - PKCE: `sha2` and `base64`, both already workspace deps.
  - ID-token verification: `jsonwebtoken` with JWKS from the OIDC discovery
    document.
  - Host ID: `uuid` needs the `v4` feature added. It has `v5` and `v7` today
    (`Cargo.toml` line 259).
  - `yup-oauth2` (used by Gmail) doesn't fit. It can't send `resource`,
    `ext_agent_host_id` or `agent_name_hint`, and it can't take a `client_id`
    back from the callback.

### Tokens go in the OS keychain through `mxr-keychain`, following Gmail's pattern

- **Storage.** `crates/keychain/src/lib.rs` exposes
  `get_password` / `set_password` (keychain on macOS, `keyring` elsewhere).
  `crates/provider-gmail/src/auth_storage.rs` already stores yup-oauth2 token
  JSON there under `KEYCHAIN_SERVICE = "mxr-gmail-oauth"`. Use a
  `mxr-chatgpt-oauth` service holding one JSON record per issued `client_id`:
  email, `sub`, `client_id`, `id_token`, access token, refresh token, scopes,
  expiry and `earliest_refresh_at`.
- **What not to copy.** Don't use Outlook's model, which is plaintext JSON at
  0600 (`crates/provider-outlook/src/auth.rs`, `save_tokens`).
- **Missing pieces.** `mxr-keychain` has no delete function, and logout needs
  one.
- **Host ID.** The non-secret `ext_agent_host_id` lives in a small file in
  mxr's data dir and must be created before the first sign-in.

### Login runs as a daemon auth session, driven by `mxr llm login chatgpt`

- **The pattern to follow.** Per `AGENTS.md`, new capabilities are daemon IPC
  plus CLI JSON. `crates/daemon/src/handler/auth_sessions.rs` already runs
  Gmail and Outlook OAuth as daemon-owned sessions.
- **Daemon side.** Add a `ChatGpt` session type. The daemon binds
  `127.0.0.1:<port>/auth/callback`, builds the authorize URL (state, nonce,
  PKCE, host ID, `agent_name_hint=mxr`) and returns it. Then it waits for the
  callback, exchanges the code, verifies the ID token and checks for
  `chatgpt.tokens.use.direct`.
- **CLI side.** `crates/daemon/src/commands/llm.rs` (today `Status` and config
  actions) gains:
  - `login chatgpt`, which opens the URL with the `open` crate, already a
    dependency;
  - `logout chatgpt`, which revokes the token and then clears it;
  - plan state in `status`.
- **Protocol.** Requests go in `crates/protocol/src/lib.rs` next to
  `GetLlmStatus`.
- **Diagnostics.** `crates/daemon/src/handler/diagnostics/mod.rs` reports
  `api_key_present` today. Add the connection state and scopes there, never
  the tokens.
- **Headless installs.** There's no device code flow. A headless install
  needs the "sign in where the browser is, then copy the credential" path from
  the self-hosted-VM doc, or SSH port forwarding of the loopback port.

### The daemon refreshes tokens on demand behind one lock

- The daemon's `AccessTokenSource` refreshes when the token is within 60 s of
  expiry and `earliest_refresh_at` has passed, as the public token docs
  describe.
- A `tokio::Mutex` serializes refreshes.
- The rotated refresh token is written to the keychain before the new access
  token is used.
- The CLI and web never refresh tokens themselves.
- On `invalid_grant` or `refresh_token_reused`, the daemon marks the
  connection as needing sign-in and stops. It doesn't loop.

### Config selects the provider kind, and the privacy gate needs a terms dimension

- **Config.** `LlmConfig` in `crates/config/src/types.rs` (line 225) assumes
  an OpenAI-compatible URL plus `api_key_env`. Add a provider kind
  (`openai_compatible` or `chatgpt_plan`) to the base config and the
  overrides. Then `build_llm_provider` in `crates/daemon/src/state.rs`
  (line 198) can branch on it. That way only, say,
  `llm.overrides.draft_assist` points at the plan.
- **Why locality isn't enough.** Today's gate is locality only:
  `relationship_data_block_reason`, `relationship_data_allowed` and
  `llm_endpoint_is_local` in `state.rs` (lines 330-370). A cloud endpoint plus
  `allow_cloud_relationship_data` passes. That is right for an API key under
  API terms. It is wrong for a path whose content terms are unpublished or
  consumer.
- **The fix.** Give each provider a data-terms attribute (for example
  `DataTerms::Api | Unverified`). Make background classification and the
  relationship features refuse `Unverified` regardless of the cloud flags.
  Name the reason in `PrivacyBlocked`, the way the current gate does.
- **UI and docs.** "Continue with ChatGPT", "Using ChatGPT plan" and "Manage
  usage" go in the web app and the TUI status. Docs pages to update:
  `site/src/content/docs/guides/llm-features.md` (provider table and cloud
  guard), `site/src/content/docs/reference/config.md` and
  `site/src/content/docs/guides/security-and-privacy.md`.

## Blockers

1. **Data terms aren't published.** No OpenAI page says whether plan-usage
   requests are trained on or retained, and Plus and Pro have no DPA. This
   alone blocks sending any mail content, background or interactive, under
   mxr's processor rule.
2. **The allowance is shared with Codex.** Unattended classification would
   spend the user's coding allowance, and on Plus their 5-hour window. The
   route also rejects `max_output_tokens` and `temperature`.
3. **mxr's status as an open-source app is an assumption.** The self-serve
   path covers open-source and personal local apps. mxr qualifies as long as
   it stays free and open source. A paid tier would move it to the commercial
   waitlist.
4. **No Rust SDK, and the DevKit's licence bars reuse.** The OAuth, JWKS and
   SSE code has to be written from the docs.

## Unresolved questions

- Which data terms govern `chatgpt.tokens.use.direct` requests: consumer data
  controls, or API terms? This can only be settled by OpenAI in writing.
  Asking them is BK's call.
- Can Business or Enterprise workspaces use plan usage? If so, would their
  no-training default and DPA cover these requests? The docs bind clients to a
  "workspace", but the eligible list names only Plus and Pro.
- Is a commercial mxr in scope later? That decides whether the self-serve path
  stays available.
- How many small requests does a 5-hour or weekly window hold? This is
  unpublished. Measuring it would require sending content, which blocker 1
  forbids.
