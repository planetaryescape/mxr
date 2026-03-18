# mxr — Thread Export

## Purpose

Export email threads for use outside mxr: sharing with colleagues, feeding to AI, archiving, processing in scripts.

## Export formats

```rust
pub enum ExportFormat {
    Markdown,     // Clean readable thread
    Json,         // Structured, for programmatic use
    Mbox,         // Standard email format (RFC 4155)
    LlmContext,   // Optimized for AI: stripped, minimal tokens
}
```

### Markdown

Clean, human-readable format:

```markdown
# Thread: Deployment rollback plan

## alice@example.com — Mar 15, 2026 14:30

Hey team,

What's the rollback strategy for the v2.3 deployment?
We need a plan by Friday.

## bob@example.com — Mar 15, 2026 15:12

I think we should keep the v2.2 containers warm and use
a blue-green switch. Alice, can you check if the load
balancer config supports it?

## bk@example.com — Mar 17, 2026 09:45

Blue-green is the right call. I've verified the LB config
supports it. Here's what I'm thinking...

---
Exported from mxr | 3 messages | 3 participants
```

### JSON

Structured, machine-readable:

```json
{
  "thread_id": "...",
  "subject": "Deployment rollback plan",
  "participants": ["alice@example.com", "bob@example.com", "bk@example.com"],
  "message_count": 3,
  "messages": [
    {
      "id": "...",
      "from": { "name": "Alice", "email": "alice@example.com" },
      "date": "2026-03-15T14:30:00Z",
      "body_text": "Hey team,\n\nWhat's the rollback strategy..."
    }
  ]
}
```

### Mbox

Standard Unix mailbox format for interoperability with other mail tools.

### LLM Context

This is the interesting one. Optimized for feeding to AI:

- Reader mode applied: signatures, quoted replies, boilerplate all stripped
- Chronological order
- Minimal metadata (just from + date, no full headers)
- Clean text, no HTML artifacts
- Attachment references included as metadata, not binary content
- Token-efficient: every word carries information

```
Thread: Deployment rollback plan
Participants: alice, bob, bk
Messages: 3

---
[alice@example.com, Mar 15 14:30]
What's the rollback strategy for the v2.3 deployment? We need a plan by Friday.

---
[bob@example.com, Mar 15 15:12]
Keep the v2.2 containers warm and use a blue-green switch. Alice, can you check if the load balancer config supports it?

---
[bk@example.com, Mar 17 09:45]
Blue-green is the right call. I've verified the LB config supports it. Here's what I'm thinking:

1. Tag current v2.3 deployment as "blue"
2. Spin up v2.2 containers as "green"
3. Switch LB to green on failure signal
4. Automated health checks every 30s

Attachments: rollback-plan.pdf (45KB)
```

Note how this uses the same reader mode pipeline that powers the TUI display. One pipeline, multiple outputs.

## CLI usage

```bash
# Export as markdown (default)
mxr export THREAD_ID

# Export as LLM context and pipe to AI
mxr export THREAD_ID --format llm | llm "Summarize this thread and extract action items"

# Export as JSON for scripting
mxr export THREAD_ID --format json | jq '.messages[].from.email'

# Export as mbox for import into another client
mxr export THREAD_ID --format mbox > thread.mbox
```

## TUI usage

In thread view, press `e` to export. Shows a format picker:

```
Export thread as:
  m = Markdown
  j = JSON
  l = LLM context (for AI)
  x = Mbox

→ Copied to clipboard / Saved to ~/mxr/exports/
```
