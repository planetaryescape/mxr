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
mxr snippets set intro "Hi {{name}}, copying {{cc}} so we have one thread on this."
mxr snippets set ack "Got it, will follow up by EOD."
```

mxr does not interpret template variables (`{{name}}`); they pass through as literal text. The convention exists for visual recognition, not auto-fill — fill them in your editor.

## TUI

There's a read-only **Snippets browser** modal:

- `Ctrl-p` (palette) → "Snippets"
- `j`/`k` to navigate, body preview on the right, `Esc` to close

CRUD goes through the CLI on purpose — your editor is the canonical place to draft prose, not a TUI form.

## See also

- [Compose](/guides/compose/) — the `$EDITOR` workflow that snippets ride on
- [LLM features](/guides/llm-features/) — when you want the system to draft, not just paste
- [CLI: snippets](/reference/cli/snippets/) — every flag
