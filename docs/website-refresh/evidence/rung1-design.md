# Rung 1 first-screen design packet

## What this comp is for

The first screen is for a technical newcomer deciding whether mxr adds something beyond an agent's Gmail connector. It names mxr as a local mail runtime, answers the connector question directly, shows one small subscription task over synced local mail, and leaves a human review step visible. The comp is static. It does not connect to mail or run commands.

## Design direction

**The working mailbox:** one ruled line carries a connected Gmail demo account into shared local state; the subscription summary and review step continue from that same mailbox. Cyan marks the local/runtime path. Amber marks the choice that remains with the person. The rules and typography carry the structure, with no feature cards, gradients, glows, or illustration unrelated to mail.

The comp keeps the current palette and typography roles from `site/src/styles/custom.css` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4`: deep blue `#071522`, soft blue `#0d2234`, pale blue `#f4f9fc`, cyan `#57d5ff`, and amber `#ffd166`; Recursive display, body, and mono roles are retained. Light mode uses the existing light-theme colors. The HTML uses a system fallback because the comp is self-contained and downloads no font. Its screenshot does not validate exact Recursive rendering.

## Opening copy

The comp uses the candidate headline “Put your whole mail history to work.” It states the category above it. The first support paragraph says a Gmail connector gives an agent access to Gmail, while mxr syncs connected accounts into a local mailbox with common operations shared by people, scripts, and agents. It makes no universal claim about connector account limits, cache behavior, permissions, or speed.

The install command remains visible. The safe trial links to `/getting-started/quick-start/#1-try-the-demo-inbox`; the agent path links to `/guides/for-agents/`. Both routes exist at the base SHA. No new site route is introduced.

## Task proof and limits

The compact task is “Find subscriptions to review.” Its fixed sender rows and percentages are synthetic illustration only, clearly labeled as such and pending S2 capture. They are not captured CLI output or a benchmark. The caption says the result is a mechanism illustration. The adjacent note says READ flags are a message-state proxy, not tracked opens, and that only the returned candidate set is ranked. The review boundary is an unsubscribe preview before any change is applied.

The small second section names Now and all five current modes to show that mxr also gives a person daily views over the mailbox. This does not promote stored-home behavior from a feature branch.

## Provenance checked at the base

`f39ae66578e3099e464967ab2f042e24b1ad8ec4` is `HEAD`, `origin/main`, and the merge base after fetching `origin main`. The worktree was clean before this comp was added.

- Existing dark palette and Recursive role definitions: `site/src/styles/custom.css#L15-L44`; light palette: `site/src/styles/custom.css#L64-L79`.
- Existing homepage product description and provider framing: `site/src/content/docs/index.mdx#L1-L16` and `site/src/content/docs/index.mdx#L19-L31`.
- Existing homepage install command: `site/src/content/docs/index.mdx#L34-L39`.
- Existing demo query and dry-run unsubscribe path: `site/src/content/docs/index.mdx#L87-L104`.
- Existing Now and five-mode summary: `site/src/content/docs/index.mdx#L191-L193`.
- Quick-start and agent guide source routes exist as `site/src/content/docs/getting-started/quick-start.md` and `site/src/content/docs/guides/for-agents.md`.
- Subscription counts group by account and lowercased sender; `opened_count` counts messages with the READ bit, while `message_count` supplies its denominator: `crates/store/src/message.rs#L1109-L1114`, `crates/store/src/message.rs#L1155-L1156`, and `crates/store/src/message.rs#L1195`.
- The subscription query applies its limit before the CLI optionally sorts the returned rows by READ-state rate: `crates/store/src/message.rs#L1150-L1153` and `crates/daemon/src/commands/subscriptions.rs#L101-L123`. That is why the comp says “returned candidate set” instead of implying a global ranking.
- Agent CLI/MCP use of the local daemon and mutation previews: `site/src/content/docs/guides/for-agents.md#L1-L8`.
- The blueprint's connector-safe opening and local-runtime definition: planning snapshot `blueprint/04-positioning.md`, “The first paragraph answers the connector question.”
- The intended one-task proof, synthetic-data boundary, returned-result limit, and READ-state caveat: planning snapshot `blueprint/05-homepage.md`, “One task proves the story before the feature tour.”
- The mailbox relationship concept and no-card/rule-led direction: planning snapshot `blueprint/03-design-concept.md`.

The CSS, homepage, query, and guide claims above were checked against the named base SHA, not inferred from PRODUCT.md. `PRODUCT.md` has older font direction; the current site CSS is the source for this comp's visual tokens.

## Review and remaining choices

The chosen aesthetic risk is to make the mailbox relationship itself the hero visual, using one continuous ruled path from account to local state to human review. The page stays within the site's existing visual vocabulary around it.

The candidate headline, connector paragraph, and “working mailbox” concept remain editorial choices for BK's design gate. S2 must replace the fixed illustration with a captured synthetic demo result before any public proof claims are made. S4 comprehension review is not complete. No runtime behavior, site source, framework, font dependency, telemetry, or auth flow changed.

The layout responds down to 390px and is intended for review at 390px and 1440px in both themes. Since no Recursive font asset is embedded, review screenshots validate spacing, hierarchy, responsive behavior, and the palette only, not exact production typography.

## Closeout

This phase adds only a self-contained evidence comp and this design note. It does not update published docs or Obsidian. No JavaScript or TypeScript was added, so the TypeScript review is not applicable. The code-simplifier pass found no executable code to simplify; the static markup and CSS avoid a theme widget and extra interaction state.
