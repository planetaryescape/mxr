---
title: Recipes
description: Real pipelines that compose mxr with fzf, jq, xargs, watch, cron, $EDITOR, and your AI agent.
---

mxr is a Unix citizen. Most list/search commands support `--format json|jsonl|ids|csv|table`; the core mail mutations support `--dry-run` and accept either explicit IDs, `--search QUERY`, or piped IDs on stdin. The exact per-command capabilities live in the [automation contract](/guides/automation-contract/); the JSON shapes are documented in [JSON output schemas](/reference/json-output/). This page is the cookbook.

Each recipe shows three things:

1. **Situation** — the actual problem you're solving.
2. **Pipeline** — copy-pasteable, no shell prompt prefix.
3. **What you get** — the shape of the output.

If you'd rather hand the situation to an LLM, every section ends with a `Tell an agent` block — a natural-language prompt that maps cleanly onto the same pipeline.

:::tip[Two flags do most of the work]
`--format ids` (one ID per line) and `--format json` (structured records) are the building blocks. Pipe `ids` into mxr mutations, `fzf`, or `while read`; pipe `json` into `jq`.
:::

:::note[Two equivalent forms]
For mxr-on-mxr chaining, every read command that takes a single ID (`cat`, `thread`, `headers`, `summarize`, `draft-assist`, `open`, `attachments list`) **also** accepts `--search QUERY` directly, with `--first` (most recent only) or `--limit N` modifiers. That's daemon-native, snapshot-consistent, and one fewer process.

```bash
# Pipeline form (works with any partner tool: jq, fzf, xargs, parallel, ...)
mxr search 'from:alice' --format ids | xargs -I{} mxr cat {} --view reader

# --search form (mxr-on-mxr; resolves once inside the daemon)
mxr cat --search 'from:alice' --first
mxr summarize --search 'from:alice newer_than:7d' --limit 5
```

The recipes below use whichever form reads better. When the partner tool is mxr itself, prefer `--search`. When it's anything else, the pipeline form is canonical.
:::

## With `fzf` — interactive picker

### Pick a thread to read

Situation: too many unread messages to scroll, but you know it's "from someone at acme".

```bash
mxr search 'is:unread from:acme' --format jsonl \
  | jq -r '"\(.message_id)\t\(.from)\t\(.subject)"' \
  | fzf --delimiter='\t' --with-nth=2,3 \
  | cut -f1 \
  | xargs -I{} mxr cat {} --view reader
```

What you get: an interactive list keyed by sender + subject; pressing Enter prints the chosen message body (rendered with reader mode) to your terminal. Use `--view raw` for the unrendered body, `--view html` to dump the original HTML.

### Pick a draft to resume

```bash
mxr drafts --format jsonl recover \
  | jq -r '"\(.draft_id // .id)\t\(.subject // "(no subject)")"' \
  | fzf --delimiter='\t' --with-nth=2 \
  | cut -f1 \
  | xargs mxr drafts resume
```

### Browse senders by volume

```bash
mxr storage --by sender --format jsonl \
  | jq -r '"\(.bytes)\t\(.key)"' \
  | sort -rn \
  | fzf --header='bytes | sender' \
  | awk '{print $2}' \
  | xargs mxr sender
```

What you get: pick the heaviest senders interactively; Enter drills into their full profile (volume, cadence, open commitments).

```text
Tell an agent
"Help me find the sender I email most often that I haven't replied to in 30 days. List the candidates first; I'll pick one."
```

## With `jq` — filter and reshape JSON

### Daily digest of unread

```bash
mxr search 'is:unread newer_than:1d' --format json \
  | jq -r 'group_by(.from)
           | map({sender: .[0].from, count: length, latest: max_by(.date).subject})
           | sort_by(-.count)
           | .[]
           | "\(.count)\t\(.sender)\t\(.latest)"'
```

What you get: a per-sender digest, descending by message count, with the most recent subject for context.

### Find threads that need follow-up (no reply in 7 days)

