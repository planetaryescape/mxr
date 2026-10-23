# Every authored website page has a destination and an implementation brief

Original inventory baseline: `origin/main` `f39ae66578e3099e464967ab2f042e24b1ad8ec4`, read on 2026-10-23. That tree had 80 authored Markdown/MDX files under `site/src/content/docs/` before CLI reference generation. Generated CLI children are handled by their generator, not counted as hand-authored files. This is the original route disposition inventory; priority pages were read in depth, other pages retain their mechanics pending any focused verification required by implementation. “Keep and relink” is not a claim that every existing command was replayed.

At `fe8ea0f1ba4253df34d586c82eb2a58e1a240695`, the current tree has 81 authored Markdown/MDX routes. The `getting-started/agent-quick-start.md` route was added after the original audit. The added row records its disposition; this update does not claim that the original full audit was rerun.

Use the [navigation map](../website-refresh/blueprint/07-guides-and-navigation.md) for group placement and the [verification contract](../website-refresh/blueprint/10-verification.md) for acceptance. All existing routes remain unless a separately justified redirect preserves their links.

| Current source below site/src/content/docs | Disposition | Required result |
|---|---|---|
| `examples.md` | Rewrite | Select jobs that demonstrate runtime value; link full recipes and reference rather than listing every command family. |
| `getting-started/agent-quick-start.md` | Revise | Take an existing agent through synthetic mail to a shared draft; preserve the profile and review boundaries. |
| `getting-started/first-sync.md` | Revise | Explain usable partial history, completeness and cached-content limits; point to local query verification. |
| `getting-started/gmail-setup.md` | Keep and relink | Keep provider/platform setup mechanics; link shared demo and agent route, and verify any setup command newly promoted. |
| `getting-started/imap-smtp-setup.md` | Keep and relink | Keep provider/platform setup mechanics; link shared demo and agent route, and verify any setup command newly promoted. |
| `getting-started/install.md` | Keep and relink | Keep provider/platform setup mechanics; link shared demo and agent route, and verify any setup command newly promoted. |
| `getting-started/quick-start.md` | Rewrite | Remove TUI-first assumption; demo result first; human/agent choice, sticky demo exit and sync coverage. |
| `guides/accounts.md` | Keep and relink | Connect and inspect provider accounts. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/activity-log.md` | Keep and relink | Inspect local activity and client attribution. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/adapter-development.md` | Keep and relink | Build a provider adapter. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/agent-skill.md` | Revise | Make skill versus MCP choice explicit; verify named host setup including Codex; skill guidance is not enforcement. |
| `guides/analytics.md` | Revise | Lead with local decisions; make subscription read-state and limited-ranking semantics explicit; preview touched mutation examples. |
| `guides/architecture.md` | Revise | Include web, MCP and skill accurately; provider abstraction and shared state before IPC taxonomy. |
| `guides/archive-intelligence.md` | Keep and relink | Ask questions grounded in archive evidence. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/archive.md` | Keep and relink | Retrieve sourced record facts and documents. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/assistant.md` | Revise | Position as an optional built-in route; preserve exact approval behaviour and distinguish native configuration from existing-agent use. |
| `guides/automated-followups.md` | Keep and relink | Track a reply and follow up. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/automation-contract.md` | Revise | Own previews, retained identity/revision where applicable, output contracts, scope, outcomes, retry and recovery. |
| `guides/backup-recovery.md` | Keep and relink | Back up and restore the selected runtime. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/briefings-and-loop-in.md` | Keep and relink | Recover thread and recipient context. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/calendar-invites.md` | Keep and relink | Review and respond to calendar mail. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/compose.md` | Keep and relink | Write and review a draft. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/crash-safe-drafts.md` | Keep and relink | Recover durable draft state. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/deliveries.md` | Keep and relink | Inspect delivery evidence. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/desktop-app.md` | Keep and relink | Use the desktop wrapper over the web client. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/email-modes.md` | Revise | Keep five-jobs explanation; add runtime relationship; distinguish email home from conversation jobs only after release verification. |
| `guides/focus-and-reply.md` | Keep and relink | Finish owed replies. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/for-agents.md` | Revise | Keep worked examples; put correct profile/CLI-origin boundary near first action; show scoped evidence-preview-verify loop. |
| `guides/forgotten-work.md` | Keep and relink | Inspect commitments and stale work. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/glossary.md` | Revise | Define runtime, account, mode, Now, Inbox, assistant, agent, Archive/action and runtime host consistently. |
| `guides/headless-linux.md` | Keep and relink | Operate a Linux runtime. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/labels-and-saved-searches.md` | Keep and relink | Organise provider labels and saved queries. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/linked-drafts.md` | Revise | Use as shared-draft canonical guide; identify local versus Gmail-linked support and current revision/outcome contract. |
| `guides/llm-features.md` | Revise | Separate external-agent model, native synthesis and assistant; verify routing/defaults and each disclosure. |
| `guides/mail-merge.md` | Keep and relink | Preview and send an authorised merge. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/mailbox.md` | Revise | Keep arrival-order mailbox tasks as a deliberate view; link modes as the daily workflow rather than calling Inbox the whole product. |
| `guides/marginalia.md` | Keep and relink | Transfer selected reading and sync highlights. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/messages.md` | Keep and relink | Work through conversations with people. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/now.md` | Keep and relink | Choose the next item across modes. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/observability.md` | Keep and relink | Diagnose runtime state. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/pairing-authority.md` | Keep and relink | Inspect or revoke client authority. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/personal-categories.md` | Keep and relink | Group mail independently of modes and provider labels. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/pre-send-safety.md` | Keep and relink | Review actual pre-send checks and limits. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/public-rust-crates.md` | Keep and relink | Reuse standalone mail building blocks. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/raycast.md` | Keep and relink | Use quick actions over the same runtime. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/reading-and-paper-trail.md` | Revise | Retain compatibility workflow; explain its relation to Reading, Updates and Archive records without conflating names. |
| `guides/reading.md` | Keep and relink | Read an edition and save items. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/recipes.md` | Revise | Goal-shaped local workflows, exact output, account scope, preview and verification; no blanket model claims. |
| `guides/remote-mailbox.md` | Keep and relink | Use a selected runtime on another host. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/rules.md` | Keep and relink | Preview and apply repeatable mail rules. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/search.md` | Keep and relink | Find synced mail and inspect result scope. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/security-and-privacy.md` | Revise | State runtime-host boundary, identified profiles, actual activity/undo limits, model and attachment network paths. |
| `guides/semantic-search.md` | Keep and relink | Enable or inspect optional local retrieval. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/sender-view.md` | Keep and relink | Inspect sender history and relationship context. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/snippets.md` | Keep and relink | Reuse authored compose text. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/sound-hints-and-touch.md` | Keep and relink | Configure client interaction feedback. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/timing-and-cadence.md` | Keep and relink | Schedule reviewed sends and timing actions. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/todo-resolution.md` | Keep and relink | Review later evidence before resolving work. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/todo.md` | Keep and relink | Act on dated evidenced tasks. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/trace-model-calls.md` | Keep and relink | Trace model calls in a developer build. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/triage-flow.md` | Keep and relink | Handle arrival-order mail safely. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/unsubscribe.md` | Keep and relink | Review and apply an unsubscribe method. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/updates.md` | Keep and relink | Glance at source-grouped changes and let go. Preserve current workflow; add the canonical parent/next-task links and verify any promoted claim. |
| `guides/web-app.md` | Revise | Explain the human client over the same runtime, starting Now; preserve real controls and link agent-draft review. |
| `guides/why-mxr.md` | Rewrite | Own the connector answer, runtime fit, composability, common state, modes and explicit trade-offs. |
| `guides/your-day.md` | Revise | One full human route from Now through the modes; show why each shape and completion action differs. |
| `index.mdx` | Rewrite | Category and connector answer precede one local task proof; add modes, shared-review and bounded trust in order. |
| `reference/adapters.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/bridge.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/bug-report.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/cli/concepts.md` | Revise | Use current mode/runtime nouns and correct JSON envelopes; retain exact search and format semantics. |
| `reference/config.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/conformance.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/desk-and-places.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/json-output.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/keybindings.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/mcp.md` | Revise | Keep exact tools/contracts; provide a short setup pointer and accurate profile/confirm/identity boundaries. |
| `reference/time-phrases.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `reference/tui.md` | Keep reference | Preserve exact technical contract; link the owning tutorial/how-to. Regenerate source-derived fields rather than hand-editing them. |
| `troubleshooting.md` | Revise | Add journey-specific routing for wrong runtime/profile, partial sync, model disabled and permission refusal using actual errors. |
| `videos.mdx` | Revise | Audit each capture for current behaviour; label synthetic/canned content, retain useful proofs and re-record stale stories. |

