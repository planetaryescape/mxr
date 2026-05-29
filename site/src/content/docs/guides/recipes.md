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

:::tip[Add account scope when needed]
Most mail-facing commands accept `--account <selector>`, where selector
is an account key, email address, account id, or unambiguous display
name. Keep the same selector on search, dry-run, and apply:

```bash
mxr archive --account work --search 'from:no-reply older_than:30d' --dry-run
mxr archive --account work --search 'from:no-reply older_than:30d' --yes
```
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

### Owed-reply backlog ranked by how overdue

```bash
mxr owed --format json \
  | jq -r 'sort_by(-.overdue_score) | .[]
           | "\(.overdue_score | tostring | .[0:4])\t\(.waiting_days|round)d\t\(.counterparty_email)\t\(.subject)"' \
  | head -20
```

What you get: top 20 threads where *you* are the bottleneck, ranked by
`waiting_days / expected_days` (using the recipient's typical cadence;
default 7 days when no history). Same set as `mxr search
'is:owed-reply'` — pick whichever surface fits your script.

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

## With `--check` — pre-send safety gate

Every recipe here uses the [pre-send safety](/guides/pre-send-safety/)
pipeline. The `--check` flag runs every safety check WITHOUT sending,
exits 2 on any Blocker, and prints a JSON report you can parse.

### Block sends with leaked secrets (pre-commit hook)

`.git/hooks/pre-commit` for a repo where you stash outgoing-mail
templates:

```bash
#!/usr/bin/env bash
set -euo pipefail
for draft in mail/*.md; do
  to=$(yq '.to' "$draft")
  body=$(awk '/^---$/{n++;next} n==2' "$draft")
  printf '%s' "$body" | mxr compose --to "$to" --body-stdin --check --format json | \
    jq -e '
      ([.issues[] | select(.severity == "blocker") | .code]) as $blockers
      | if $blockers | length == 0 then true
        else error("blocked: \($blockers | join(\", \")) in \(input_filename)")
        end' || exit 1
done
```

What you get: any commit that introduces a draft with a PEM private
key, AWS/OpenAI/GitHub token, or other blocker-grade secret fails the
hook before push.

### Send only if safety is clean, otherwise mint and pause

```bash
report=$(mxr send "$DRAFT_ID" --check --format json)
verdict=$(echo "$report" | jq -r '.verdict')
case "$verdict" in
  safe|warn) mxr send "$DRAFT_ID" ;;
  blocked)
    token=$(echo "$report" | jq -r '.issues[] | select(.severity == "blocker") | .override_token | select(. != null)' | head -1)
    echo "BLOCKED. To override: mxr send $DRAFT_ID --override-safety $token"
    exit 2
    ;;
esac
```

### Audit every scheduled send before it fires

```bash
mxr drafts --format json | jq -r '.[] | select(.send_at != null) | .id' \
  | while read draft_id; do
      verdict=$(mxr send "$draft_id" --check --format json | jq -r '.verdict')
      printf '%s\t%s\n' "$draft_id" "$verdict"
    done
```

