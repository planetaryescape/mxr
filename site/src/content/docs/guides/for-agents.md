---
title: For agents
description: How to drive mxr from a coding agent, an LLM script, or your own loop. Three worked examples, the safety primitives that make them safe, and the boundaries the daemon won't cross.
---

mxr is built so an LLM agent can run it directly. The CLI emits structured JSON, the first-party MCP server exposes typed tools over stdio, every risky mutation has a dry-run or preview path, and the [HTTP bridge](/reference/bridge/) exposes the same daemon for non-shell clients. There is no provider-specific SDK to wrap, no headless browser, no DOM scraping — the agent uses the local mxr daemon.

This page is the practical guide. For the comprehensive list of what's safe to script, see the [automation contract](/guides/automation-contract/). For the field-level JSON shape, see [JSON output schemas](/reference/json-output/).

## Safety primitives, all the time

1. **Read first.** `mxr search`, `mxr cat`, `mxr stale`, `mxr sender`, `mxr summarize` never mutate. Use them to understand the situation before acting.
2. **Dry-run everything.** `--dry-run` works on every mutation; show the user the affected count before you run the real thing.
3. **`--yes` is opt-in.** Without `--yes`, mutations prompt. When stdin isn't a TTY (i.e. piped from an agent), pass `--yes` explicitly so the user has a clear "I authorise this batch" moment in the loop.
4. **Carry account scope through the whole loop.** If the user says "work account" or "personal account", resolve it with `mxr accounts`, then use `--account <selector>` on CLI search, dry-run, mutation, and verification reads. MCP tools take `account_id` where they can select an account; daemon profiles also enforce `allowed_accounts` for `agent` and `mcp` IPC origins.
5. **Use `mxr history` / `mxr activity`.** Every mutation gets a `mutation_id`. Capture it; offer `mxr undo <id>` within ~60 seconds. Activity rows include the request origin (`cli`, `agent`, `mcp`, etc.) and stay local.

## Worked example 1 — Newsletter prune

**Goal:** unsubscribe from low-engagement subscriptions, archive the residue.

```bash
mxr subscriptions --rank --format json \
  | jq '.[] | {
      sender_email,
      message_count,
      opened_count,
      replied_count,
      archived_unread_count,
      unsubscribe
    }'
```

The agent gets:

```jsonc
[
  {
    "sender_email": "newsletter@example.com",
    "message_count": 12,
    "opened_count": 0,
    "replied_count": 0,
    "archived_unread_count": 9,
    "unsubscribe": { "OneClick": { "url": "https://..." } }
  }
  /* ... */
]
```

The agent picks candidates with `opened_count == 0` and `message_count >= 4`,
presents them to the user, then dry-runs. `opened_count` is the number of
messages from that sender with the local `READ` flag set, not a tracking-pixel
or distinct-open count; `opened_count == message_count` means every message in
that sender bucket is already read locally.

```bash
mxr unsubscribe newsletter@example.com --dry-run
mxr archive --search 'from:newsletter@example.com' --dry-run
```

User confirms. Agent runs:

```bash
mxr unsubscribe newsletter@example.com --yes
mxr archive --search 'from:newsletter@example.com' --yes
```

Agent verifies and reports:

```bash
mxr history --category mutation --limit 3 --format json
```

For account-specific requests, keep the selector on every command in the
sequence:

```bash
mxr subscriptions --account work --rank --format json
mxr unsubscribe --account work newsletter@example.com --dry-run
mxr unsubscribe --account work newsletter@example.com --yes
```

## Worked example 2 — Meeting prep

**Goal:** for tomorrow's 1:1 with Sarah, gather the relevant threads from the last two weeks and draft an agenda.

```bash
mxr search 'from:sarah@example.com OR to:sarah@example.com after:2026-04-23' --account work --format json
```

The agent gets compact search rows with `message_id`, `from`, `subject`, `date`, `read`, `starred`, and `score`. When it needs thread context, it exports the matching search directly as markdown:

```bash
for tid in 01JFQ7K3M2X8N5R0VYZA9CTBPF 01JFQ8...; do
  mxr export "$tid" --format markdown
done
```

Or in one call with `--search`:

```bash
mxr export --account work --search 'from:sarah@example.com OR to:sarah@example.com after:2026-04-23' --format markdown > /tmp/sarah-context.md
```

Agent feeds the markdown into its summariser, then uses `mxr draft-assist` to generate a suggested reply body on stdout. Draft assist can use local relationship context when available and JSON output includes humanizer/voice-match metadata. The agent can show the body to the user or pass it into `mxr compose --body-stdin` / `mxr reply --body-stdin` after approval:

