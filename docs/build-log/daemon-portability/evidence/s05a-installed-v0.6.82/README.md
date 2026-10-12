# Published v0.6.82 passes all attachment clients

Source `5007eb3f95484ec3a067fe5d3126d3aa8d1fa9d3`, tree `43269888f7c907d6c77043ee65265e33259f8e45`, produced macOS binary SHA-256 `3bb6d6b178cf97aa3aaeae246667cea001a3523c84d18fac794a8d16879c35a4`. Both archives and checksum files matched their GitHub asset digests; the standalone installer and Homebrew matched the macOS binary. [Release workflow](https://github.com/planetaryescape/mxr/actions/runs/38187913750) passed.

The installed CLI transferred 1,188,000 bytes through a command connector while denied access to the daemon files. MCP assembled the same bytes from 19 chunks and rejected invalid ranges, wrong accounts and missing mutation permission. The real TUI created a missing Downloads directory, wrote the exact bytes with mode `0600`, preserved an existing file until overwrite confirmation, and opened verified text from its client cache.

The browser downloaded 1,280,116 bytes of synthetic HTML. Open used the download fallback without opening a second tab or running the fixture script. A separate 810,000-byte text attachment opened in a passive tab whose rendered text hash matched. All observed attachment routes returned 200. The console contained only a missing dialog-description warning.

Only synthetic mail was used. All owned fixture processes and browser sessions were stopped; sockets, listeners and fixture bridge tokens were removed. External editor contents and raster-image Open were not inspected. Linux artifacts were checksum-verified; these consuming-client journeys ran on macOS. Raw CLI/MCP command records are omitted; the manifest binds originals to retained receipts.

S05a is done. The v0.6.80 missing-directory regression remains in its historical packet; v0.6.82 is the corrected accepted release.
The production mailbox guide deployment is recorded in [`docs-deployment.json`](docs-deployment.json). Its source was `site/src/content/docs/guides/mailbox.md` at `12109eac12265b8e1e2567c49cdb7289b44c69ce`; [`docs-deployment-source.json`](docs-deployment-source.json) records the verified source-content hash and deployment receipt hash.