```bash
mxr stale --theirs --older-than-days 7 --format json \
  | jq -r '.[] | "\(.thread_id)\t\(.latest_subject)\t\(.latest_date)"'
```

`--theirs` means *they* owe me a reply (latest message is outbound).
`--older-than-days 7` excludes threads with activity in the last week.

### Extract all attachment filenames from a query

```bash
mxr search 'has:attachment from:billing' --format ids \
  | while IFS= read -r id; do
      mxr attachments list "$id"
    done
```

```text
Tell an agent
"Summarize my unread mail from the last 24 hours grouped by sender. Use `mxr search 'is:unread newer_than:1d' --format json` and group with jq."
```

## With `xargs` — bulk operations

### Archive everything matching a search

```bash
mxr search 'from:no-reply@*.example.com older_than:30d' --format ids \
  | mxr archive --yes
```

`--yes` skips the confirmation prompt; needed when stdin is not a terminal.

### Trash a sender's entire backlog

```bash
mxr search 'from:spam@example.com' --format ids \
  | mxr trash --yes
```

### Apply a label to a query, in parallel

```bash
mxr search 'from:billing@*.example.com' --format ids \
  | mxr label billing --yes
```

For mxr-on-mxr bulk actions, prefer piping IDs directly into the mutation. Use GNU `parallel` only when you fan out non-mxr work.

### Dry-run before committing

Always preview when piping into mutations:

```bash
mxr search 'from:no-reply' --format ids \
  | mxr archive --dry-run
```

The core mail mutations (`archive`, `trash`, `spam`, `snooze`, `label`, etc.) support `--dry-run` and print what would happen without touching the provider.

```text
Tell an agent
"Archive every unread newsletter older than 30 days. Show me the dry-run first; I'll approve."
```

## With `watch` — live dashboards

### Live unread count

```bash
watch -n 30 'mxr count is:unread'
```

### Sender activity heatmap

```bash
watch -n 60 'mxr search "newer_than:5m" --format json \
  | jq -r ".[] | .from" | sort | uniq -c | sort -rn | head'
```

What you get: a refreshing top-10 of senders pinging you in the last 5 minutes — useful during incidents.

### Sync health monitor

```bash
watch -n 5 'mxr sync --status --format table'
```

```text
Tell an agent
"Tell me when @ceo emails me. Poll every 30 seconds with `mxr search 'from:ceo@example.com is:unread' --format ids` and ping me on stdout when the result is non-empty."
```

## With `cron` / `systemd` — scheduled work

### Morning digest at 09:00

`crontab -e`:

```text
0 9 * * 1-5 /usr/local/bin/mxr search 'is:unread newer_than:1d' \
  --format json | mail -s "Morning digest" you@example.com
```

### Auto-snooze low-priority newsletters until weekend

```text
0 17 * * 5 /usr/local/bin/mxr search 'label:newsletters is:unread' \
  --format ids | /usr/local/bin/mxr snooze --until 'monday 9am' --yes
```

### systemd user timer

`~/.config/systemd/user/mxr-cleanup.service`:

```ini
[Service]
Type=oneshot
ExecStart=/bin/sh -c 'mxr search "from:no-reply older_than:90d" --format ids | mxr archive --yes'
```

`~/.config/systemd/user/mxr-cleanup.timer`:

```ini
[Timer]
OnCalendar=Sun *-*-* 02:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

```bash
systemctl --user enable --now mxr-cleanup.timer
```

```text
Tell an agent
"Set up a weekly cron that archives no-reply mail older than 90 days. Show me the cron line; don't install it."
```

## With `$EDITOR` — compose loops

### Reply to a search result interactively

```bash
mxr search 'from:alice' --format ids \
  | xargs -I{} mxr cat {} --view reader \
  | $EDITOR -                       # paste content into editor as scratch
