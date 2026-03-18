# mxr — Rules Engine

## Philosophy

Rules are deterministic first. They are data, not scripts. They must be:

- **Inspectable**: "Show me all my rules"
- **Replayable**: "Run this rule over the last 7 days"
- **Idempotent**: Running a rule twice produces the same result
- **Dry-runnable**: "Show me what this rule WOULD do" without actually doing it
- **Auditable**: "Why was this message tagged?" → trace back to the rule that triggered

This matters because once mail automation starts mutating things, users need trust. If a rule archives something it shouldn't have, the user needs to understand why and reverse it.

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

## Dry-run mode

```
mxr rules dry-run RULE_ID                    # What would this rule do to current inbox?
mxr rules dry-run RULE_ID --after 2026-03-10 # What would it do to messages from last week?
mxr rules dry-run --all                      # What would ALL rules do?
```

Dry-run outputs a table of matches:

```
Rule: "Archive newsletters after read"
Matched 47 messages:
  ★ newsletter@sub..  "This Week in Rust #580"     → archive
    newsletter@sub..  "Hacker Newsletter #742"      → archive
    digest@github.com "Your digest for March 14"    → archive
  ...

Would affect 47 messages. Run with --execute to apply.
```

## Rule definition

Rules can be defined in:

1. **Config file** (TOML):

```toml
[[rules]]
name = "Archive read newsletters"
enabled = true
priority = 10

[rules.conditions]
type = "and"
conditions = [
    { type = "field", field = "has_label", label = "newsletters" },
    { type = "field", field = "is_read" },
]

[[rules.actions]]
type = "archive"
```

2. **Command palette**: Interactive rule builder (future)

3. **CLI**:
```
mxr rules add "Archive read newsletters" \
  --when 'label:newsletters AND is:read' \
  --then archive
```

## Shell hooks (the escape hatch)

For anything the declarative rules engine can't do, shell hooks pipe message data to external commands:

```toml
[[rules]]
name = "Process invoices"
enabled = true

[rules.conditions]
type = "field"
field = "subject"
pattern = { type = "contains", value = "invoice" }

[[rules.actions]]
type = "shell_hook"
command = "~/scripts/process-invoice.sh"
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

## Execution phasing

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
