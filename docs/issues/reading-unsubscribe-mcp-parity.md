# MCP has no Reading unsubscribe tool

Status: open. Found on `origin/feat/reading-unsubscribe-choice`
`5e4c88b3c1f4c85abf41b0ae4978847da3041ec9` during PR #319's preview contract repair.

The daemon exposes `Request::UnsubscribePurge` in
`crates/protocol/src/types.rs` and dispatches it in
`crates/daemon/src/handler/mod.rs`. CLI, TUI and web callers exist. The MCP tool
router in `crates/mcp/src/lib.rs` exposes neither `Unsubscribe` nor
`UnsubscribePurge`; `rg -ni unsubscribe crates/mcp/src` returned no matches at
that ref. Agents connected through MCP cannot use this capability.

Add a sender unsubscribe tool on the existing daemon preview/commit surface.
Expose the preview token and explicit keep-mail or clear-mail choice, preserve
account scoping, and test refusal of stale and reused tokens. This gap predates
the repair; adding the first MCP unsubscribe tool is separate work.
