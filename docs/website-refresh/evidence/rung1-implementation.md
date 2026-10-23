# Rung 1 implementation evidence

The homepage and Why mxr opening now describe mxr as a local mail runtime for
people and agents. They distinguish a provider connector from a locally owned
mailbox that continues syncing with its provider, explain that clients can
reuse daemon operations and applicable policy checks, and say that people and
agents using the same daemon share mailbox state. Client paths can expose
different operations and authority. The page keeps the human route, For agents,
the brew install command, all five modes and Now, and an isolated synthetic
trial visible. Its search rows and offline sequence are labeled illustrations,
not captured output.

## Runtime evidence and limits

S2 evidence was produced separately at commit
`b10355038354d3a03fa21609d6c9adc42997e55e`; its proof is
`docs/website-refresh/evidence/s2-runtime-proof.md`. That run used the published
`mxr 0.6.156` binary from source commit
`f39ae66578e3099e464967ab2f042e24b1ad8ec4` against an isolated fake-provider
profile. It synced 55 messages into each of `personal` and `work`. The scoped
subscription captures at `.cache/website-s2/subscriptions-personal.json` and
`subscriptions-work.json` each contain four sender buckets; each bucket has
`message_count: 1` and `opened_count: 0`. This provides no useful READ-based
ranking variation, so the homepage retains its explicitly synthetic example
instead of presenting those captures as a compelling result.

An unscoped subscriptions request resolves to the configured default account,
not all accounts. The S2 proof ran separate scoped requests with
`subscriptions --account personal --limit 200 --rank --format json` and
`subscriptions --account work --limit 200 --rank --format json`. The homepage
does not claim that its unscoped demo command aggregates accounts.

These behaviors were checked against source at
`f39ae66578e3099e464967ab2f042e24b1ad8ec4`:

- `crates/daemon/src/handler/diagnostics/mod.rs#L428-L441` resolves an omitted
  subscription account to the configured default.
- `crates/daemon/src/cli/mod.rs#L1063-L1075` defines the 200-result default,
  `--account`, and `--rank` options.
- `crates/store/src/message.rs#L1109-L1114` counts messages and current READ
  flags per account and sender; `#L1150-L1156` selects the latest sender row
  and derives `opened_count` from the READ flag.
- `crates/daemon/src/lib.rs#L75-L87` routes subsequent CLI commands through
  the isolated demo environment after `mxr demo` is active.
- `crates/protocol/src/lib.rs#L1-L16` defines the shared daemon contract for
  CLI, TUI, web, and MCP clients; `crates/mcp/src/lib.rs#L90-L115` connects MCP
  requests to the selected daemon.
- `crates/daemon/src/handler/account_scope.rs#L1-L7` scopes requests by the
  configured agent/MCP profiles; `crates/protocol/src/types.rs#L534-L551`
  distinguishes agent/MCP clients from the built-in assistant's approval gate.

No real mailbox was accessed by this implementation. S2 used only its isolated
fake provider; no S2 files or commit were copied into this candidate.

## Validation

`agent-work run -- env CARGO_BUILD_JOBS=4 npm run build --prefix site` passed:
10 site script tests, docs validation for 262 OpenAPI paths, and the full
208-page Astro build. Vite reported a chunk over 500 kB and Astro
reported an empty `i18n` collection; neither stopped the build. The generator
left no tracked CLI reference or OpenAPI changes.

The production preview at 1280×800 placed the headline, routes, and brew
command fully in view (the command occupied y=622–693) with no horizontal
overflow. Snapshot capture timed out twice; DOM geometry checks succeeded, and
the built page was opened in Dia for inspection.

Phone-width layout was not verified: requests for a 390×844 preview viewport
left the browser at 1280×800. Recheck the mobile breakpoint during integration.
