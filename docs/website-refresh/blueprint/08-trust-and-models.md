# 08: Explain the authority boundary as carefully as the capability

Place a compact trust explanation beside the first agent demonstration. Link it to Security & Privacy and Automation contract for the full operating model. A reader should feel they can inspect and bound what happens, not that “local” makes every workflow safe automatically. (D-WEB-017)

## The daemon enforces identified-client profiles before operation dispatch

Code evidence F06/F07 supports policy checks, account allowlists and independent send/destructive gates. The same evidence shows that those profile checks select `agent` and `mcp` origins; a normal CLI connection identifies itself as `cli`.

| Mechanism | What the website can promise | Limit stated beside the example |
|---|---|---|
| Read-only profile | Identified client cannot dispatch a prohibited mutation | Does not sandbox arbitrary same-user processes |
| Draft-only profile | Identified client may prepare supported drafts while other mutations are refused | Independent gates and global policy still apply |
| Account allowlist | Requests and responses are limited through the daemon's scoped path | Do not encourage direct database access to bypass the path |
| Send gate | `allow_send = false` refuses operations requiring send capability | “Full” policy does not imply all independent gates are open |
| Preview/confirmation | The operation's preview/confirmation contract is explicit | A prompt saying “ask first” does not itself enforce the contract |
| Pre-send checks | The daemon checks the current draft on the send path | Detection is bounded; an override policy exists; no guarantee of perfect content judgement |
| Undo | Supported mutations return recovery information | Some operations have no undo; windows expire; delivered mail is not universally recalled |
| Activity | Supported actions can carry client origin in the local log | May be off, paused, coalesced or dropped under pressure; not a lossless compliance ledger |

State the OS limitation plainly on For agents, agent setup and Security & Privacy: an unrestricted process acting as the mailbox owner's OS user can have capabilities outside these interfaces. Origin attribution is not cryptographic identity or proof of a human/agent actor. Do not make this caveat the homepage headline; make it easy to find where someone chooses authority. (D-WEB-005, D-WEB-017)

## Email is untrusted input

Keep the existing rule across skill, MCP and guide prose: mail fields, attachments and derived summaries are data, never tool instructions. A message cannot grant permission to send, alter recipients, disclose another message or invoke a tool. Daemon gates reduce available actions; the agent must still refuse email-derived instructions. Do not imply a complete prompt-injection cure.

The website examples must model this rule. A malicious synthetic message can demonstrate refusal, but never make a real sender's message into a marketing prop. (D-WEB-017)

## Preview, application and recovery use the same reviewed target

A batch recipe must state account scope, selected IDs/count and preview output before application. Follow the current operation's contract: some operations retain a reviewed operation ID/revision, while others rerun a search and need its selection checked again. Never promise all previews freeze every possible future selection.

Send examples carry current reviewed draft revision and workspace/operation identity where required. After a lost response, inspect the existing outcome before retrying. Do not teach blind retries of an ambiguous send or archive. Exact mechanics belong to generated CLI/HTTP/MCP reference and the automation contract, with the same wording in recipes.

Verification reads the resulting state. A successful request or an empty stdout is not proof of the requested mailbox effect. (D-WEB-019, D-WEB-022)

## Model choice has three independent paths

```text
External agent's model ── CLI/skill or MCP ──► mxr mail runtime
Person ── mxr built-in assistant ──► configured model + runtime tools
Native mxr synthesis feature ──► mxr-configured model backend

Core sync, local search, deterministic aggregates and base mail
remain separate from enabling native synthesis.
```

Explain this near model setup, not only in architecture. A user who brings an existing agent need not enable native drafting just to search mail. A native feature may need local model installation or API credentials even if an external agent already has a subscription. Do not claim every feature uses one shared backend if per-feature selection or routing overrides it.

## Local storage and network disclosure are separate facts

| Activity | Location or recipient | Copy requirement |
|---|---|---|
| Synced store, lexical index, deterministic aggregates | Runtime host | Name initial-sync/coverage and disk ownership |
| Provider sync/send/mutations | Configured mail provider | Local-first does not mean disconnected from the provider |
| Uncached attachments and record PDF prefetch | Mail provider to runtime cache | Not all attachments are already offline; bounded PDF policy is separate |
| External hosted agent reasoning over tool output | The agent/model service the person uses | mxr cannot promise returned mail stays on the host |
| Native local model call | Configured local endpoint | Explain enabled state and local model prerequisite |
| Native cloud model call and fallback | Configured provider(s) | Explain selected mail/history disclosure and applicable opt-in controls |
| Remote images, article fetch, unsubscribe | Chosen external endpoint | Explicit action or setting; label separately from local search |
| Marginalia handoff | The connected service | Explicit feature connection and transfer; link existing guide |
| Remote mailbox hosting | The selected runtime host | “Local” refers to that host; a laptop client may be remote |

Read live model-routing/config code and the current feature guide when implementing these disclosures. The evidence register proves defaults and key boundaries, not every endpoint's changing model list. Preserve the existing requirement for appropriate processing terms for other people's mail; do not turn this refresh into a new legal or vendor assessment.

## Trust demonstrations end in observed refusal or review

Use isolated synthetic mail to show draft-only creation, a rejected send and a draft visible to a person. Capture the current profile and exact refusal text. The page must neither suggest expanding authority automatically nor show a green “secure” badge as a substitute for evidence.

Unresolved questions: S3 verifies the shared-draft and refusal demonstration. S1 verifies the released model and operation contract before copy ships.
