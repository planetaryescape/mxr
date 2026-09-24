Title: I built a local-first email client in Rust with Ratatui, SQLite and Tantivy

I have been building mxr, an open-source mail app that keeps a local copy of the mailbox and exposes it through a TUI, CLI, web client, MCP server and agent skill.

The long-running daemon owns sync, search and mailbox state. SQLite is the canonical store and Tantivy handles full-text search. The TUI is built with Ratatui. Gmail, Outlook/Microsoft 365 and IMAP are adapters behind the same internal mail model, with SMTP available for sending.

I split the daemon from the clients because sync should continue when the TUI closes, and because my agent should be able to use the same mail operations as I do. The CLI returns JSON and supports dry runs for batch changes.

There is a 50,000-message synthetic demo if you want to try it without connecting an account:

https://mxr.sh/

Source: https://github.com/planetaryescape/mxr

I would be especially interested in feedback on the daemon/client split and the provider adapter model. If you want to help build either of them, contributions are welcome.

Video: clips/11-mxr-is-written-in-rust.mp4
