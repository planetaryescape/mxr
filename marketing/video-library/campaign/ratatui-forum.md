Title: mxr: a local-first email client built with Ratatui

I built mxr because I wanted to handle email without leaving the terminal. The TUI has Vim-style navigation, `$EDITOR` for composing and local full-text search.

The TUI is one client of a long-running daemon. The daemon syncs Gmail, Outlook/Microsoft 365 and IMAP accounts, keeps the canonical mailbox in SQLite and maintains a Tantivy index. Closing the TUI does not stop sync. The same daemon also serves the CLI, web client, MCP server and agent skill.

You can try the interface against a 50,000-message synthetic inbox with `mxr demo`.

Site: https://mxr.sh/

Source: https://github.com/planetaryescape/mxr

I would appreciate feedback on the navigation, keyboard model and how much information the three-pane layout shows at once. If you want to help improve the TUI, contributions are welcome.

Video: clips/09-tui-mail-tools.mp4
