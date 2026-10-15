# Assistant follow-ups

Open work after the assistant shipped (blueprint [25](../blueprint/25-assistant.md), D122). Status: open, none started.

## The TUI has no assistant pane

Client parity says a daemon capability needs CLI, TUI, web and MCP. The TUI reaches the assistant only through `mxr assistant` in another terminal. A right-hand pane in `crates/tui/src/app/draw.rs` (a third horizontal split), with approval and question prompts in the status line, would close it. `Mod+.` is listed in `KEYMAP_DIFFERENCES` as web-only until then. MCP stays deliberately without it (an agent driving the assistant could approve its own changes).

## Replies don't stream

`mxr-llm` has no streaming, so the sidebar shows steps and a status line, then the reply all at once. Claude's Messages API and chat-completions both stream; the turn would emit text deltas as `AssistantUpdated` progress. Needs tolerant partial-JSON handling for tool inputs.

## Small-context local models fill up

History is append-only (Anthropic rejects edited history), and the tool list is about 11k tokens. A local model with an 8k or 32k window fails a long conversation with a provider error. Options: start a new conversation automatically with a summary, Anthropic server-side compaction for Claude, or a smaller tool menu for small models (which costs prompt-cache reuse).

## Every change needs its own click

There is no "approve every change in this turn". Notto deliberately has no per-request exemption because a model once used one to bypass its own supervision; if this is added it should be a user setting checked in the daemon, never something the model can ask for.

## The web activity filter lists five sources

`features/activity/ActivityRoute.tsx` offers tui, cli, web, daemon and assistant. Agent, MCP, human and script rows are now recorded (migration 89) but can't be filtered in the web app.

## Re-running `mxr demo` duplicates its saved searches

Running `mxr demo --no-tui` three times on one profile without `--reset` left four copies of each demo saved search, and React warns about duplicate keys in the sidebar. Seen while testing the assistant on 2026-10-15; not investigated.
