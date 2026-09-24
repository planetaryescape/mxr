Title: Show HN: mxr - local email for the terminal and your agent

I built mxr because I wanted to write email in Vim. It became a local mail engine with a TUI, CLI, web client, MCP server and agent skill.

It syncs Gmail, Outlook, Microsoft 365 and IMAP accounts into one local mailbox. SQLite stores the mailbox and Tantivy indexes it, so search does not need a round trip to the provider. The daemon handles sync and mailbox state while the clients come and go.

The agent part is what I use most now. An agent can discover the CLI through `mxr --help`, consume JSON, search and read mail, and dry-run changes before applying them.

The project is written in Rust and open source. There is a 50,000-message synthetic demo, so you can try the TUI and CLI without connecting an inbox.

Site: https://mxr.sh/

Source: https://github.com/planetaryescape/mxr

Contributions are welcome, especially new provider adapters and improvements to the terminal experience.
