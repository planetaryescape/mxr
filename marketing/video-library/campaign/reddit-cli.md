Title: I made my mailbox scriptable with a local CLI

mxr syncs Gmail, Outlook/Microsoft 365 and IMAP accounts into a local mailbox, then exposes mail operations through a CLI.

Most read commands can return JSON, so I can combine them with jq or use them from an agent. Search runs on a local Tantivy index. Batch changes support dry runs that resolve the query and show the exact messages first.

The same daemon also serves a TUI, web client and MCP server. It is written in Rust and open source:

https://mxr.sh/

Source: https://github.com/planetaryescape/mxr

If you try it and find it useful, give the repo a star. Contributions to the CLI and provider adapters are welcome too.

Video: clips/06-cli-pipes-mail-through-jq.mp4
