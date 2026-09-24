Title: mxr: a Vim-friendly email TUI with local search

I started building mxr because I was tired of leaving the terminal every time I needed to write an email. I wanted Vim keys, `$EDITOR` for composing and fast search across the whole mailbox.

mxr now syncs Gmail, Outlook/Microsoft 365 and IMAP accounts into a local SQLite mailbox. Tantivy handles full-text search, and the daemon keeps syncing when the TUI closes. There is also a pipeable CLI, which turned out to be useful for scripts and agents.

You can open a 50,000-message synthetic inbox with `mxr demo` if you want to try the interface without connecting an account.

https://mxr.sh/

Source: https://github.com/planetaryescape/mxr

I would love feedback from people who spend a lot of time in terminal UIs, especially around navigation and information density. If you want to help improve the TUI, contributions are welcome.

Video: clips/08-tui-searches-local-mail.mp4
