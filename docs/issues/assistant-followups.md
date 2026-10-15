# Assistant follow-ups

Open work on the assistant (blueprint [25](../blueprint/25-assistant.md), D122). Status: open, none started.

## The TUI history list can't delete

The web's history menu deletes a past conversation in place. In the TUI, `L` lists conversations but deleting means opening one and pressing `D`.

## TUI approvals don't show the exact call

The web's approval card has an "Exact call" toggle with the tool and arguments that run on approval. The TUI card shows the change in words, the reason and the messages it touches, but not the call itself.

## Toasts cover the TUI pane's hint line

Toasts draw over the bottom-right of the screen, which is where the pane's key hints sit when it is open.

## The `reason` field costs about 2,700 tokens a request

Every tool's schema gains a required `reason` with a one-sentence description, about 150 characters across 73 tools. With the tool list near 15,000 tokens, a model with a 20,000-token window can't summarise its way to room (the fixed part is over three quarters of the window), so it runs until the provider refuses. A shorter description, or describing `reason` once in the system prompt, would win back most of it.
