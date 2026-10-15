# Assistant follow-ups

Open work after the assistant shipped (blueprint [25](../blueprint/25-assistant.md), D122). Status: open, none started.

## The TUI has no assistant pane

Client parity says a daemon capability needs CLI, TUI, web and MCP. The TUI reaches the assistant only through `mxr assistant` in another terminal. A right-hand pane in `crates/tui/src/app/draw.rs` (a third horizontal split), with approval and question prompts in the status line, would close it. `Mod+.` is listed in `KEYMAP_DIFFERENCES` as web-only until then. MCP stays deliberately without it (an agent driving the assistant could approve its own changes).