What you get: one line per scheduled draft with its current verdict.
Catch drafts that would silently fail when the scheduler fires them
(the scheduler clears the schedule on Blocker, so an unaddressed
warning isn't enough — only Blockers stop a scheduled send).

### Owed-reply digest emailed every morning

`crontab -e`:

```text
0 8 * * 1-5 /usr/local/bin/mxr owed --format json | jq -r 'sort_by(-.overdue_score) | .[0:10] | .[] | "  • \(.counterparty_email): \(.waiting_days|round)d — \(.subject)"' | { echo "Threads you owe (top 10):"; cat; } | mail -s "mxr: owed replies" you@example.com
```

```text
Tell an agent
"Walk my `mxr search 'is:owed-reply'` set. For each thread, show me a
two-line summary, then ask if I want to reply (`r`), snooze a week
(`s`), or skip. Use `mxr summarize` for context. Never send without
showing me the body."
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
mxr archive --search 'from:noreply older_than:30d' --dry-run
mxr archive --search 'from:noreply older_than:30d' --yes
mxr label <name> ID --dry-run
mxr snooze ID --until '...' --dry-run
mxr send DRAFT_ID --dry-run            # preview send
mxr screener allow|deny|feed|paper-trail <addr>
```

The agent doesn't need a special API. The same flags humans use are the same ones it composes.

## With AI features — synthesis with citations

### "What did we decide about X last quarter?"

Situation: you need a grounded answer plus the messages that prove it.

```bash
mxr ask "what did Alice and I decide about pricing in Q2?" \
  --from alice@example.com \
  --after 2026-04-01 --before 2026-06-30 \
  --format json \
  | tee /tmp/answer.json \
  | jq -r '.text, "\nCitations:", (.citations[] | "- \(.message_id)\t\(.subject)")'
```

What you get: the synthesized answer to stdout, JSON to `/tmp/answer.json`. `jq` prints the answer text followed by citation rows you can pipe back into `mxr cat`. If retrieval can't support an answer the text is literally "not enough evidence" — no synthesized confidence.

### Resolve overdue commitments before standup

Situation: it's Friday and you want to know what you promised to send this week.

```bash
mxr commitments --status open --format json \
  | jq -r '.[] | select(.direction == "yours" and .by_when != null)
           | "\(.by_when)\t\(.contact_email)\t\(.what)\t\(.evidence_msg_id)"' \
  | sort \
  | column -t -s $'\t'
```

What you get: a sortable table of every open promise with the source message id — pipe `--format ids` and `xargs -I{} mxr cat {} --view reader` if you want to read the original draft text.

### Find an expert before forwarding

Situation: inbound question you'd otherwise forward — find who's answered something similar.

```bash
mxr expert MESSAGE_ID --format json \
  | jq -r '.[0:3]
           | .[]
           | "\(.score | tostring | .[0:4])\t\(.email)\t\(.reason)"'
```

What you get: top 3 candidate experts with score and the cited reason. Their `citations[]` point at the *answer* messages, not at the matching questions — verify before forwarding.

### Re-enter a dormant thread

Situation: you're about to reply on a 3-month-old thread.

```bash
mxr briefing thread THREAD_ID --format json \
  | jq -r '.body_markdown,
           "\nCitations:",
           (.citations[]? | "  - \(.message_id // .thread_id): \(.quote)")'
```

What you get: a Markdown recap plus citations when the model grounded claims in
specific messages. Cached, so the second run is instant.

### Pick a send slot that matches the recipient

Situation: scheduling a sensitive ask, want to land it in their fastest reply bucket.

```bash
mxr send-time alice@example.com --at "$PROPOSED_AT" --format json \
  | jq -r 'if .confidence == "low" then "low confidence — no recommendation"
           else "best window: \(.recipient_rows[0].best_windows[0] | "\(.weekday) \(.hour_start):00-\(.hour_end):00")"
           end'
```

What you get: a one-liner with the best window if mxr has enough data, or an honest "no recommendation" line when sample count is low. Pair with `mxr send DRAFT_ID --at "$WHEN"` to actually schedule.

```text
Tell an agent
"Before scheduling DRAFT_ID with `mxr send DRAFT_ID --at <when>`, run
`mxr send-time <to_address> --at <when> --format json`. If the proposed
slot is at least 2x slower than the best window AND confidence is medium
or high, propose the better slot. Don't auto-reschedule."
```

## See also

- [CLI reference](/reference/cli/) — every command and flag.
- [Automation contract](/guides/automation-contract/) — which commands support `--format json`, `--dry-run`, stdin IDs.
- [JSON output schemas](/reference/json-output/) — canonical field names for piping into `jq`.
- [HTTP bridge](/reference/bridge/) — same surface over HTTP for web, mobile, and agent clients.
- [API explorer](/reference/api-explorer/) — interactive Scalar reference; try requests against your local daemon.
- [For agents](/guides/for-agents/) — boundaries and safe defaults when an LLM is driving.
- [AI agent skill](/guides/agent-skill/) — install the mxr skill into Claude / Cursor / Continue.
- [Forgotten work](/guides/forgotten-work/) — commitments and owed-reply lens behind the recipes above.
- [Archive intelligence](/guides/archive-intelligence/) — citation-validated `mxr ask` and the decision log.
- [Briefings and loop-in](/guides/briefings-and-loop-in/) — dormant-thread briefings, expert finder, suggest-recipients, whois.
- [Timing and cadence](/guides/timing-and-cadence/) — send-time optimizer and cadence watchlist.
