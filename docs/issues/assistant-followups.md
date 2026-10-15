# Assistant follow-ups

Open work on the assistant (blueprint [25](../blueprint/25-assistant.md), D122). Status: resolved on branch `feat/assistant-tui-followups`; nothing open.

## The TUI history list can't delete

Resolved. In the list `L` opens, `D` asks the daemon for a dry run of deleting the highlighted conversation, shows what goes, and deletes it on `y`, through the same `DeleteAssistantConversation` request the CLI and web use. Deleting the open conversation leaves a fresh one. A refusal, such as a conversation that is still running, shows under the list.

## TUI approvals don't show the exact call

Resolved. `x` on a waiting approval unfolds the tool and its arguments as indented JSON, and the raw preview when the card can't put it in words, as the web's "Exact call" does. The next approval starts folded.

## Toasts cover the TUI pane's hint line

Resolved. While the pane is open, toasts stack bottom-right of the area left of it, so the pane's key hints stay visible.
