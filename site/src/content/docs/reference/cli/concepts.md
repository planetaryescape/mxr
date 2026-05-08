---
title: CLI concepts
description: Query operators, search modes, JSON output, IPC buckets — the things that span every command.
---

The per-command pages are auto-generated. Everything that's true _across_ commands lives here: query syntax, search modes, output formats, the IPC contract.

## The one line that does most of it

```bash
mxr search '<query>' --format ids | xargs -I{} <command> {}
```

That's the entire composition story. Every list/search command writes
one ID per line under `--format ids`; every other command accepts an
ID as a positional argument. So search becomes the universal selector,
and any other tool — your own script, fzf, jq, GNU parallel — slots
in where `<command>` is.

Two equivalent forms exist for read commands that take an ID:

```bash
# Option A: shell pipeline (works with any partner tool)
mxr search 'from:alice newer_than:7d' --format ids \
  | xargs -I{} mxr cat {} --view reader

# Option B: --search flag (daemon-native, snapshot-consistent,
# no client-side fanout, plus --first / --limit modifiers)
mxr cat --search 'from:alice newer_than:7d'
mxr cat --search 'from:alice' --first         # latest match only
mxr cat --search 'from:alice' --limit 10      # top 10 by date desc
```

`--search` lives on every read command that takes a single ID
(`cat`, `thread`, `headers`, `summarize`, `draft-assist`, `open`,
`attachments list`) and on every mutation (`archive`, `trash`,
`label`, `snooze`, etc.). When you're chaining mxr-on-mxr, prefer
`--search` — it resolves once inside the daemon, with the same view
the daemon mutators see. When you're piping into a non-mxr tool, use
the `--format ids | xargs` form.

For real recipes — `fzf` interactive pickers, `jq` digests, parallel
`xargs`, cron / systemd, `watch` dashboards, agent prompts — see the
[Recipes guide](/guides/recipes/).

## Query operators

The query parser accepts Gmail-style operators. The same grammar drives `mxr search`, `mxr count`, `mxr saved add`, the TUI `/`, and the `--search` flag on core batch mutations.

| Operator | Example | Notes |
|---|---|---|
| `from:` / `to:` / `cc:` / `bcc:` | `from:alice@example.com` | substring + display-name match |
| `subject:` | `subject:"quarterly review"` | quoted phrase, exact within tokens |
| `body:` | `body:reimbursement` | full-text body |
| `label:` | `label:inbox` | matches by `provider_id` (case-insensitive) |
| `in:` | `in:sent` | folder/label shortcut |
| `category:` | `category:promotions` | provider category mapped to labels where available |
| `list:` | `list:<newsletter.example.com>` | List-Id header |
| `deliveredto:` | `deliveredto:alias@example.com` | Delivered-To header |
| `rfc822msgid:` | `rfc822msgid:abc@example.com` | RFC 822 Message-ID header |
| `is:` | `is:unread`, `is:starred`, `is:answered` | flags |
| `has:` | `has:attachment`, `has:pdf`, `has:image` | attachment/body metadata where indexed |
| `before:` / `after:` | `after:2026-01-01` | YYYY-MM-DD |
| `older_than:` / `newer_than:` | `older_than:30d`, `newer_than:7d` | days |
| `older:` / `newer:` | `older:30d`, `newer:7d` | aliases for `older_than:` / `newer_than:` |
| `size:` / `larger:` / `smaller:` | `larger:10m` | message size filters |
| `filename:` | `filename:invoice.pdf` | attachment names |
| `OR`, `AND`, `NOT`, `(...)` | `from:vendor AND (label:bills OR label:travel)` | `AND` is implicit between bare terms |

## Search modes

`mxr search` accepts `--mode lexical|hybrid|semantic`. Default is whatever `config.search.default_mode` is set to.

- `lexical` — Tantivy BM25 only. Exact, fast, deterministic.
- `hybrid` — lexical + dense retrieval, fused with reciprocal-rank fusion. Best recall.
- `semantic` — dense retrieval only. Useful when you don't know the keywords.

Field prefixes route to chunk types under hybrid/semantic:

- `subject:` → header chunks
- `body:` → body chunks
- `filename:` → attachment-origin chunks

```bash
mxr search "body:house of cards" --mode hybrid --explain
mxr search "subject:quarterly report" --mode hybrid --explain
mxr search "filename:roadmap" --mode hybrid --explain
```

## Output formats

Most reads accept `--format <FORMAT>`. Available values per command live in the auto-generated CLI pages; the union is:

| Format | Use it for |
|---|---|
| `table` | human reading; default for terminals |
| `json` | one full record per call (single-payload commands) |
| `jsonl` | line-delimited JSON, one record per line (streaming-friendly) |
| `ids` | one ID per line — pipe into `xargs`, `fzf`, etc. |
| `csv` | spreadsheet ingest |

For canonical field names per command, see [JSON output schemas](/reference/json-output/). For what's safe to script and which mutations accept piped IDs, see the [automation contract](/guides/automation-contract/).

## IPC buckets

The CLI is a thin wrapper around daemon IPC. Conceptually, every subcommand falls into one of three buckets:

- **`core-mail`** — stable mail/runtime capabilities. Search, read, mutate, sync, send.
- **`mxr-platform`** — accounts, rules, saved searches, subscriptions, semantic runtime.
- **`admin-maintenance`** — status, events, logs, doctor, bug reports, local reset, repair.

Client-specific shaping (TUI panes, web view models) is _not_ a daemon concern. The daemon serves reusable truth; clients shape it for their UI.

This matters when reading the auto-generated pages — most flags fall cleanly within their bucket and don't surprise across them.

## Daemon lifecycle

`mxr` autostarts the daemon. You don't need to manage it yourself unless debugging.

- `mxr daemon` — starts it explicitly (use `--foreground` to see logs)
- `mxr restart` — reaps the running daemon and starts a fresh one against the current binary
- `mxr status` — health check
- `mxr reset --hard` / `mxr burn` — destroy local runtime state (preserves config + credentials by default)

## See also

- [CLI command index](/reference/cli/) — every subcommand, alphabetical
- [Automation contract](/guides/automation-contract/) — `--format`, `--dry-run`, stdin support per command
- [JSON output schemas](/reference/json-output/) — canonical field names
- [HTTP bridge](/reference/bridge/) — same surface over HTTP