```

### Open a draft directly in your editor (no daemon)

`mxr compose` already opens `$EDITOR` with the markdown + frontmatter shell. To prepare a one-off reply from a script:

```bash
mxr reply MESSAGE_ID --body-stdin <<'EOF'
Hey — quick yes from me. Will follow up tomorrow with the deck.
EOF
```

### Walk the reply queue

```bash
mxr replies --format ids | while read id; do
  mxr cat "$id" --view reader
  echo "Reply? [y/N/q]"
  read answer < /dev/tty
  case "$answer" in
    y) mxr reply "$id" ;;
    q) break ;;
  esac
done
```

## With `grep` / `ripgrep` — content search across exports

```bash
# Export everything from a sender, search the corpus locally
mxr search 'from:legal@example.com' --format ids \
  | while IFS= read -r id; do mxr export "$id" --format markdown; done \
  | rg -i 'NDA|confidential|terms'
```

```bash
# Fast full-text grep over message bodies via Tantivy
mxr search 'NDA OR "non-disclosure"' --format jsonl \
  | jq -r '.subject + " — " + .from'
```

The second form is ~100× faster because it stays inside the search index. Use the first only when you need regex or ripgrep features the search grammar doesn't support.

## With `parallel` — fan-out work

### Fetch bodies for many threads concurrently

```bash
mxr search 'from:billing' --format ids \
  | parallel -j8 mxr cat {} --view reader \
  | rg -i 'amount due|invoice' \
  | head
```

### Per-account sync in parallel

```bash
mxr accounts --format ids \
  | parallel -j4 mxr sync --account {}
```

## Talking to your agent

mxr is designed so agents can run it directly. Three rules keep the interaction safe:

1. **Read-only first**. Have the agent run `mxr search`, `mxr cat`, `mxr stale`, `mxr storage --by sender`, `mxr sender`, `mxr summarize` before any mutation.
2. **Always `--dry-run` before bulk mutations.** Every mutation supports it; the agent should preview the affected IDs and report them back to you.
3. **Use `--format json` or `--format jsonl`** when piping into the agent's reasoning loop, never `table` (that's for humans).

### Prompt patterns that work

- *"Show me the 10 senders I owe replies to. Use `mxr stale` and `mxr sender` to verify cadence."*
- *"Find every newsletter from this month, group by sender, and propose a label rule for the noisiest three."*
- *"Draft a polite decline to the latest message from acme.com, but show me the draft before sending."*
- *"Summarize unread mail from the last 24h grouped by importance. Use `mxr summarize` only on threads with 4+ messages."*

### Read-only fast paths agents should know

```bash
mxr search '<query>' --format json     # full search
mxr cat MESSAGE_ID --view reader         # rendered, distraction-free
mxr summarize THREAD_ID                # LLM Markdown summary + next steps
mxr sender alice@example.com           # per-sender aggregates
mxr stale --mine --older-than-days 7 --format json
mxr count <query>                      # count without payload
mxr drafts --format json               # what's waiting
mxr doctor --format json               # daemon health
```

### Mutating fast paths to gate behind review

```bash
mxr archive ID --dry-run               # preview
mxr archive --search '<query>' --dry-run --yes
mxr label <name> ID --dry-run
mxr snooze ID --until '...' --dry-run
mxr send DRAFT_ID --dry-run            # preview send
mxr screener allow|deny|feed|paper-trail <addr>
```

The agent doesn't need a special API. The same flags humans use are the same ones it composes.

## See also

- [CLI reference](/reference/cli/) — every command and flag.
- [Automation contract](/guides/automation-contract/) — which commands support `--format json`, `--dry-run`, stdin IDs.
- [JSON output schemas](/reference/json-output/) — canonical field names for piping into `jq`.
- [HTTP bridge](/reference/bridge/) — same surface over HTTP for desktop / web / mobile clients.
- [API explorer](/api/bridge/) — interactive Scalar reference; try requests against your local daemon.
- [For agents](/guides/for-agents/) — boundaries and safe defaults when an LLM is driving.
- [AI agent skill](/guides/agent-skill/) — install the mxr skill into Claude / Cursor / Continue.
