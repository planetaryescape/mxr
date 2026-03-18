# mxr — TUI (Terminal User Interface)

## Framework

Ratatui + crossterm. This is the standard for Rust TUI apps and gives us full control over rendering, layout, and input handling.

## Layout

The TUI has a multi-pane layout:

```
┌─ sidebar ─────┬─ message list ────────────────────────────┐
│               │                                           │
│ Inbox (12)    │ ★ alice@ex.com  Deployment plan    Mar 17 │
│ Starred       │   bob@work.com  Q1 Report          Mar 16 │
│ Sent          │   newsletter@.. This week in Ru..  Mar 16 │
│ Drafts (2)    │ → carol@ex.com  Invoice #2847      Mar 15 │
│ Trash         │   dave@ex.com   Meeting notes      Mar 14 │
│               │                                           │
│ ── Labels ──  │                                           │
│ work          │                                           │
│ personal      │                                           │
│ newsletters   │                                           │
│               │                                           │
│ ── Saved ──   │                                           │
│ Unread inv..  │                                           │
│ From team     │                                           │
│               │                                           │
├───────────────┴───────────────────────────────────────────┤
│ [INBOX] 12 unread | synced 2m ago | reader mode           │
└───────────────────────────────────────────────────────────┘
```

When a message is selected, the right pane shows the message body:

```
┌─ sidebar ─────┬─ message list ──────┬─ message view ──────┐
│               │                     │ From: alice@ex.com   │
│ Inbox (12)    │ ★ alice  Deploy..   │ To: team@work.com    │
│ Starred       │   bob    Q1 Rep..   │ Date: Mar 17, 2026   │
│ ...           │   news.. This w..   │ Subject: Deployment  │
│               │ → carol  Invoi..    │                      │
│               │   dave   Meeti..    │ Hey team,            │
│               │                     │                      │
│               │                     │ Here's the rollback  │
│               │                     │ strategy for v2.3... │
│               │                     │                      │
│               │                     │ [1] plan.pdf (2.3MB) │
│               │                     │                      │
│               │                     │ reader: 142 → 28     │
└───────────────┴─────────────────────┴──────────────────────┘
```

### Layout modes

- **Two-pane**: Sidebar + message list (default when no message selected)
- **Three-pane**: Sidebar + message list + message view (when a message is selected)
- **Full-screen**: Message view takes full width (toggle with `f`)
- **Thread view**: Full conversation, messages stacked vertically

## Vim motions and keybindings

### What we need

We need vim NAVIGATION in the TUI, not vim EDITING. Editing happens in $EDITOR. The TUI just needs list navigation, pane switching, and action dispatch.

### We wire these ourselves

There is no "vim navigation for ratatui" drop-in crate. But the surface area is small — maybe 20 keybindings for navigation. It's a match statement in the event loop. The design work is in building a configurable keymap layer, not in the quantity of bindings.

### Default keybindings

#### Navigation
```
j / ↓           Move down in list
k / ↑           Move up in list
gg              Jump to top of list
G               Jump to bottom of list
Ctrl-d          Half-page down
Ctrl-u          Half-page up
H               Jump to top of visible area
M               Jump to middle of visible area
L               Jump to bottom of visible area
zz              Center current item in view
Enter           Open selected message / expand thread
Escape / q      Back / close pane / quit context
Tab             Switch focus between panes
```

#### Actions
```
c               Compose new message
r               Reply
R (in msg view) Toggle reader mode / raw view
f               Forward
a               Archive
d               Trash (move to trash)
s               Star / unstar
u               Mark as unread
U               Unsubscribe (one-key, with confirmation)
Z               Snooze menu (Z t = tomorrow, Z n = next week, etc.)
o               Open in browser (HTML body or attachment)
e               Export thread
/               Search (focus search input)
n               Next search result
N               Previous search result
Ctrl-p          Command palette
:               Command mode (ex-style, if implemented)
?               Help / keybinding reference
```

#### Attachment handling
```
a (in msg view) Show attachment list
1-9             Select attachment by number
Enter           Download selected attachment
o               Open downloaded attachment with xdg-open
```

### Multi-key sequences

The one genuinely tricky part is multi-key sequences like `gg`. This requires a small state machine:

```rust
enum KeyState {
    Normal,
    WaitingForSecond { first: char, deadline: Instant },
}

// In the event loop:
match (&self.key_state, key.code) {
    (KeyState::Normal, KeyCode::Char('g')) => {
        self.key_state = KeyState::WaitingForSecond {
            first: 'g',
            deadline: Instant::now() + Duration::from_millis(500),
        };
    }
    (KeyState::WaitingForSecond { first: 'g', .. }, KeyCode::Char('g')) => {
        self.jump_to_top();
        self.key_state = KeyState::Normal;
    }
    (KeyState::WaitingForSecond { .. }, other) => {
        // Timeout or different key: treat as two separate inputs
        self.key_state = KeyState::Normal;
        self.handle_key(other);
    }
    // ...
}
```

This is maybe 50 lines of code but it needs to be right. The 500ms timeout is standard for vim-like multi-key sequences.

