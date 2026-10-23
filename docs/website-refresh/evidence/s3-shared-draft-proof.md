# S3 shared-draft proof

Checked 2026-10-23 against fresh HTTPS `main` at `cc00d4081bb5bd7875c39da4641ff95888817d80` (tree `d06c902fc47fba841312bed31fb8a5d098dd6310`). `HEAD`, fetched `origin/main` and merge base were the same commit. The published v0.6.156 tag resolves to `f39ae66578e3099e464967ab2f042e24b1ad8ec4`; `git diff --quiet v0.6.156 cc00d408 -- crates Cargo.toml Cargo.lock` passed. The read-only reference binary reports `mxr 0.6.156 (Mixer)`, and its retained macOS archive SHA-256 matches `8f428d8324bd34e342d14d66526459ac0290dc3bdb8f6caebf0ababf78b9b8c9`. Thus this proof exercised the released runtime code for the current main commit; the website docs changed after f39.

The synthetic MCP-to-web draft handoff was observed. MCP read-only refused creation; an owned config change to draft-only allowed local creation/update; the real web client rendered that same draft. A send attempt was rejected before request dispatch and the account's local Sent search remained empty. The UI does not display the numeric draft revision, so revision 2 is directly observed in MCP and associated with the web view through the same draft UUID route and revision-2-only body text; this report does not claim the UI displayed the revision number.

## Runtime and profile

The released binary was reused at `/Users/bhekanik/.t3/worktrees/mxr/t3-726e9266/.cache/website-s2/release/mxr`; it was not copied or modified. Its version/build ID from `mxr_status` was `0.6.156`, build ID `0.6.156:/Users/bhekanik/.t3/worktrees/mxr/t3-726e9266/.cache/website-s2/release/mxr:242845376:1792730997`.

All new runtime state and harness files are retained privately in `.cache/website-s3/`, registered in `ownership.md`. Config, data, tokens, bridge, socket, logs and attachments stayed below that root. `run-s3.sh` invokes the binary with `env -i`, explicit config/data/token/secrets/bridge-token/daemon-token/bridge-port/socket paths, `MXR_INSTANCE=website-s3-20261023`, and `MXR_ACTIVITY=off`. The exact macOS sticky marker `/Users/bhekanik/Library/Application Support/mxr/demo-active` was absent before the run and each runtime command was sandboxed to deny reads and writes to that exact path. No `mxr demo` command was used and no installed/global mxr config or real demo profile was changed.

The isolated account key is `synthetic`, email selector `draft@example.invalid`, and runtime account ID `90624098-478f-519e-9a59-3cc4dae1bf36`. The daemon's first startup synced 55 messages from the built-in fake provider; restart synced 0 additional messages. No real provider was configured, no provider credentials were read, and LLM and semantic model downloads were disabled. The local bridge bound only `127.0.0.1:19865`; the daemon used `.cache/website-s3/data/mxr.sock`.

The config uses `[agents.profiles.mcp]`. At this SHA, `MxrConfig.agent_surfaces` accepts both `agents` and `agent` TOML aliases (`crates/config/src/types.rs#L17-L21`), and config parsing covers `[agents.profiles.mcp]` (`crates/config/src/lib.rs#L232-L256`), so the blueprint spelling `[agent_surfaces.profiles.mcp]` is also valid. The actual `AgentProfileConfig` fields/defaults are at `crates/config/src/types.rs#L676-L700`.

The first isolated profile had `safety_policy = "read-only"`, `allowed_accounts = ["draft@example.invalid"]`, `allow_send = false`, `allow_destructive = false`. The real MCP `mxr_save_draft` call returned: `Request `save_draft` rejected by mcp profile safety policy`. The draft was not saved by that call.

I changed only the owned config to `safety_policy = "draft-only"`, leaving the same account allowlist and both gates false. I verified PID `8582`, executable, S3 socket and loopback listener, sent SIGTERM to that PID only, observed clean exit and socket removal, then started the owned daemon again as PID `19333`. This establishes that a daemon restart applies the profile change in this setup. It does not establish whether another release/config path reloads live.

## MCP and draft observations

`mcp_probe.py` speaks JSON-RPC over stdio to the actual released `mxr mcp serve`; it performed `initialize`, `tools/list`, and `tools/call`. The server identified itself as `rmcp 3.5.1`. The captured tool names/schemas are in the private fixture at `.cache/website-s3/evidence/mcp-tools.json`. Draft operations are `mxr_save_draft`, `mxr_get_draft`, `mxr_update_draft`, `mxr_draft_preview`, `mxr_send_draft`, and `mxr_list_drafts`. This is deterministic MCP protocol/tool replay from a local Python harness, not an agent or model reasoning run. No model call was made.

