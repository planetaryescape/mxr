# rmcp 3.9.0 is waiting on the 7-day quarantine

Status: open until 2026-11-12. Seen 2026-11-11.

Three rmcp advisories landed together. mxr can reach none of them: `mxr-mcp` builds rmcp with the `client`, `server` and `transport-io` features, so it speaks MCP over stdio and has no OAuth client and no Streamable HTTP transport.

| Advisory | Affected | Fixed in | State |
|---|---|---|---|
| RUSTSEC-2026-0457, OAuth client SSRF filter checks host strings | 2.0.0 to 3.6.x | 3.7.0 | fixed by rmcp 3.8.0 |
| RUSTSEC-2026-0461, unbounded tool schema cache in `StreamableHttpService` | 3.0.0 to 3.5.x | 3.6.0 | fixed by rmcp 3.8.0 |
| RUSTSEC-2026-0462, Streamable HTTP clients buffer whole response bodies | before 3.9.0 | 3.9.0 | ignored in `deny.toml` |

rmcp 3.8.0 was published on 2026-10-26. 3.9.0 was published on 2026-11-05, so it clears the 7-day minimum on 2026-11-12.

## On 2026-11-12

1. Set `rmcp = "3.9.0"` in the workspace `Cargo.toml` and run `cargo update -p rmcp --precise 3.9.0`.
2. Delete the RUSTSEC-2026-0462 entry from `deny.toml`.
3. Run `cargo deny check advisories` and `scripts/cargo-test -p mxr-mcp --tests`, then delete this file.
