# 06: A newcomer must reach one useful result without connecting real mail

Revise the current quick start and add `/getting-started/agent-quick-start/`. The first remains the shared tutorial; the second is the existing-agent tutorial. For agents stays the operating how-to, and MCP/CLI reference stays reference. (D-WEB-014, D-WEB-015, D-WEB-022)

## The common quick start proves local mail before choosing a client

Remove the assumption “The TUI is what you'll use most.” Open with the result: inspect synthetic mail locally, then choose a client or attach an agent. Keep the installed binary as the prerequisite and the existing demo behaviour, including its sticky profile warning, visible.

| Step | Reader action | Observable result | Failure path |
|---|---|---|---|
| 1 | Install from the existing supported method | `mxr version` reports the installed binary | Link installation and platform notes |
| 2 | Start `mxr demo` using the documented mode | Synthetic profile and DEMO marker visible | Explain progress and how to stop; no fixed latency promise |
| 3 | Query a local result | Structured search or subscription result with known synthetic evidence | Empty result names query/coverage, not a confident “nothing found” conclusion |
| 4 | Choose CLI, web or TUI | Same demo accounts/data in the chosen surface | Inspect daemon/profile targeting |
| 5 | Optionally attach an agent | Continue to agent tutorial without losing demo context | Host/tool setup troubleshooting |
| 6 | Exit demo before adding real accounts | `mxr demo stop` and a clear transition | Confirm targeting before connection |
| 7 | Connect and sync only by explicit user action | Sync status and usable partial history | First sync explains incomplete coverage and provider errors |

The current demo can use canned model responses. Label that beside any model-assisted demo result. The tutorial proves the interface and workflow; it does not benchmark model quality. Avoid exact “30–45 seconds” or “five minutes” promises unless a maintained measurement justifies them. Small demo options already available may shorten the path; verify them rather than adding new seeding features.

## The agent tutorial ends with work the person can see

Prerequisites: installed binary, running isolated demo, an agent host with local shell or local stdio MCP support. A remote hosted agent does not gain access to a laptop's socket merely because the docs say “MCP”. Explain that boundary and link the existing remote mailbox guide when relevant.

Two explicit entry choices:

- MCP: the host starts `mxr mcp serve`; show one tested host configuration, point to MCP reference for tools and transport, and use a bounded `mcp` profile.
- CLI with skill: install the existing skill using verified host-specific instructions. State that the skill supplies guidance; it does not enforce daemon permissions, and ordinary CLI requests use `cli` origin. Restrict the host's execution authority separately.

Do not combine both installation methods into a mandatory sequence. They are alternatives over the same runtime. Add a Codex recipe only after checking its current official/local host documentation; do not infer its skill path from Claude's. (D-WEB-008, D-WEB-017)

The demonstration prompt is a task request, not a message to another person:

> Use the mxr demo mailbox. Find a recent synthetic conversation, show the evidence you used, and save a short reply as a draft. Do not send or change any other mail. Tell me the draft ID and account so I can open it in mxr.

For the default MCP tutorial, move from read-only to an explicitly configured draft-only profile before saving. Show that read-only refuses draft creation; draft-only permits drafting while the independent send gate remains closed. The human then finds the same draft ID/account in web/TUI and reads the stored content. Opening a compose screen with similar text is insufficient proof if it is a different draft.

MCP configuration can be shown as this code-backed example, scoped to the demo's actual account selectors discovered during replay:

```toml
[agent_surfaces.profiles.mcp]
safety_policy = "draft-only"
allowed_accounts = ["DEMO_ACCOUNT_SELECTOR"]
allow_send = false
allow_destructive = false
```

The implementing agent must replace the placeholder with a fixture-resolved account selector and verify it. Do not overwrite a user's configuration, present the whole table as a universal default, or grant all-account/full authority to save setup steps. State restart/reload requirements only after verifying the actual configuration lifecycle. (D-WEB-017)

## Read state, draft state and permission state have separate checks

The tutorial records the selected account, message/thread IDs, saved draft ID, content/revision, client URL/profile and permitted operation. It captures a prohibited send refusal without allowing actual delivery. It inspects applicable activity when enabled and explains why CLI origin may be `cli`.

A useful error teaches the correction: “The MCP profile is read-only; creating a draft needs draft-only authority.” Do not advise disabling safety globally. A draft that fails a pre-send check remains a draft; the tutorial does not auto-fix and send it.

## Human onboarding joins Now and the five modes

From the demo, open Now, follow a Messages item, inspect a To do, glance at Updates, read a newsletter, find a record. The full tour belongs in Your day, not the install page. Retain CLI equivalents as short alternate paths. Keep human modes legitimate for a user who never configures any AI.

The intended feeling is steady progress: one action, one visible result, one clear continuation. Avoid a long setup wall before any useful mail appears. (D-WEB-012, D-WEB-016)

Unresolved questions: Q6 assumes demo-first and existing-agent-first ordering. S2 and S3 must replay these flows before their commands and captures are published.
