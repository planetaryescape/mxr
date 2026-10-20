# mxr — Rules Engine

## Philosophy

Rules are deterministic first. They are data, not scripts. They must be:

- **Inspectable**: "Show me all my rules"
- **Replayable**: "Run this rule over the last 7 days"
- **Idempotent**: Running a rule twice produces the same result
- **Dry-runnable**: "Show me what this rule WOULD do" without actually doing it
- **Auditable**: "Why was this message tagged?" → trace back to the rule that triggered

This matters because once mail automation starts mutating things, users need trust. If a rule archives something it shouldn't have, the user needs to understand why and reverse it.

Replayability and idempotence above are design goals, not blanket runtime
guarantees. In particular, a shell hook is not an exactly-once interface.

## Rule structure

```rust
pub struct Rule {
    pub id: RuleId,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,         // Lower number = runs first
    pub conditions: Conditions,
    pub actions: Vec<Action>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Conditions

Conditions are composable with AND/OR/NOT:

```rust
pub enum Conditions {
    And(Vec<Conditions>),
    Or(Vec<Conditions>),
    Not(Box<Conditions>),
    Field(FieldCondition),
}

pub enum FieldCondition {
    From { pattern: StringMatch },
    To { pattern: StringMatch },
    Subject { pattern: StringMatch },
    HasLabel { label: String },
    HasAttachment,
    SizeGreaterThan { bytes: u64 },
    SizeLessThan { bytes: u64 },
    DateAfter { date: DateTime<Utc> },
    DateBefore { date: DateTime<Utc> },
    IsUnread,
    IsStarred,
    HasUnsubscribe,
    BodyContains { pattern: StringMatch },  // Requires body to be fetched
}

pub enum StringMatch {
    Exact(String),
    Contains(String),
    Regex(String),
    Glob(String),
}
```

### Actions

Actions are the mutations a rule can perform:

```rust
pub enum RuleAction {
    AddLabel { label: String },
    RemoveLabel { label: String },
    Archive,
    Trash,
    Star,
    MarkRead,
    MarkUnread,
    Snooze { duration: SnoozeDuration },
    /// Run a shell command with the message piped to stdin.
    /// This is the escape hatch for anything the rules engine can't do natively.
    ShellHook { command: String },
}

pub enum SnoozeDuration {
    Hours(u32),
    Days(u32),
    Until(DateTime<Utc>),
}
```

## Rule evaluation

Rules evaluate against incoming messages during sync:

```
New message arrives via sync
  → For each enabled rule (sorted by priority):
    → Evaluate conditions against message
    → If conditions match:
      → Execute actions (or collect actions for dry-run)
      → Log the match in rule execution history
```

Rules are evaluated in priority order. A message can match multiple rules. Actions are accumulated and applied after all rules have been evaluated (not between rules). This prevents rule ordering bugs where rule A's action changes the message in a way that affects rule B's conditions.

Live evaluation and ordinary preview share the same stored message view,
including persisted link/word counts and labels resolved to provider IDs.
They also share the action-dependent missing-data policy: ordinary rules treat
a missing body predicate as false (including true under negation); rules with
`SetTreatment` keep it unknown under `And`/`Or`/`Not` and require a known-true
condition. Empty text is known data. Body matching prefers plain text, falling
back to stored HTML; neither path fetches body data. Independent rule previews
do not simulate preceding actions. Equal-priority ordinary rules retain storage
order; sorting winner tie-breaking remains rule-ID based.

## Dry-run mode

```
mxr rules dry-run RULE_ID                    # Preview a selected rule, even if disabled
mxr rules dry-run RULE_ID --after 2026-03-10 # What would it do to messages from last week?
mxr rules dry-run --all                      # Preview all enabled rules
```

Dry-run outputs a table of matches:

```
Rule: "Archive newsletters after read"
Matched 47 messages:
  ★ newsletter@sub..  "This Week in Rust #580"     → archive
    newsletter@sub..  "Hacker Newsletter #742"      → archive
    digest@github.com "Your digest for March 14"    → archive
  ...

