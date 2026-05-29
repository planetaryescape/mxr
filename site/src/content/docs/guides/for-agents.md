---
title: For agents
description: How to drive mxr from a coding agent, an LLM script, or your own loop. Three worked examples, the safety primitives that make them safe, and the boundaries the daemon won't cross.
---

mxr is built so an LLM agent can run it directly. The CLI emits structured JSON, every mutation has a dry-run, and the [HTTP bridge](/reference/bridge/) exposes the same surface for non-shell clients. There is no provider-specific SDK to wrap, no headless browser, no DOM scraping — the agent uses the same commands a human would.

This page is the practical guide. For the comprehensive list of what's safe to script, see the [automation contract](/guides/automation-contract/). For the field-level JSON shape, see [JSON output schemas](/reference/json-output/).

## Safety primitives, all the time

1. **Read first.** `mxr search`, `mxr cat`, `mxr stale`, `mxr sender`, `mxr summarize` never mutate. Use them to understand the situation before acting.
2. **Dry-run everything.** `--dry-run` works on every mutation; show the user the affected count before you run the real thing.
3. **`--yes` is opt-in.** Without `--yes`, mutations prompt. When stdin isn't a TTY (i.e. piped from an agent), pass `--yes` explicitly so the user has a clear "I authorise this batch" moment in the loop.
4. **Carry account scope through the whole loop.** If the user says "work account" or "personal account", resolve it with `mxr accounts`, then use `--account <selector>` on search, dry-run, mutation, and verification reads.
5. **Use `mxr history`.** Every mutation gets a `mutation_id`. Capture it; offer `mxr undo <id>` within ~60 seconds.

## Worked example 1 — Newsletter prune

**Goal:** unsubscribe from low-engagement subscriptions, archive the residue.

```bash
mxr subscriptions --rank --format json
```

The agent gets:

```jsonc
{
  "subscriptions": [
    {
      "sender": "newsletter@example.com",
      "list_id": "<list.example.com>",
      "messages_30d": 12,
      "open_rate_30d": 0.0,
      "last_opened": null,
      "unsubscribe": { "OneClick": { "url": "https://..." } }
    }
    /* ... */
  ]
}
```

The agent picks candidates with `open_rate_30d == 0` and `messages_30d >= 4`, presents them to the user, then dry-runs:

```bash
mxr unsubscribe --search 'list:<list.example.com> OR list:<...>' --dry-run
mxr archive --search 'from:newsletter@example.com OR from:<...>' --dry-run
```

User confirms. Agent runs:

```bash
mxr unsubscribe --search 'list:<list.example.com> OR list:<...>' --yes
mxr archive --search 'from:newsletter@example.com OR from:<...>' --yes
```

Agent verifies and reports:

```bash
mxr history --category mutation --limit 3 --format json
```

For account-specific requests, keep the selector on every command in the
sequence:

```bash
mxr subscriptions --account work --rank --format json
mxr unsubscribe --account work --search 'list:<list.example.com>' --dry-run
mxr unsubscribe --account work --search 'list:<list.example.com>' --yes
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

The agent never sends. The user reviews the generated body, saves a draft, or sends only after explicit approval.

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

## IPC bucket model (skim)

Behind the CLI, every request lands in one of four [IPC buckets](/guides/glossary/#ipc-buckets): `core-mail`, `mxr-platform`, `admin-maintenance`, `client-specific`. The first three are stable; the fourth is per-client view-shape and not part of the daemon contract. If you're scripting against the [HTTP bridge](/reference/bridge/), think in those buckets — they're the contract surface.

## Current limits (be honest)

- No first-party MCP server yet. The agent surface is the CLI plus the HTTP bridge; both are real and stable.
- No `--read-only` daemon mode yet. Use `safety_policy = "restricted"` or `"read-only"` in `[general]` to cap mutations daemon-wide if you need the guardrail.
- Account scoping is a CLI convention, not a separate agent permission model. `--account` limits mxr's command target set, but the agent can still run other commands with whatever OS access the user gave it.

If you need any of these as enforcement (rather than convention), file an issue — the design space is open.

## See also

- [Automation contract](/guides/automation-contract/) — exhaustive table of `--format`, `--dry-run`, stdin support
- [JSON output schemas](/reference/json-output/) — field names for `jq`
- [Unsubscribe](/guides/unsubscribe/) — header methods, body-link fallback, and safe cleanup flow
- [Recipes](/guides/recipes/) — pipelines for common tasks
- [Agent skill](/guides/agent-skill/) — install the mxr skill into Claude Code, Cursor, Continue, Aider
- [HTTP bridge](/reference/bridge/) — same surface over HTTP
- [API explorer](/reference/api-explorer/) — interactive Scalar reference