## Sources outside the authored guide tree also change

| Source | Disposition and reason |
|---|---|
| `site/src/pages/privacy.md`, `terms.md`, `layouts/legal.astro` | Audit category/data-flow consistency; keep legal obligations and layout unless a verified change warrants revision. |
| `site/src/pages/404.astro` | Keep useful recovery; verify navigation after reorganisation. |
| `site/src/pages/reference/api-explorer.astro`, `api/v1/openapi.json.ts` | Keep exact API surface and route inventory; verify generated spec/current coverage. |
| `site/astro.config.mjs`, `site/vercel.json`, `site/public/robots.txt` | Update sidebar/head metadata at source; preserve production host, redirects and existing deployment configuration. |
| `site/public/llms.txt`, generator-produced `.md` siblings and `llms-full.txt` | Rewrite curated category/task index and regenerate full outputs; inspect actual emitted text. |
| `site/scripts/generate-cli-reference.mjs`, examples JSON, help snapshots | Keep command-generation authority; change examples only after replay. |
| `site/scripts/dump-openapi-spec.sh`, `site/public/openapi.json` | Keep generation/check contract; do not manually patch generated JSON. |
| `site/scripts/generate-social-images.mjs`, `site/public/og.png` | Correct source promises and synthetic timing; regenerate and inspect image. |
| `site/public/*webm`, posters, `site/public/videos/`, `VideoGallery.astro`, recording tape/scripts | Keep verified relevant captures; recaption or re-record those that teach the old story or wrong commands. |
| `site/src/assets/architecture.svg` | Redraw only as needed to make shared state, clients and provider boundary accurate; provide useful alt text. |
| `site/src/styles/custom.css`, `Footer.astro` | Refine touched hierarchy using current tokens; keep themes, search and useful footer navigation. |
| `README.md`, `PRODUCT.md`, `DESIGN.md`, `ARCHITECTURE.md` | Align current pitch/surface descriptions and link canonical website guides; preserve historical plans and avoid unrelated rewrites. |

## New pages need a missing task, not a missing keyword

- `/getting-started/agent-quick-start/`: one complete synthetic-mail agent-to-human-draft tutorial. Existing For agents is the operating guide, not a newcomer setup sequence.
- `/guides/invoice-workflow/`: conditional new attachment workflow. Publish only if replay proves the whole selection/download/manifest/verification path. Otherwise improve the existing proven recipe; do not add product features.

## This inventory is also the coverage check

Before handoff, compare the table paths to `rg --files site/src/content/docs` excluding generated files. When the baseline moves, add new authored routes to this table and the appropriate navigation group rather than silently omitting them.
