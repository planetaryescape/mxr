# mxr — CLI & Shell Integration

## CLI design

The CLI and TUI are both daemon clients. The CLI is the canonical user and automation surface.

New capabilities should land in the CLI first or at the same time as the TUI. The TUI is not a separate mail/search system.

## Search-oriented commands

```text
mxr search "query"
mxr search "query" --mode lexical|hybrid|semantic
mxr search "query" --format table|json|csv|ids
mxr search "query" --limit 100
mxr search "query" --explain

mxr count "query"
mxr count "query" --mode lexical|hybrid|semantic
```

Saved search examples:

```text
mxr saved add "Unread invoices" "subject:invoice is:unread" --mode lexical
mxr saved add "Loose body recall" "body:house of cards" --mode hybrid
mxr saved list
mxr saved run "Loose body recall"
```

## Search modes

- `lexical`: exact/local Tantivy BM25
- `hybrid`: BM25 + dense retrieval + RRF
- `semantic`: dense retrieval only, with the same structured filters applied afterward

`lexical` stays the default until semantic search is enabled and the caller or saved search opts into another mode.

## `--explain`

`mxr search --explain` returns:

- requested mode
- executed mode after fallback
- semantic query text when used
- lexical/dense candidate windows and counts
- fallback/debug notes
- per-result lexical/dense contribution details

Fallback notes must stay honest.

Examples:

- semantic unavailable in this binary
- semantic search disabled in config
- query has no semantic text terms
- query contains negated semantic terms
- dense retrieval returned no candidates

## Fielded semantic examples

```bash
mxr search "body:house of cards" --mode hybrid --explain
mxr search "subject:house of cards" --mode hybrid --explain
mxr search "filename:house of cards" --mode hybrid --explain
```

Expected behavior:

- lexical side stays literal and field-aware
- dense side respects chunk source kinds
- hybrid fuses both with RRF

Dense source-kind mapping:

- `subject:` -> header chunks
- `body:` -> body chunks
- `filename:` -> attachment-origin chunks

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

- local model installation
- active profile selection
- semantic index lifecycle
- operator-visible status

Important behavior:

- enabling semantic installs/uses the active local profile
- profile switching rebuilds embeddings for the new profile from stored chunks
- reindex is the full chunk + embedding rebuild path

## Diagnostics

```text
mxr restart
mxr doctor
mxr doctor --reindex
mxr doctor --reindex-semantic
mxr doctor --semantic-status
mxr doctor --format table|json|csv|ids
```

`--reindex` rebuilds Tantivy.

`--reindex-semantic` rebuilds the active semantic profile from message content via:

1. chunk rebuild
2. embedding rebuild
3. ANN rebuild

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

Machine-readable output is a product feature.

Examples:

```bash
mxr search "subject:invoice is:unread" --mode hybrid --format json | jq .
mxr search "body:house of cards" --mode hybrid --explain
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