### Configurable keybindings

Keybindings are configurable via TOML:

```toml
# ~/.config/mxr/keys.toml
[mail_list]
j = "move_down"
k = "move_up"
gg = "jump_top"
G = "jump_bottom"
"/" = "search"
Enter = "open"
a = "archive"
d = "trash"
s = "star"
r = "reply"
c = "compose"
"Ctrl-p" = "command_palette"
U = "unsubscribe"
Z = "snooze"

[thread_view]
j = "next_message"
k = "prev_message"
r = "reply"
R = "toggle_reader_mode"
f = "forward"
o = "open_in_browser"
e = "export_thread"

[message_view]
j = "scroll_down"
k = "scroll_up"
R = "toggle_reader_mode"
o = "open_in_browser"
a = "attachment_list"
```

### Action dispatch system

This is the key architectural piece. Keybindings and the command palette both dispatch through the same action system:

```rust
pub enum Action {
    // Navigation
    MoveDown(usize),
    MoveUp(usize),
    JumpTop,
    JumpBottom,
    PageDown,
    PageUp,
    OpenSelected,
    Back,
    SwitchPane,

    // Mail actions
    Compose,
    Reply,
    ReplyAll,
    Forward,
    Archive,
    Trash,
    Star,
    MarkUnread,
    Unsubscribe,
    Snooze(SnoozeOption),

    // Search
    OpenSearch,
    NextResult,
    PrevResult,
    ExecuteSavedSearch(SavedSearchId),

    // View
    ToggleReaderMode,
    OpenInBrowser,
    FullScreen,

    // Export
    ExportThread(ExportFormat),

    // System
    CommandPalette,
    SyncNow,
    Quit,
}
```

Keybindings map to Actions. The command palette maps to Actions. CLI subcommands map to Actions. One dispatch system for everything.

## Command palette (Ctrl-P)

The command palette is mxr's discoverability layer, extensibility layer, automation entry point, and keyboard UX multiplier. It's inspired by VS Code's Ctrl-P and Superhuman's Ctrl-K.

### How it works

1. User presses Ctrl-P
2. A fuzzy-searchable overlay appears at the top of the screen
3. User types to filter commands
4. Commands are ranked by fuzzy match score (nucleo crate)
5. User selects with Enter or arrow keys
6. Command is dispatched as an Action

### What appears in the palette

```
> _                                              [Ctrl-P to dismiss]

  Compose new message                             c
  Reply to selected                               r
  Archive                                         a
  Trash                                           d
  Star                                            s
  Mark unread                                     u
  Unsubscribe                                     U
  Snooze...                                       Z
  ──────────────────────────────────────────────
  Search...                                       /
  Saved: Unread invoices
  Saved: From team
  Saved: Newsletters this week
  ──────────────────────────────────────────────
  Go to Inbox
  Go to Starred
  Go to Sent
  Go to Drafts
  ──────────────────────────────────────────────
  Sync now
  Export thread...
  Toggle reader mode                              R
  Open in browser                                 o
  Settings
  Help                                            ?
```

### Command registry

```rust
pub struct CommandPalette {
    matcher: nucleo::Matcher,
    commands: Vec<PaletteCommand>,
}

pub struct PaletteCommand {
    pub id: &'static str,
    pub label: String,
    pub shortcut: Option<String>,  // Display text for the shortcut
    pub action: Action,
    pub context: Vec<PaletteContext>,  // When is this command available?
    pub category: PaletteCategory,
}

pub enum PaletteContext {
    Always,
    InMailList,
    InMessageView,
    InThreadView,
    MessageSelected,
    HasUnsubscribe,
}

pub enum PaletteCategory {
    Actions,
    Navigation,
    SavedSearches,
    System,
}
```

### Fuzzy matching

Uses `nucleo` (extracted from the Helix editor). Faster than skim/fzf matching. Matches on the command label, so typing "unsub" finds "Unsubscribe", typing "inv" finds "Saved: Unread invoices", etc.

## Status bar

The bottom bar shows:
- Current context: `[INBOX]`, `[label:work]`, `[search: "invoice"]`
- Unread count
- Sync status: `synced 2m ago` or `syncing...` or `sync error: auth expired`
- Reader mode indicator
- Snooze count (if any snoozed messages exist)

## TUI ↔ Daemon communication

The TUI is a client. It connects to the daemon via Unix socket and speaks the JSON protocol defined in the protocol crate.

### Event model

The TUI maintains a local view state and updates it when:
1. User input triggers an action → send command to daemon → update on response
2. Daemon pushes events → TUI receives and updates (e.g., new message synced, snooze woke)

For push events, the daemon sends notifications over the socket:

```rust
pub enum DaemonEvent {
    SyncCompleted { account_id: AccountId, messages_synced: u32 },
    SyncError { account_id: AccountId, error: String },
    NewMessage { envelope: Envelope },
    MessageUnsnoozed { message_id: MessageId },
    LabelCountsUpdated { labels: Vec<(LabelId, u32, u32)> },
}
```

The TUI listens for these events in its event loop alongside user input (crossterm events). This is a standard tokio `select!` pattern.
