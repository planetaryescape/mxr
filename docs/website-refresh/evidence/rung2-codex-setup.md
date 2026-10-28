# Codex local MCP setup: evidence for the existing-agent path

Checked 2026-10-23 against mxr `main` at `cc00d4081bb5bd7875c39da4641ff95888817d80`. This is setup research, not a runtime replay. Rung 1 is identified by the project brief as deployed and smoked; this note does not re-verify that deployment.

The current quick start's fixed demo timing statements conflict with blueprint 06, which says to remove them unless maintained measurements support them. Root classifies this as a **P2** documentation issue, rather than a release-blocking runtime failure. The agent-skill guide has no Codex install path and its only concrete path is Claude-specific (**P2**, onboarding gap); do not fill that gap by copying the Claude path.

## Use Codex on the same local demo daemon

Codex CLI can launch mxr's first-party stdio MCP server as a local process. That lets an existing local Codex installation call the same mxr daemon used by the user's CLI and web/TUI. It does not make an ordinary remote Codex agent able to reach a laptop socket: the MCP server process must run on a host that can reach the daemon. OpenAI's Codex MCP documentation confirms stdio servers run as local processes and that Codex CLI, its IDE extension, and ChatGPT desktop on the same Codex host share MCP configuration ([OpenAI Codex MCP](https://developers.openai.com/codex/mcp/)).

