# Evaluate Eve before replacing the assistant runtime

**Status:** prototype complete; production migration not approved by this spike.
**Observed:** 2026-11-01. **Primary source:** branch `spike/eve-agent-runtime`,
[`crates/assistant/prototype-eve/`](../../crates/assistant/prototype-eve/README.md).

## Recommendation

Continue toward a local Eve runtime, but keep the current assistant until the
active-step crash and deletion requirements pass on the version selected for
integration. The prototype proves that local model calls, tool execution,
approval and checkpointed recovery work without Vercel hosting. It does not
prove production readiness or that mxr's safety contracts survive an integration.

The tested version was `0.83.2`. The machine's npm release-age policy excluded
the current `1.0.0-rc.1`; no exception was made. Do not attribute these results
to the untested RC. Its changelog includes a changed event/client protocol and
interrupted-step recovery changes in `0.84.0`.

## What ran

Production `eve build` / `eve start`, loopback-only HTTP with a generated Basic
auth secret, local filesystem Workflow storage, no default shell/file/network
tools, no telemetry or local traces, and three synthetic-mail tools. No actual
mxr daemon, provider or production mailbox was involved.

Two independent modes avoid confusing runtime behaviour with model behaviour:

- A scripted OpenAI-compatible provider drives exact crash and approval cases
  through the real Eve runtime.
- Local Ollama `gemma4:latest` performs the search/preview/deny/approve smoke run.
  An initial natural-language search used a phrase that did not match the
  fixture's literal-substring search. The successful run explicitly requested
  the substring `shipping`; this is not a broad model-quality evaluation.

## Measured results

| Experiment | Observation |
| --- | --- |
| Unauthenticated session creation | HTTP 401 |
| Real Gemma search | Both synthetic shipping notices found; 3.6 seconds for the turn |
| Real Gemma archive proposal | Search, preview and runtime approval in 7.1 seconds |
| Denied approval | No mailbox mutation |
| Approved archive | Exactly the two previewed IDs changed; personal message untouched |
| Crash after approval step checkpoint | Restarted process resumed the saved approval and applied the exact selection |
| Duplicate approval | No second archive execution |
| Crash immediately after approval event | Earlier probes did not settle within 30 seconds; an active `turnStep` remained in persisted state |
| Crash after fake send effect, before tool result | One effect and one attempt recorded, but the continued turn did not recover within 30 seconds |
| Retire that interrupted send session | Reset failed; its stream still returned HTTP 200 and included the synthetic message body |
| Session retirement with zero retention | Session stream returned HTTP 410; inspected completed run metadata no longer had `input` |
| Full deletion | Not provided by retention alone: `$eve.title` still contained the user's request |

The build profile reported 7.6 seconds, of which 6.5 seconds was unnecessary
sandbox prewarming, and 9.7 MB of server output. Installed dependencies occupied
about 92 MB. Process health was ready in roughly 0.7-0.9 seconds using `eve start`.
These are single-machine observations, not performance guarantees.

Selected machine-readable observations are under
[`observations/`](../../crates/assistant/prototype-eve/observations/).
Raw scratch transcripts and state are intentionally ignored. The absence of
fixture strings in a plain/base64 file scan is not proof of secure erasure:
Workflow payloads can be compressed, and metadata has separate retention.

## What mxr must still own

- Keep existing IPC requests and CLI/web/TUI behaviour. Translate runtime
  events inside the daemon, not separately in each client.
- Keep the approval classification in `crates/assistant/src/gate.rs` as code
  enforced at the daemon/tool-execution boundary. Eve's default tool policy is
  permissive, and `once()` trusts future arguments too broadly for mxr.
- Bind approval to the stored exact call and preview. Only a verified human
  path may approve it or enable reversible-change trust; model tools must not
  access that authority. Do not just expose `AssistantDecide` to Eve.
- Preserve profile/account restrictions, workspace ownership, preview tokens,
  send safety, prompt-injection handling and activity source `assistant`.
- Preserve mxr's configured model routing, local/cloud relationship-data consent,
  key ownership, cancellation, grounded links and failed-action readouts.
- Keep application-level operation identity for irreversible side effects.
  Neither framework approvals nor replay checkpoints guarantee exactly-once send.

The replaceable code includes the generic loop in `turn.rs`, compaction in
`compact.rs`, and question plumbing in `ask_user.rs`. Those files total about
1,300 lines including tests and documentation. This is a candidate removal
area, not a measured net reduction. Much of the daemon integration and product
policy remains necessary. Shared `mxr-llm` functionality also serves other
features and cannot simply be deleted.

## Smallest next integration

1. Select a policy-permitted release and rerun both crash probes with bounded
   diagnostics. Explain the active-step outcome, including duplicate-side-effect
   handling, before replacing the existing loop.
2. Add a daemon-owned adapter for read-only tools against an isolated mxr demo
   profile. Eve's built-in MCP connection expects Streamable HTTP/SSE, while
   mxr's local entry point uses stdio and its existing assistant calls MCP
   in-process. Tool discovery must reuse real schemas rather than hand-maintain
   one TypeScript file per mxr tool.
3. Add one real preview/approval/commit workflow with daemon-side authority, plus
   a deletion path that removes sensitive titles and all associated runtime
   state. Run it through CLI and an existing UI before expanding the tools.

Also verify packaging without requiring Node or Docker to be installed by the
user. This spike did not implement that packaging, real daemon transport,
cross-model transcript migration, skills, subagents, schedules, compaction,
provider fallback, eval integration, or adversarial prompt-injection testing.
