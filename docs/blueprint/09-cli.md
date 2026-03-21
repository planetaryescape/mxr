# mxr — CLI & Shell Integration

## CLI design

The CLI and TUI are both daemon clients. CLI commands talk to the daemon over the Unix socket, so scripts hit the same system the TUI uses.

The CLI is the canonical surface. New capabilities land in the CLI first or at the same time as the TUI. The TUI is a faster interactive layer over the same daemon requests, not a separate product surface.

## Search-oriented commands

```text
mxr search "query"
mxr search "query" --mode lexical|hybrid|semantic
mxr search "query" --format table|json|csv|ids
mxr search "query" --limit 100
mxr search "query" --explain

mxr count "query"
mxr count "query" --mode lexical|hybrid|semantic

mxr saved add "Unread invoices" "subject:invoice is:unread" --mode lexical
mxr saved list
mxr saved run "Unread invoices"
mxr saved delete "Unread invoices"
```

`lexical` remains the default until semantic search is enabled and the user or saved search opts into another mode.

`mxr search --explain` returns:

- requested mode
- executed mode after fallback
- lexical/dense candidate windows and counts
- semantic query text when used
- fallback/debug notes
- per-result lexical/dense contribution details

## Mutation discipline

Mutations should be safe to script:

- destructive and batch commands must support `--dry-run`
- preview paths should reuse the same selection logic as the real mutation
- `--yes` or explicit confirmation gates commit for broad mutations

Examples:

```text
mxr archive --search "older:30d label:notifications" --dry-run
mxr trash --search "from:spam@example.com" --dry-run
mxr move --search "label:triage" --to work/todo --dry-run
```

## Semantic commands

```text
mxr semantic status
mxr semantic enable
mxr semantic disable
mxr semantic reindex

mxr semantic profile list
mxr semantic profile install bge-small-en-v1.5
mxr semantic profile install multilingual-e5-small
mxr semantic profile install bge-m3
mxr semantic profile use multilingual-e5-small
```

These commands manage:

- model installation
- active profile selection
- semantic index lifecycle
- operator-visible status and errors

## Diagnostics

```text
mxr restart
mxr doctor
mxr doctor --reindex
mxr doctor --reindex-semantic
mxr doctor --semantic-status
mxr doctor --format table|json|csv|ids
```

`mxr restart` restarts the daemon with the current binary. Clients should use this when the running daemon build/protocol no longer matches the invoking binary.

`mxr status` and `mxr doctor` expose daemon lifecycle state with explicit health classes:

- `healthy`
- `degraded`
- `restart_required`
- `repair_required`

`--reindex` rebuilds Tantivy. `--reindex-semantic` rebuilds the active semantic profile from SQLite.

Daemon lifecycle policy:

- stale socket cleanup is automatic
- daemon build/protocol mismatch should trigger restart, not silent failure
- search index corruption/emptiness should trigger targeted repair or rebuild
- provider sync errors degrade health but should not cause daemon self-restarts
- daemon update checks, if added later, should be opt-in notices only; never self-apply from inside the daemon

## Broader CLI surface

```text
mxr                         Open TUI
mxr daemon                  Start daemon explicitly
mxr daemon --foreground     Start daemon in foreground
mxr restart                 Restart daemon with current binary
mxr sync                    Trigger sync for all accounts
mxr sync --account work     Trigger sync for one account
mxr compose                 Open $EDITOR for new message
mxr reply MESSAGE_ID        Open $EDITOR for reply
mxr forward MESSAGE_ID      Open $EDITOR for forward
mxr drafts                  List drafts
mxr send DRAFT_ID           Send a draft
mxr export THREAD_ID        Export a thread
mxr accounts                List configured accounts
mxr labels                  List labels with counts
mxr config                  Show resolved configuration
mxr version                 Version info
```

## Output formats

Data commands support:

- `table`
- `json`
- `csv`
- `ids`

Machine-readable output is a first-class feature. JSON is for pipes, scripts, and agents, not only diagnostics.

Examples:

```bash
mxr search "subject:invoice is:unread" --mode hybrid --format json | jq -r '.results[].subject'
mxr count "label:work after:2026-01-01" --mode lexical
mxr semantic status --format json | jq .
mxr doctor --semantic-status --format json | jq .
```

## Shell integration

### Completions

```bash
mxr completions bash > /etc/bash_completion.d/mxr
mxr completions zsh > ~/.zfunc/_mxr
mxr completions fish > ~/.config/fish/completions/mxr.fish
```

### Exit codes

- `0`: success
- `1`: general error
- `2`: usage error
- `3`: daemon not running
- `4`: auth error
