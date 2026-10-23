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

At that check, phone-width layout was not verified: requests for a 390×844
preview viewport left the browser at 1280×800. Round 2 adds a responsive HTML
fallback check below; the actual production page's phone layout remains
unverified.

## Round 2: first-screen task proof

This implementation starts from parent commit
`08e5d54f99bb6ca3e7654da16b718fa5c290ea41` (tree
`1d2bfc2271441a276bcf92042ce9bc4a9eabd89b`). The refreshed HTTPS fetch resolved
`origin/main` to `f39ae66578e3099e464967ab2f042e24b1ad8ec4`; that is also the
merge base. The category line is ordinary hero markup before the headline. The
homepage now pairs the connector explanation and install action with one
static mailbox task: local READ-state evidence, an explicit agent proposal to
review Field Notes first, and a human unsubscribe preview. The existing
CLI/TUI/agent media carousel follows this opening proof. The task rows and
proposal are labeled synthetic illustration; S2's captured fixture did not
provide useful READ-rate variation.

The example names its account scope. Its note says an unscoped request uses the
configured default account, the query limits to 200 eligible sender buckets
before ranking, and counts use current READ flags rather than tracked opens or
all newsletters. The operation ends at preview and human choice; the page does
not claim that mxr unsubscribes automatically. These limits are checked against
the source references at `f39ae66578e3099e464967ab2f042e24b1ad8ec4` in the S2
evidence above.

### Round 2 rendering and build

`npm ci --prefix site` passed with 528 packages installed and no reported
vulnerabilities. `npm run build --prefix site`, run with a Node-only `PATH` in
the shared agent-work queue, passed 10 site script tests, validation of 262
OpenAPI paths, and the 208-page Astro build. The generator used the committed
`site/public/openapi.json` because Cargo was excluded from `PATH`; this was a
hosting-mode build, not a live OpenAPI conformance check. Vite reported a chunk
over 500 kB and Astro reported an empty `i18n` collection; both warnings were
non-blocking. No generated reference or OpenAPI source changes remained.

The built page was previewed at 1280×800 in Dia. The preview DOM reported the
hero at x=75–581, the proof at x=645–1205 and y=139–749, and the install row at
y=598–669, with no horizontal overflow. The real category text precedes the
headline in DOM order. A light-theme DOM check returned the configured light
background and foreground colors. The collaborative browser's request to
resize to 390×844 still reported 1280×800; its page snapshot timed out twice.

The native HTML preview therefore checked a self-contained rendering of the
built opening copy and the same layout at 390 and 1440 px in both themes. It
reported 390 px document/body widths at 390 and 1440 px widths at 1440, with no
console errors. This fallback uses system sans typography and distilled
homepage CSS rather than the production Recursive font and full Starlight CSS;
it checks responsive hierarchy and overflow, not exact production-font
wrapping. No phone-width pass is claimed for the actual production page.

## Root integration verifies the production font and layout

Root reviewed candidate `920d6c23dea185b17c4be0dee5f3f442370b8e75`, tree `44f701b9886ab4b889b4ca0971332af5a5f1c503`. The task-first proof resolves the opening mismatch. An inherited percentage row gap made the desktop hero's content overflow its intrinsic height; setting a fixed `1rem` row gap separates the action links from the install row. At1440px, the last action ends at636.8px and the install row begins at650.8px.

Root rendered the built HTML with all compiled site CSS in native HTML preview at390px and1440px, both themes. The actual Recursive font loaded in each check. Its downloaded production asset and built asset both hash to `14ef41458aeba3d812284c4e32ee956910b88845062c61e91cc8578c2b78bccb`. Document and body widths matched the requested width; no console errors or horizontal overflow occurred. The full proof remains readable by scrolling on a phone.

The extraction strips external JavaScript and resolves viewport-height units to844px for the phone capture and900px for desktop because the native renderer rejects viewport-height units. It retains the compiled layout rules and typography. This verifies static production layout rather than mobile browser interaction. The earlier distilled-CSS check is superseded for layout; collaborative-browser resize remains faulty.

The final root build passed10script tests, validation for262OpenAPIpaths, and208Astropages in4.35seconds. It used the supported npm-only hosting fallback for the unchanged OpenAPI file. All36homepage internal links and anchors resolve in the built output. The generated homepage Markdown, Why mxr Markdown, curated llms.txt and llms-full retain the category. Node syntax checks passed for the two changed JS files. Blueprint validation reports17documents/27decisions with no errors or warnings. No new types, contracts or runtime behavior were introduced. Inline simplifier review kept the static proof and existing copy/media behavior; no additional abstraction was needed.

Remaining evidence: no three-reader comprehension study has been performed. Deployment and live browser smoke are recorded by root after integration.