Install mxr by an existing supported method and confirm `mxr version`. Start the synthetic demo using `mxr demo`; keep its sticky `DEMO` indication visible. The current quick start says demo mode redirects subsequent mxr commands until `mxr demo stop`, but also makes elapsed-time promises the website blueprint explicitly says to remove unless maintained measurements support them. See [quick-start.md](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/site/src/content/docs/getting-started/quick-start.md#L8-L26) and the journey requirements in [blueprint 06](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/docs/website-refresh/blueprint/06-onboarding-and-agent-journeys.md#L5-L19).

For a Codex project-specific connection, add this table to that project's `.codex/config.toml` (the project must be trusted):

```toml
[mcp_servers.mxr]
command = "mxr"
args = ["mcp", "serve"]
```

`command` and `args` follow Codex's stdio schema. Omit `cwd` unless the project requires a working directory; Codex documents it as optional. If Codex cannot resolve `mxr` from its process `PATH`, replace `command` with the absolute path to the installed binary. Do not put daemon tokens, mailbox credentials, or personal paths in a checked-in snippet.

The installed CLI help (codex-cli `0.163.0`, checked with `codex --version`, `codex --help`, `codex mcp --help`, and `codex mcp add --help`) confirms the command contract:

```sh
codex mcp add mxr -- mxr mcp serve
codex mcp list
```

This CLI add command uses Codex's default config scope; help exposes no project-scope flag. For project-only setup, edit `.codex/config.toml` directly as above. For a personal user-wide setup instead, put the same `[mcp_servers.mxr]` table in `~/.codex/config.toml` or use the CLI add command. Project config is loaded only for trusted projects. Codex MCP configuration is shared among local Codex clients on the same host. OpenAI docs direct users to restart the IDE extension after adding there and the desktop app after setup; the installed CLI help does not specify live reload. Tell the reader to restart/reopen the Codex session if `codex mcp list` or `/mcp` does not show the server, then verify in the live session with `/mcp`.

## Keep mxr policy in the demo profile

Codex transport configuration and mxr authorization are separate files. The MCP process resolves the selected daemon when it starts, and each request identifies itself as `Mcp` to that daemon ([mcp/src/lib.rs](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/mcp/src/lib.rs#L68-L107), [serve_stdio](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/mcp/src/lib.rs#L2171-L2179)). Every non-demo CLI invocation checks the user's active demo marker and applies demo paths before command dispatch ([daemon/src/lib.rs](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/daemon/src/lib.rs#L75-L107)); therefore the separately launched `mxr mcp serve` process must run as the same OS user with access to that marker and demo runtime. A remote model can receive tool results after the local host call; mxr cannot keep that supplied context local ([blueprint 08](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/docs/website-refresh/blueprint/08-trust-and-models.md#L49-L63)).

Before the draft step, replace the placeholder below with the fixture-resolved demo selector; S3 owns that runtime discovery. Do not guess `work`, an email, or an ID. Edit only the demo's mxr `config.toml`, never the user's real profile:

```toml
[agents.profiles.mcp]
safety_policy = "draft-only"
allowed_accounts = ["<DEMO_ACCOUNT_SELECTOR_FROM_S3>"]
allow_send = false
allow_destructive = false
```

The TOML key is accepted by mxr config parsing, and profile fields/defaults are defined in [config/src/types.rs](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/config/src/types.rs#L648-L700); a parse test uses `[agents.profiles.mcp]` in [config/src/lib.rs](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/config/src/lib.rs#L231-L257). Daemon enforcement rejects MCP calls without a configured profile, rejects policy-disallowed operations, and separately checks send and destructive gates ([handler/mod.rs](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/daemon/src/handler/mod.rs#L2288-L2344)). With `draft-only` and both gates false, draft creation may be allowed while send remains denied; the actual end-to-end refusal and shared draft must come from S3, not this code read.

Codex's `env` sets variables for the stdio process; `env_vars` allowlists variables forwarded from Codex's local environment. The official docs do not establish which mxr selection variables survive a particular host's launch setup. mxr re-applies the same user's sticky demo selection when the `mxr` process starts, but verify `mxr demo status`, `mxr status`, and `mxr mcp serve` behavior on the actual host before relying on it. Do not hard-code `MXR_CONFIG_DIR` or daemon-token values in reusable Codex project config. The MCP server keeps its selected daemon target for tool calls; restart the MCP process after changing its daemon target. mxr config reload behavior for editing the demo MCP profile was not established in this research and needs the S3/host run before docs say “restart” or “reload.”

## What Codex's CLI-plus-skill option does

Codex may also run the documented `mxr` CLI directly with the repository skill as agent instructions. The current skill page has no Codex-specific install instructions: it lists Claude Code, Cursor, Continue, Aider and generic custom instructions, then gives a Claude-specific `~/.claude/skills/mxr/` path. Do not reuse that path for Codex. Root subsequently checked the [official local-skill discovery documentation](https://developers.openai.com/codex/skills/): repository skills live under `.agents/skills`, and user-level skills under `~/.agents/skills`. A repository-local mxr installation therefore needs `.agents/skills/mxr/SKILL.md` and its `references/commands.md`; the mxr repository already contains those two source files. Codex detects skill changes automatically; restart if the skill does not appear. This verifies the documented discovery contract, not an installation or live activation run.

The skill is guidance, not daemon enforcement. Ordinary CLI connections are identified as `cli`, for which `enforce_client_profile` returns without the `agent`/`mcp` profile checks; a skill cannot turn CLI requests into MCP-origin calls ([For agents](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/site/src/content/docs/guides/for-agents.md#L39-L45), [agent-skill.md](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/site/src/content/docs/guides/agent-skill.md#L17-L37), [handler/mod.rs](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/crates/daemon/src/handler/mod.rs#L2288-L2318)). If offering it, require the host's own shell/sandbox restrictions and keep the same synthetic-mail warning. Email content remains untrusted input under either path ([agent skill](https://github.com/planetaryescape/mxr/blob/cc00d4081bb5bd7875c39da4641ff95888817d80/.agents/skills/mxr/SKILL.md#L1-L31)).

Root rechecked the local version and `codex mcp add --help`, the official MCP and skill documentation, and mxr source at `cc00d4081bb5bd7875c39da4641ff95888817d80`. Both `[agent_surfaces.profiles.mcp]` and `[agents.profiles.mcp]` are accepted: `agent_surfaces` is the field name, with `agents` and `agent` aliases at `crates/config/src/types.rs:20`. The blueprint's spelling is therefore valid. No host configuration or personal skill directory was changed.

## Unresolved setup checks

- S3: resolve the actual demo account selector; prove read-only refusal, draft-only saved draft, human visibility of that same draft, and a denied send without delivery.
- Host run: confirm Codex discovers the project MCP entry, the launched process sees the active demo and correct daemon target, and the selected account matches the MCP profile; establish actual reload/restart behavior.
- Release docs: use the smoke and captured output at the deployed main SHA, not this note, for user-visible completion. Avoid fixed setup timing promises.