```bash
mxr draft-assist <thread_id> "Build a 1:1 agenda. Group by open question, decision needed, status update."
```

The agent never sends from draft-assist. The user reviews the generated body, saves a draft, or sends only after explicit approval. For MCP, `mxr_send_draft` also requires `confirm=true`; the daemon can still block the send if the active `mcp` profile has `allow_send = false`.

## Worked example 3 — CI failure cleanup

**Goal:** archive every CI failure email from last week whose underlying test has since been fixed.

```bash
mxr search 'from:noreply@github.com subject:"failed" after:2026-04-30' --format json
```

For each failure, the agent extracts the commit SHA and test name from the body (using `mxr cat <id> --view reader`). It cross-references against the local repo:

```bash
git log --since=1.week --pretty='%H %s' | grep -i 'fix.*test'
```

It builds a list of message IDs to archive. Dry-run:

```bash
echo 01JFQ... 01JFQ... | xargs mxr archive --dry-run
```

User confirms. Apply:

```bash
echo 01JFQ... 01JFQ... | xargs mxr archive --yes
```

Capture the `mutation_id` in the output. If the user notices an over-archive, the agent runs `mxr undo <mutation_id>` within 60 seconds.

## What stays local, what doesn't

- **Embeddings (semantic search)** — local, with locally-stored model weights. Never sent off-device.
- **`mxr summarize` and `mxr draft-assist`** — call your configured `[llm]` endpoint. That can be a local server (Ollama, LM Studio) or a remote provider. Configure in `config.toml`. The thread content goes wherever the LLM is.
- **Provider mail content** — passes through mxr to whatever provider the account is connected to (Gmail, IMAP). mxr never proxies through third parties.

If you want a strict local-only setup: set `[llm].base_url = "http://localhost:11434/v1"` for Ollama and `[search.semantic].enabled = true`. No third-party calls beyond your own provider.

## Token-budget tips

- Use `--limit` aggressively. `mxr search 'is:unread' --format json --limit 20` is plenty for triage.
- Use `--format ids` when you only need to drive a mutation. Saves tokens vs. full envelopes.
- Use `mxr summarize <thread_id>` for long threads instead of feeding `mxr cat` into the model.
- Use `mxr export <thread_id> --format llm` for thread context formatted for an LLM (omits redundant headers, strips signatures).

## MCP quick start

Run the server under an MCP client as a stdio command:

```bash
mxr mcp serve
```

Required daemon config is explicit. If `source = "mcp"` requests arrive without an `[agents.profiles.mcp]` profile, the daemon rejects them before handlers touch mail providers:

```toml
[agents.profiles.mcp]
safety_policy = "draft-only"      # read-only | restricted | draft-only | full
allowed_accounts = ["work"]       # account key, email, or account id
allow_send = false
allow_destructive = false
```

Use `safety_policy = "full"`, `allow_send = true`, and `allow_destructive = true` only for a client/session where the human approval loop is strong enough. MCP mutation and send tools still require `confirm=true`.

## IPC bucket model (skim)

Behind the CLI and MCP server, every request lands in one of four [IPC buckets](/guides/glossary/#ipc-buckets): `core-mail`, `mxr-platform`, `admin-maintenance`, `client-specific`. The first three are stable; the fourth is per-client view-shape and not part of the daemon contract. If you're scripting against the [HTTP bridge](/reference/bridge/) or MCP, think in those buckets — they're the contract surface.

## Current limits (be honest)

- MCP is stdio-only today; run `mxr mcp serve` under your client. There is no hosted MCP endpoint.
- Agent/MCP profiles enforce daemon requests by IPC origin, account allowlist, safety policy, send gate, and destructive gate. They do not sandbox the rest of the OS; a coding agent can still run any shell command you allowed outside mxr.
- Account scope must still be carried in prompts and commands. The daemon blocks out-of-profile accounts, but the best UX is to include account selectors in every search/read/mutation step.

If you need stronger OS sandboxing, run the agent in a separate user/session and give it only the mxr config/profile you intend.

## See also

- [Automation contract](/guides/automation-contract/) — exhaustive table of `--format`, `--dry-run`, stdin support
- [JSON output schemas](/reference/json-output/) — field names for `jq`
- [Unsubscribe](/guides/unsubscribe/) — header methods, body-link fallback, and safe cleanup flow
- [Recipes](/guides/recipes/) — pipelines for common tasks
- [Agent skill](/guides/agent-skill/) — install the mxr skill into Claude Code, Cursor, Continue, Aider
- [MCP server](/reference/mcp/) — first-party stdio MCP tools and profile gates
- [HTTP bridge](/reference/bridge/) — same surface over HTTP
- [API explorer](/reference/api-explorer/) — interactive Scalar reference
