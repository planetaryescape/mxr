---
title: Snippets
description: Compose-time shorthand. Type `;name` in a draft and it expands into a saved phrase, signature, or boilerplate block.
---

Snippets are saved chunks of text expanded by typing `;name` inside an `$EDITOR` compose buffer. They're meant for the bits of email you write the same way every time:

- "schedule a 15-min chat" closers
- recruiter responses
- decline templates
- one-liner postscripts

The TUI doesn't intercept; expansion happens on the editor side via mxr's compose helper. The snippet store is global and lives in your config.

## CLI

```bash
mxr snippets list
mxr snippets set sig "— Bhekani · planetaryescape.io"
mxr snippets set decline "Thanks for thinking of me — passing for now. Best of luck with the search."
mxr snippets remove decline
```

Output formats: `--format table|json|jsonl`. Pipe to `jq` to script.

## Use it in a draft

Open a compose buffer:

```bash
mxr compose --to alice@example.com --subject "..."
```

In your editor, type `;sig` and run `mxr-compose-expand` (the helper installed alongside mxr; bound by default in the supplied vim/neovim/helix snippet — see your editor docs). The token expands inline.

For a quick one-shot reply:

```bash
mxr reply MESSAGE_ID --body-stdin <<EOF
Sounds good — see you Friday.

;sig
EOF
```

## Examples

```bash
mxr snippets set followup-3d "Bumping this — has there been any movement since last Tuesday?"
mxr snippets set intro "Hi {first_name}, copying the team on this thread."
mxr snippets set followup "Following up on {thread_subject} ({today})."
mxr snippets set ack "Got it, will follow up by EOD."
```

## Built-in tokens

Snippet bodies — and any text you type directly into the compose
buffer — may reference these tokens. They are resolved at send time,
once your draft has a `to:` and `subject:`:

| Token              | Resolves to                                                                                       |
|--------------------|---------------------------------------------------------------------------------------------------|
| `{first_name}`     | First whitespace-delimited token of the recipient's display name. Falls back to the email's local part if no display name is set. Handles `"Last, First"` directory-export order. |
| `{full_name}`      | Full display name of the recipient (the first address in `to:`).                                  |
| `{thread_subject}` | Draft subject with leading `Re:` / `Fwd:` / `Fw:` chains stripped.                                |
| `{today}` / `{date}` | Today's date in your local timezone (`YYYY-MM-DD`).                                             |
| `{year}`           | Current year (`YYYY`).                                                                            |

A token with no value (e.g. `{first_name}` when the draft has no
recipient yet) is left literal so the [send-time validator](/guides/pre-send-safety/) can warn about it instead of silently
producing "Hi ," in your message.

Custom `{var}` tokens you invent (`{deadline}`, `{cc_team}`, …) are
treated as user-fillable — they pass through unchanged and are flagged
at send-time, so you don't ship a message with a half-edited template.

```bash
# Show me what's installed; pipe to jq for scripts.
mxr snippets list --format json | jq -r '.[].name'
```

## TUI

There's a read-only **Snippets browser** modal:

- `Ctrl-p` (palette) → "Snippets"
- `j`/`k` to navigate, body preview on the right, `Esc` to close

CRUD goes through the CLI on purpose — your editor is the canonical place to draft prose, not a TUI form.

## See also

- [Compose](/guides/compose/) — the `$EDITOR` workflow that snippets ride on
- [LLM features](/guides/llm-features/) — when you want the system to draft, not just paste
- [CLI: snippets](/reference/cli/snippets/) — every flag
