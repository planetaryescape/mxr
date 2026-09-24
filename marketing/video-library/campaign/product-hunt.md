Name: mxr

Tagline: Local email for your terminal and your agent

Website: https://mxr.sh/

Description:
mxr syncs Gmail, Outlook, Microsoft 365 and IMAP accounts into a local mailbox. Use it through a Vim-friendly TUI, a pipeable CLI, a web client, MCP or an agent skill. SQLite stores the mailbox and Tantivy makes full-history search fast. Batch changes can be previewed before they run.

Maker comment:
I made mxr because I wanted to write email in Vim.

Once the mailbox was syncing into SQLite behind a daemon, I could give the same commands to my agent. It can search and read years of mail locally, use JSON from the CLI, and show me what a batch action will change before it runs.

mxr works with Gmail, Outlook, Microsoft 365, IMAP and SMTP. There is a 50,000-message synthetic demo, so you can try the TUI and CLI without connecting your inbox.

I would love to hear how other people want to use their mail from the terminal or with an agent.

If you try mxr and like it, give the repo a star. Contributions are welcome too, especially new provider adapters and improvements to the terminal experience:

https://github.com/planetaryescape/mxr

Video: clips/10-one-mailbox-three-interfaces.mp4
