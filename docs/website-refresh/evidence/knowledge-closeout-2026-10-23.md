# Documentation closeout corrects metrics, examples and project boundaries

The 23 October closeout checked the website refresh against freshly fetched
`origin/main` at `b3c2f3b00aca85c2c7375030e86fb7f2380ba4fc` and the existing
Obsidian project and concept notes. This is a targeted check of the refresh's
claims, recipes and architectural lessons, not an audit of every mxr feature.
No product code or generator logic changed.

## The runtime model holds with narrower authority claims

Local mailbox ownership and reusable daemon operations remain supported.
SQLite and search stay on the daemon host while provider operations still
need their services. Shared state permits a draft handoff; it does not give
every caller the same authority. The dispatcher enforces client profiles
before handling a request. Local `agent` and `mcp` origins use named profiles;
ordinary CLI requests do not inherit the MCP profile. Paired HTTPS MCP uses
the host-approved registration grant.

Sources at the base SHA:

- `crates/daemon/src/handler/mod.rs:590–615`
- `crates/daemon/src/handler/account_scope.rs:1194–1204`
- `crates/mcp/src/lib.rs:385`
- `site/src/content/docs/guides/architecture.md`
- `site/src/content/docs/guides/security-and-privacy.md`

The existing Client-Agnostic Cores and Local Daemon Pattern notes now qualify
claims that every client is an equal peer or that a second client is
necessarily cheap. Backend reuse removes duplicated domain work; client
interaction, accessibility and distribution still require work. Unsupported
latency numbers were removed from the daemon explanation.

## Desktop distribution supersedes the old no-wrapper decision

The vault still described the removed Electron app as the last desktop
state. Current source contains a Tauri shell under `apps/desktop/`. It runs
`mxr web --print-url --no-open`, loads the local web app and uses the installed
runtime. It bundles no second frontend or daemon and exposes no Tauri invoke
commands to that page.

`apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/src/mxr_cli.rs`, the desktop guide and
`.github/workflows/release.yml` establish the source and release contracts.
GitHub metadata for v0.6.159, published 23 October 2026, lists
`mxr-desktop-0.6.159-aarch64.dmg` and its SHA-256 file. No DMG was installed
or launched during this closeout. Signing configuration does not by itself
prove that a given artifact completed optional notarisation.

The existing Mxr Desktop App project note is active again. Mxr, Mxr Web App
and Mxr Docs Site link to its current contract. The Electron removal remains
as dated history; no duplicate project note was created.

## Metrics and selection order change the question a recipe answers

Subscription `opened_count` counts messages currently carrying READ. The
store limits eligible results before the CLI ranks them, and the unscoped
command uses the configured default account. The earlier S2 observation is
still a v0.6.156 fixture result, not a current-runtime replay or a ranking of
every list across all accounts.

Wrapped uses the same READ-state proxy for its highest-volume list inside
the selected date window. The analytics guide now says that explicitly.
Sources: `crates/store/src/message.rs:1084–1157`,
`crates/daemon/src/handler/diagnostics/mod.rs:428–441`,
`crates/daemon/src/commands/subscriptions.rs:95–123`, and
`crates/store/src/wrapped.rs:565–638`.

The canonical writing guide's owed-reply example used the internal field
`counterparty_email`; CLI JSON emits `from_email`. It also treated the default
You owe lane as equivalent to the raw unanswered-thread population. The
writing guide and public recipe now name the account, candidate bound and
lane. The public recipe limits its JSON slice directly instead of truncating
text with `head`.

Sources: `crates/daemon/src/commands/owed.rs:8–38`,
`crates/daemon/src/handler/owed.rs:1–76`,
`crates/daemon/src/handler/desk_lanes.rs:773–782`, and
`crates/daemon/src/handler/diagnostics/search_execute.rs:141–216`.
The search operator filters search results against the raw owed-thread set
with its own limits; it is not an alternative rendering of the default lane.

## Recording and export checks have separate boundaries

The gallery features 14 clips; the 15 original recording pairs are retained.
Its notes distinguish synthetic mail, older TUI layouts, recorded agent
answers and edited montages. Those demonstrations do not establish current
model quality or a continuous shared-state journey.

S3 separately observed a draft saved and updated through MCP, then opened in
the actual web client with matching ID, account and body. MCP reported
revision 2; the UI did not display the numeric revision. A draft-only profile
refused a send before provider dispatch. This remains a deterministic
v0.6.156 replay, not a live Codex reasoning session.

`site/scripts/generate-llms-txt.mjs` exports authored bodies without rendering
Astro components. The homepage is exported; `<VideoGallery />` is not
expanded into clip notes in Markdown. The gallery's authored body now links
the agent quick start, account-scoped analysis recipe and current TUI controls,
so those next steps survive export. The canonical writing guide documents
how to inspect that boundary. No conversion pipeline was added.

Source and provenance: `site/src/components/VideoGallery.astro`,
`site/src/content/docs/videos.mdx`, the exporter, and the existing
[S2](./s2-runtime-proof.md), [S3](./s3-shared-draft-proof.md) and
[media map](./rung5-discovery-media-map.md).

## The existing notes own the synthesis

Three private concept notes capture the reusable findings:

- Shared state needs separate authority
- A ranking needs its metric and candidate population
- A recording proves the transitions it preserves

Nine existing notes were updated: Mxr, Mxr Desktop App, Mxr Web App,
Mxr Docs Site, Client-Agnostic Cores, The Local Daemon Pattern,
How I Write Software Docs, Generated Docs as Drift Defense, and
Durable Docs Avoid Opponent-Shaped Claims. New defining relationships are
reciprocal. Existing publication flags are preserved; no vault publishing
action was taken. The existing project and docs notes already provide the
synthesis, so no competing hub was created.

## Verification covers prose and examples, with runtime limits retained

The root checks passed:

- 10 site-script tests and the docs validator against 265 committed OpenAPI
  paths. The validator checks selected patterns and spec metadata; this is
  not live endpoint conformance.
- Both documented `jq` filters ran against 25 synthetic schema-shaped rows.
  Each returned the expected 20 rows, in descending score order, with sender
  and subject fields. No mxr command or mailbox was used for that check.
- Gallery MDX compilation returned zero messages. New authored links resolve
  to existing source routes and headings.
- Markdown siblings and `llms-full.txt` regenerated for 205 source pages;
  the new gallery guidance and Wrapped wording survived export. Existing HTML
  output was not treated as a build of this candidate.
- Twelve touched vault notes passed frontmatter, new-link target, source-path
  and reciprocal-link checks. All 27 added `related` entries have reciprocal
  entries. Two pre-existing line-wrapped wikilinks were repaired. Three older
  unresolved concept targets in Mxr remain outside this closeout.

No Rust build, real-mail access, daemon or live model run occurred. The local
machine reported elevated memory pressure, so the full locked-dependency
site build belongs to CI. Reader comprehension, conversion, current-model
quality and interactive phone rendering remain unmeasured. Deployment
verification must use the candidate SHA, not a last-ready branch alias.
