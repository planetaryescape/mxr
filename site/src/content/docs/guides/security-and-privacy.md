---
title: Security & Privacy
description: What stays local in mxr, what guardrails exist today, and which safety features are still pending.
---

mxr is local-first by design.

Mail syncs from the provider into SQLite on your machine. Search runs against the local index. The daemon, TUI, CLI, and agent workflows all operate on that local state. There is no hosted mxr relay in the middle.

## What stays local

- SQLite is the canonical store
- Tantivy index is local and rebuildable
- The daemon runs on your machine
- The TUI and CLI talk to the daemon over a local Unix socket
- The web app talks to the same daemon through a loopback HTTP/WebSocket bridge

## What still talks to a provider

- Sync
- Send
- Provider-side mutations like archive, trash, labels, and spam
- Browser handoff for HTML or unsubscribe pages when needed

There is no mxr-operated service in that list. Six other things can reach the network, each because you configured or asked for it:

- The model endpoint under `[llm]`, when you turn model features on and point them at a cloud provider.
- The semantic search model weights, downloaded once from Hugging Face unless `search.semantic.auto_download_models = false`. Embedding runs on your CPU after that.
- Remote images in HTML mail, unless `render.html_remote_content = false` (the web app has its own Remote images toggle).
- The unsubscribe endpoint a sender chose, when you run `mxr unsubscribe`.
- [Marginalia](/guides/marginalia/), once you connect it. `O` on an item, or `mxr reading marginalia send`, sends the item's text there. Marginalia may embed its opening text with the embedding provider your library is set up with. Sync then reads highlights back from it. Nothing is sent until you press `O` or run `send`.
- The site a newsletter links to, when you fetch its article in [Reading](/guides/reading/#fetch-the-linked-article-only-when-you-ask) (`L`, the reader's Article tab, `b` on a link, `O` on a digest link whose article isn't saved, or `mxr reading open ITEM --article`). The client names the site first. The fetch refuses this machine and private networks by name and by every resolved address, checks each redirect, uses no proxy and stops at 5 MB. Nothing fetches an article on its own.

The daemon also downloads one kind of attachment before you open it: the PDFs of
emails [Archive](/guides/archive/) filed as records, from your own provider,
newest first, up to `records.pdf_budget_mb` (512 MB) in total and skipping
any PDF over `records.pdf_max_file_mb` (15 MB). They go to the same
attachment cache as the files you open, so they open offline and their text
is searchable. Set `records.pdf_prefetch = false` to download them only when
you open them.

Records themselves are local rows in SQLite, read from your mail by rules
and schema.org markup on your machine; no model sees them. The activity log
records no record fields: Archive's requests are not written to it.

[For agents](/guides/for-agents/#what-stays-local-what-doesnt) has the config that turns the last three off.

## Model features are off by default and go only where you point them

`[llm] enabled` defaults to `false`, and the default endpoint is a model server on `localhost`. With a local endpoint, prompts stay on your machine: mxr calls a loopback endpoint directly, ignoring `HTTP_PROXY`, and never follows a redirect from an LLM endpoint.

A cloud endpoint receives the mail each feature works on, and that is mail other people wrote to you. Use a provider whose API data terms cover that mail, with an API key you supply: in the environment variable named by `api_key_env`, or for Claude and GPT models stored with `mxr llm key set` (piped in, kept in `secrets.toml` at mode 0600 and never in `config.toml`). Claude models go to Anthropic and OpenRouter, GPT models to OpenAI and OpenRouter, and all always count as cloud. mxr ships no key and runs no relay. Besides the commands you run and the threads you open, two background jobs reach a cloud endpoint without a separate opt-in: row gists for conversations from people that a client shows (and for automated mail while the web app's Updates page is open), and confirmation of mail the delivery heuristic shortlisted. Features that carry your wider history (relationship summaries, commitments, voice matching, answer coverage, briefings, decisions, experts, `mxr ask` and the [assistant](/guides/assistant/)) are refused on a cloud endpoint unless you set `llm.allow_cloud_relationship_data = true`. Assistant conversations, including the mail text it read, are kept in your local database, left out of portable backups, and deleted with `mxr assistant delete`. Every change the assistant makes waits for your approval unless you let a conversation run reversible changes (archive, read, star, moves) on its own; sending, deleting, trash, spam and rules always ask. [LLM features](/guides/llm-features/#know-what-each-request-sends-and-where) lists what each request sends.

```bash
mxr llm status --format json
```

`enabled` and `base_url` say whether model features are on and where they go.

Released mxr contains no tracing code. A developer can [build mxr with the `langfuse` feature](/guides/trace-model-calls/) to record each model call, prompt and reply included, in a Langfuse they run themselves. That build sends traces only to a Langfuse on the same machine and refuses any other address, Langfuse Cloud included.

The [email modes](/guides/email-modes/) plan adds background classification of every incoming message. That work is planned, not shipped, and it will run on a loopback endpoint unless you point its tier at a cloud endpoint and set `allow_cloud_background_classification = true`. Running everything in the cloud with your own key will be supported, and turning it on will say that the text of every incoming message goes to that provider.

## Deleting an email removes most of what mxr derived from it

When your provider deletes or expunges a message and mxr syncs that change,
the daemon deletes the local message and clears derived database state in
the same transaction. It also queues cleanup outside SQLite so a restart
does not lose track of files and index entries that still need removing.

| Data | What deletion does |
|---|---|
| Body, headers, labels, flags, snooze, reply-later state, pins, invites and reply timing pairs | Removed with the message |
| Thread summaries, row gists, recipient briefings and affected relationship summaries | Invalidated; later requests can rebuild them from remaining mail |
| Extracted promises | Removed when their source message is deleted |
| Decisions and deliveries | Lose the deleted evidence; removed when no evidence remains |
| Archive records | Lose fields from that source; recomputed from remaining sources, or deleted with their last source |
| Automatically detected tasks | Removed with their source; tasks you made, edited or scheduled remain, with copied evidence and source pointers cleared |
| Reading items and their dependent data | Removed with their source message |
| Semantic chunks and embeddings in SQLite | Removed with the message |
| Keyword and in-memory semantic index entries, managed attachment cache files | Cleaned by sync and the durable cleanup queue |

File/index cleanup is not a secure-erasure guarantee. Failed file cleanup
uses bounded retries and is reported in daemon logs; a file the daemon
cannot remove may remain. If the in-memory semantic index cannot forget
a message, its next rebuild or daemon restart drops the entry. Copies you
exported or saved outside the managed cache are yours to remove.

User-authored drafts and corrections, tasks you chose to keep, and no-text
operation/receipt identities are retained deliberately. A record with other
source mail can keep your corrected field values. Contacts with remaining
mail are not deleted merely because one message is gone. Assistant
conversation history is separate: use `mxr assistant delete` to remove a
conversation and its transcript.

Moving mail to Trash is not a delete: on Gmail the message keeps its derived data until Gmail empties the trash and reports the deletion. mxr has no command that deletes a message permanently.

To remove everything at once, preview the reset first:

```bash
mxr reset --hard --dry-run
```

A hard reset deletes the database, both indexes, the model cache, logs, and the attachment cache when it lives inside the data directory. It keeps `config.toml` unless you add `--including-config`, and it leaves `secrets.toml` alone either way.

New stores must define how they follow message deletion. That rule is
implemented for the derived data above; it does not mean deleting your own
writing, exported files or the retry identities needed to prevent duplicate
effects.

## Guardrails that exist today

- `--dry-run` on risky mutation commands (including `mxr send` and `mxr unsnooze --all`)
- Interactive confirmation for destructive and batch mutation flows unless `--yes` is set
- Undoable mutations: `archive`, `trash`, `spam`, `read`, `read-archive` print a `mutation_id` you can pass to `mxr undo` for ~60s
- Persisted mutation history through `mxr history`
- Event and log views through diagnostics and CLI commands
- Plain-text-first reader mode, with browser escape hatch for original HTML
- Daemon IPC socket permissions are set to `0600` on Unix.
- The bridge requires bearer auth for every authority-bearing route. Only `/api/v1/health`, `/api/v1/auth/local-token`, and `/api/v1/i18n` are unauthenticated bootstrap/read-only routes.
- The bridge checks Host/CORS allowlists and adds frame, content-sniffing, and referrer-policy headers.
- Saved attachments and rendered HTML assets are written `0600` on Unix.
- User-initiated attachment downloads are limited to the configured downloads directory, the current directory, or the system temp directory.
- Remote HTML assets are capped before writing, even when the server omits `Content-Length`.

### Where credentials live

**IMAP/SMTP passwords are stored on disk, keychain-optional.** They live in
`<config_dir>/secrets.toml` — a plaintext TOML file at mode `0600` (owner
read/write only), keyed by `password_ref` + username. This is the same model
normal CLIs use (`~/.aws/credentials`, `~/.config/gh/hosts.yml`): the deliberate
tradeoff is plaintext-at-rest, protected only by filesystem permissions rather
than OS encryption. mxr chose disk-first because an ad-hoc-signed release binary
loses its OS-keychain read access on every upgrade — which used to hard-fail
daemon startup for password accounts. A `0600` file survives upgrades untouched
and is readable only by your own processes.

The OS-native secret store is an **optional fallback**:

- **macOS**: Keychain (Keychain Access)
- **Linux**: Secret Service (e.g. GNOME Keyring, KWallet)

On the first read after upgrading from a keychain-only version, an IMAP/SMTP
credential found in the keychain is automatically migrated (mirrored) into
`secrets.toml` and served from disk thereafter. `mxr accounts add` and
`mxr accounts repair NAME` write `secrets.toml` (disk-authoritative) with a
best-effort keychain mirror that never blocks the operation. The on-disk
`config.toml` only references credentials by `password_ref`; it never stores the
password itself.

Password lookup happens when an account connects or syncs, not while the daemon
is being constructed. An unreadable password should fail that account's
operation without preventing the daemon or other configured accounts from
starting. Set `MXR_KEYCHAIN=off` to disable keychain reads and writes for
IMAP/SMTP passwords; with that setting, `secrets.toml` is their only source.
The setting does not affect Gmail or Outlook OAuth token storage.

Gmail OAuth refresh tokens are stored in the OS keychain with a private disk
fallback under the active token dir, so a noninteractive keychain failure does
not strand an otherwise valid account. Outlook OAuth tokens are JSON files under
the active token dir (`<data_dir>/tokens` by default, `MXR_TOKEN_DIR` when set).

`secrets.toml` lives in the config dir, so `mxr reset` and `mxr reset --hard`
preserve it — your credentials survive a runtime-state wipe. Override its
location with `MXR_SECRETS_PATH`. Protect it like any dotfile secret: keep the
`0600` mode and do not commit it to version control.

### Backup and restore

mxr does not run a hosted backup service. Use its stopped-profile backup
workflow instead of copying a live database. Backups preserve cached mail,
drafts, and managed attachment bytes, but exclude credentials and tokens.
Restored drafts remain held; activation permits new work after review but does
not release original draft IDs or resume old schedules. See [Back up and
restore a workspace](/guides/backup-recovery/) for the full procedure and
limits.

### Bridge and local IPC boundary

The Unix socket is a local user boundary. Any process that can connect
as the same OS user can drive the daemon with that user's authority, so
mxr keeps the socket owner-only and expects it to live under a user-owned
runtime directory.

The HTTP bridge is broader because browsers cannot open Unix sockets. It
binds to loopback by default, uses a bearer token stored under the active
profile config directory, rejects DNS-rebinding-shaped Host headers, and
keeps API docs behind the same auth gate as the rest of the API.

### Attachments and remote content

Attachment names are sanitized before mxr writes local files, including
Windows reserved names such as `CON` and `LPT1`. Explicit downloads are
constrained to safe destination roots. Inline and remote HTML assets live
under mxr's attachment cache, get private file permissions, and remote
asset fetches have a fixed body-size cap.

Attachment transfers return bounded chunks with message and attachment IDs,
then clients verify the complete length and SHA-256. CLI and TUI save/open
operations write on the client machine. The browser verifies the assembled
bytes before downloading or opening a passive text or raster image in a
separate tab; HTML, SVG, PDF and other types download instead. Provider fetches
still materialize a complete byte vector on the daemon, and browser transfers
assemble a complete Blob in memory.

## Agent and MCP boundaries

The first-party MCP server and agent-origin permission profiles now run through
the daemon's normal IPC boundary. Configure `read-only`, `draft-only`,
`restricted`, or `full` policy per origin; limit accessible accounts; and gate
send or destructive operations separately. MCP send and mutation tools also
require `confirm=true`.

```toml
[agents.profiles.mcp]
safety_policy = "draft-only"
allowed_accounts = ["work"]
allow_send = false
allow_destructive = false
```

These controls reduce blast radius. They do not make email trustworthy. Every
email field and attachment remains untrusted data, so an agent must never treat
mail content as permission or instructions. See [For agents](/guides/for-agents/)
for the operating rule and worked examples.

## Practical advice

- Use `--dry-run` before any batch mutation.
- Use app passwords or provider-specific credentials where your provider recommends them.
- Keep `secrets.toml` at mode `0600`, and keep any fallback keychain entries scoped to the accounts you use.
- If an agent is involved, prefer workflows that search, read, export, and draft before workflows that mutate.

## Supplied HTML

mxr accepts a designed HTML email and preserves it exactly. That means it does
not sanitise it either, so validation is a gate rather than a filter:

- Active content — `<script>`, `<object>`, `<embed>`, `<applet>`, `<iframe>`,
  `<form>`, inline `on*` handlers, `javascript:`/`vbscript:` URLs, `<style>`
  blocks containing `expression()`, and anything of the sort hidden inside a
  conditional comment — is **reported and refused**. The draft is not created
  and the document is not modified.
- `data:` URLs are allowed only for `data:image/*`.
- The check runs in the daemon, not only in the CLI, so an IPC client cannot
  route around it.
- Validation parses with html5ever to inspect the document. A hostile document
  could in principle be parsed differently by a specific mail client, so this is
  a strong gate rather than a proof.

Content IDs supplied via `--inline` are restricted to letters, digits and
`. _ - +`, which keeps a CR/LF out of a `Content-ID` header.

Web previews of an HTML draft render in a sandboxed frame with a policy that
blocks remote loads, so opening a draft does not phone home to a sender's
server.

## What HTML drafts put in the activity log

The activity log records the content kind (`markdown` or `html`) and the number
of inline assets. It does not record the HTML, the text alternative, inline
asset paths, recipient addresses, or any template property value. See
[the activity log](/guides/activity-log/).