Would affect 47 messages.
```

The table above illustrates the match summary, not literal CLI output. Ordinary
preview scans at most the newest 10,000 stored envelopes per account and then
applies the inclusive `--after` midnight-UTC cutoff. `--all` selects rules, not
an exhaustive mailbox scan. Preview respects rule account scope and never runs
actions or shell hooks. There is no ordinary dry-run `--execute` flag.

## Rule definition

Rules can be defined in:

1. **Config file design sketch** (TOML; rule import is not implemented):

```toml
[[rules]]
name = "Archive read newsletters"
enabled = true
priority = 10

[rules.conditions]
type = "and"
conditions = [
    { type = "field", field = "has_label", label = "newsletters" },
    { type = "not", condition = { type = "field", field = "is_unread" } },
]

[[rules.actions]]
type = "archive"
```

2. **Rules page**: Interactive rule editor

3. **CLI**:
```
mxr rules add "Archive read newsletters" \
  --when 'label:newsletters AND is:read' \
  --then archive
```

## Shell hooks (the escape hatch)

For anything the declarative rules engine can't do, an explicitly configured
shell action pipes private stored mail to a trusted local command. Configure it
through the Rules editor or CLI, not a config-file rule import:

```bash
mxr rules edit RULE_ID --then 'shell:/absolute/path/to/process-message' --disable
mxr rules --format json dry-run RULE_ID
mxr rules enable RULE_ID
```

The shell hook receives the message as JSON on stdin:

```json
{
  "id": "...",
  "from": { "name": "Alice", "email": "alice@example.com" },
  "subject": "Invoice #2847",
  "date": "2026-03-17T10:30:00Z",
  "body_text": "Please find attached...",
  "attachments": [
    { "filename": "invoice.pdf", "size_bytes": 234567, "local_path": "/tmp/mxr/..." }
  ]
}
```

The script can do whatever it wants: save to a folder, update a spreadsheet, ping a webhook, etc. mxr doesn't care about the script's output.

`id`, sender name/email, subject and RFC 3339 date come from the stored envelope.
`body_text` contains only stored plain text: missing and HTML-only bodies yield
`null`, while an empty plain-text body yields `""`. Attachments come from stored
body metadata, without remote downloads. `local_path` is `null` unless the
cached path can be opened as a regular file at payload construction time; a
command must still handle files disappearing afterward.

Mail fields are JSON stdin, never part of the configured `sh -c` command.
Commands run with the user's privileges and can disclose that private mail;
enabling a shell rule is an explicit trust decision. Preview never starts it.
`[general].hook_timeout` (30 seconds by default) accounts for serialization,
stdin delivery and waiting. Timeout kills and reaps the direct child, with up
to one additional second for cleanup; cancellation kills the child on drop.
This does not supervise the command's descendants or detached processes. Nonzero exit
and timeout are failures in execution history; stdout is ignored.

## Execution phasing

This original roadmap is historical; use the rules guide for current workflows.

### v0.1: No rules engine
Focus on core: sync, search, compose, TUI.

### v0.2: Declarative rules
- Conditions + Actions as data
- Config file definition
- Dry-run
- Rule evaluation on sync

### v0.3: Shell hooks
- ShellHook action type
- Message JSON piped to stdin
- Exit code handling (0 = success, non-zero = log error)

### Future: Scripting runtime
- Embed Lua (mlua) or Rhai for in-process scripting
- Richer event hooks (on_receive, on_send, on_label_change)
- But NOT before declarative rules are proven and trusted

## Account-scoped sorting is implemented

`SetTreatment` assigns Messages, Updates or Reading without creating a manual
correction or invoking provider actions. Optional `Rule.account_id` scopes every
action; legacy rules without it retain their all-account scope. Sorting rules
require one account and header predicates. Personal corrections retain their
existing chronological precedence.

The daemon owns bounded draft preview and token application. A preview scans
the newest 200 messages, reports whether older mail was excluded and resolves
the same classification and conversation shape as live modes and arrivals.
Apply saves the rule, selected automatic treatment provenance and arrival
counts together. Rule edits affect future arrivals; historical provenance
retains source ID, name and version independently of rule deletion. Sync
replay of previously placed mail preserves its treatment.

See [the rules guide](../../site/src/content/docs/guides/rules.md) for client
commands, preview limits and downgrade requirements.
