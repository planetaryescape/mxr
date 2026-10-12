# Published v0.6.80 exposed a TUI directory regression

Source `0067e774fc690f67df481f9f9ef4568d3d4ff442`, tree `5c9393f4dd65e09d1c879c387d7a135a3ac8e289`, produced the published macOS binary SHA-256 `108af1a80beffe7eb195154d37440981480a8daeac77b0436a541b53a33f1e54`. Archive checksums, the isolated installer and Homebrew matched. CLI and MCP transferred exact synthetic bytes across a denied filesystem boundary. Browser HTML download/fallback and passive text Open passed.

The TUI failed to save to a missing Downloads directory with ENOENT. Creating that fixture directory allowed the remaining download and overwrite checks to pass, but does not establish the stage promise. PR404 restores parent creation in the shared client. A corrected published release must pass the original missing-directory journey before S05a is complete.

Only synthetic mail was used. TUI and browser fixture processes were stopped. The browser emitted a missing dialog-description warning but no console errors. External editor contents and raster-image Open were not inspected. Raw CLI command records are omitted; the manifest binds original receipts to these retained copies.
