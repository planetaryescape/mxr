# The candidate passed attachment transfers across a filesystem boundary

Source `b55994bdc352cd95db893ceda95e77a065cc20ee`, macOS binary SHA-256 `6742fde492fdeb822e67123d8edfdf46239d53809e465d83a0a047da6759c7c9`, with the embedded web UI. These are historical candidate receipts; subsequent builds reused the recorded binary path. The [manifest](manifest.json) binds each retained receipt and its original evidence file.

- [CLI](cli.json) transferred 1,188,000 bytes through `cmd://` while direct daemon-file reads failed, preserved an existing destination, and created no fallback local store or daemon.
- [MCP](mcp.json) assembled the same bytes from 19 bounded chunks and rejected missing permission, bad ranges and a wrong account assertion.
- [TUI](tui.json) saved exact bytes, required explicit replacement confirmation, and opened a client-owned synthetic text file. The OS opener returned success; external editor contents were not inspected.
- [Browser](browser.json) downloaded 1,280,116 bytes through the denied-cache bridge and used download fallback for HTML Open. Errors and console logs were empty. Passive raster-image Open was not exercised.
- [Old daemon](old-daemon.json) confirms clear unsupported rejection without path fallback. [Old client](old-client.json) confirms that the published v0.6.77 CLI retained its legacy local behavior against the candidate daemon.

All mail and bytes were synthetic; owned processes and browser sessions were stopped. The retained JSON omits verbose command records and duplicate CLI output where listed in the manifest. Original evidence hashes preserve that provenance. The [stage build log](../../s05a-attachment-download.md) records subsequent review, CI and release status.