The account selector was resolved from the fixture's actual status output and used in the MCP allowlist. Saving through `mxr_save_draft` succeeded for draft ID `5e3a0b52-4345-4b85-b59c-4a05c026d9c1`, account ID `90624098-478f-519e-9a59-3cc4dae1bf36`, subject `S3 synthetic draft proof`, and recipient `recipient@example.invalid`. Its body was explicitly synthetic and said it was not intended for delivery. `mxr_get_draft` returned revision 1. I fetched that complete object, changed only the body, and called `mxr_update_draft`; MCP returned revision 2. A subsequent `mxr_get_draft` returned the same ID, account ID, subject and revision-2 body. `mxr_draft_preview` returned `allowed: true`, `verdict: safe`, and send identity revision 2. This was only a preview; no send occurred.

The `mxr_list_drafts` tool has no account selector. Under the account-limited MCP profile, its all-account request was refused (`list_drafts rejected by mcp profile account allowlist`). The account-scoped `mxr_get_draft` by the known ID succeeded. The actual web Drafts page independently listed the object, so this restriction did not prevent the cross-client check.

## Web-client observation

The T3 preview opened the actual localhost mxr web bridge and loaded its `/drafts` page. I opened the working fixture in Dia, including the draft's compose route. The Drafts list rendered the subject and recipient; its real client link was `/compose/5e3a0b52-4345-4b85-b59c-4a05c026d9c1`. Opening it displayed the actual saved-draft compose view from the daemon: account `draft@example.invalid`, subject `S3 synthetic draft proof`, recipient `Synthetic Recipient <recipient@example.invalid>`, and the exact revision-2-only body returned by MCP. The running web client rendered this view from the fixture daemon; the observation came from its DOM. The browser tab was agent-controlled and rendered the human-facing web client; no unfamiliar-reader or S4 comprehension claim is made.

`preview_snapshot` timed out once after 15 seconds. Per the run instructions, I did not retry snapshots. `preview_evaluate` read the actual DOM and captured its values in `.cache/website-s3/evidence/human-web-observation.json`. The numeric revision is not in the rendered UI; the unique body marker and draft-ID link tie the visible content to the MCP revision-2 result, but only MCP directly exposes the number.

## Prohibited send and persistence check

With the draft-only profile still active and `allow_send = false`, I called `mxr_send_draft` with `confirm = true`, draft ID and revision 2, and the exact preview operation ID. MCP returned JSON-RPC error `daemon error policy: Request `send_stored_draft` rejected by mcp profile safety policy`. No provider handler ran. Source confirms the global/client profile enforcement happens before handler dispatch (`crates/daemon/src/handler/mod.rs#L591-L599`); draft-only allows draft-class requests but rejects sends (`#L2264-L2285`, `#L2631-L2649`). The separate `allow_send` gate is checked after policy (`#L2319-L2331`), so this attempt proves the draft-only refusal while `allow_send=false` was configured; it does not isolate the send-gate error by itself.

Before and after the prohibited attempt, MCP `mxr_search` for `label:SENT`, scoped to the synthetic account ID, returned `total: 0`. A subsequent `mxr_get_draft` still returned the same draft ID/account/revision 2/body. The fake provider records accepted sends only inside its send implementation (`crates/provider-fake/src/lib.rs#L802-L831`), which the profile rejection prevented dispatching to. The prohibited request reached no provider and delivered no mail. The T3 browser's Send control was not activated.

## Reproduction and retained artifacts

The retained daemon was stopped after verification. To reproduce the web handoff, start the owned daemon in one terminal:

```sh
.cache/website-s3/run-s3.sh daemon --foreground
```

Then use the harness in another terminal while the daemon is active:

```sh
.cache/website-s3/run-s3.sh status --format json
python3 -I .cache/website-s3/harness/mcp_probe.py list
python3 -I .cache/website-s3/harness/mcp_probe.py call mxr_workspace '{}'
```

For saved MCP request arguments and exact responses, use the private JSON files under `.cache/website-s3/evidence/`; they include the local draft and synthetic account IDs. Do not reuse the private local bridge token or expose query strings, headers or cookies. The fixture root occupied 7,316 KiB at evidence closeout, under the 500 MiB cap. Owned daemon PID `19333` exited with return code 0 after verified SIGTERM; its Unix socket and loopback listener are closed. An accidental status call during closeout auto-started PID `39267`; I verified and stopped that daemon too. The bridge runs inside the daemon. The reference binary was reused in place. No build, Rust test, TypeScript change, push or deploy was made.

The harness received an inline simplifier review: stdlib-only protocol framing is necessary to call the real stdio server; mode/argument validation now happens before process launch; the harness closes stdin and waits for its child, terminating only that child if it misses the bounded wait. `sh -n run-s3.sh`, `python3 -I -m py_compile mcp_probe.py`, MCP initialize/list/calls, and the browser DOM observation completed. TypeScript review was N/A because no JS/TS changed.
