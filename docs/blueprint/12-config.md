# mxr — Configuration

## Config file location

Following XDG Base Directory spec:

- **Config**: `$XDG_CONFIG_HOME/mxr/config.toml` (default: `~/.config/mxr/config.toml`)
- **Data**: `$XDG_DATA_HOME/mxr/` (default: `~/.local/share/mxr/`)
  - `mxr.db` — SQLite database
  - `search_index/` — Tantivy index
  - `attachments/` — Downloaded attachments
- **Runtime**: `$XDG_RUNTIME_DIR/mxr/mxr.sock` — Daemon socket

macOS equivalents:
- Config: `~/Library/Application Support/mxr/config.toml`
- Data: `~/Library/Application Support/mxr/`

## Config file structure

```toml
# ~/.config/mxr/config.toml

# ===========================================================================
# General
# ===========================================================================
[general]
# Editor for composing. Falls back to $EDITOR, $VISUAL, then "vi"
editor = "nvim"

# Default account for compose (if multiple accounts configured)
default_account = "personal"

# Sync interval in seconds (0 = manual only)
sync_interval = 60

# Download directory for attachments
attachment_dir = "~/mxr/attachments"

# ===========================================================================
# Accounts
# ===========================================================================

# --- Gmail account (sync + send) ---
[accounts.personal]
name = "Personal Gmail"
email = "bk@example.com"

[accounts.personal.sync]
provider = "gmail"
client_id = "xxxx.apps.googleusercontent.com"
# Tokens stored in system keyring, not here.
# Token ref is auto-generated during `mxr accounts add gmail`
token_ref = "mxr/personal-gmail"

[accounts.personal.send]
provider = "gmail"
# Uses same auth as sync

# --- Gmail sync + SMTP send ---
[accounts.work]
name = "Work"
email = "bk@company.com"

[accounts.work.sync]
provider = "gmail"
client_id = "yyyy.apps.googleusercontent.com"
token_ref = "mxr/work-gmail"

[accounts.work.send]
provider = "smtp"
host = "smtp.company.com"
port = 587
username = "bk@company.com"
password_ref = "mxr/work-smtp"  # Stored in system keyring
use_tls = true

# ===========================================================================
# Rendering
# ===========================================================================
[render]
# HTML to text conversion. Leave blank for built-in html2text.
# html_command = "w3m -T text/html -dump"

# Reader mode on by default
reader_mode = true

# Show stripped line count in status bar
show_reader_stats = true

# ===========================================================================
# Search
# ===========================================================================
[search]
# Default sort for search results
default_sort = "date_desc"  # date_desc | date_asc | relevance

# Maximum results to return
max_results = 200

# ===========================================================================
# Snooze defaults
# ===========================================================================
[snooze]
morning_hour = 9      # "tomorrow morning" = 9:00 AM
evening_hour = 18     # "this evening" = 6:00 PM
weekend_day = "saturday"
weekend_hour = 10

# ===========================================================================
# Appearance
# ===========================================================================
[appearance]
# Theme: "default" | "minimal" | path to custom theme TOML
theme = "default"

# Show sidebar
sidebar = true

# Date format in message list
date_format = "%b %d"         # "Mar 17"
date_format_full = "%Y-%m-%d %H:%M"  # "2026-03-17 09:45"

# Truncate subject in message list
subject_max_width = 60

# ===========================================================================
# Rules (see 10-rules-engine.md for full syntax)
# ===========================================================================
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

## Keybinding config

Separate file to keep things clean:

```toml
# ~/.config/mxr/keys.toml

# See 08-tui.md for full keybinding documentation.
# Only override what you want to change. Defaults are sensible.

[mail_list]
j = "move_down"
k = "move_up"
gg = "jump_top"
G = "jump_bottom"
"Ctrl-d" = "page_down"
"Ctrl-u" = "page_up"
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

[message_view]
j = "scroll_down"
k = "scroll_up"
R = "toggle_reader_mode"
o = "open_in_browser"

[thread_view]
j = "next_message"
k = "prev_message"
e = "export_thread"
```

## Credential storage

Credentials (OAuth2 tokens, SMTP passwords) are NEVER stored in config files. They are stored in the system keyring:

- **Linux**: `secret-service` (GNOME Keyring, KDE Wallet) via `keyring` crate
- **macOS**: Keychain
- **Fallback**: Encrypted file in `$XDG_DATA_HOME/mxr/credentials.enc` (if no keyring available)

The config file only stores a reference key (e.g., `token_ref = "mxr/personal-gmail"`) that the daemon uses to look up the actual credential from the keyring at runtime.

## Config resolution

Config values are resolved in this order (later overrides earlier):

1. Built-in defaults (compiled into the binary)
2. Config file (`config.toml`)
3. Environment variables (`MXR_SYNC_INTERVAL=30`, `MXR_EDITOR=vim`)
4. CLI flags (`mxr --sync-interval 30`)

`mxr config` shows the fully resolved configuration, useful for debugging.
