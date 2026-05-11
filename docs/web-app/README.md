# mxr web app — implementation docs

These are the **execution-side** docs for building the mxr web app at `apps/web/`. The original plan lives at `~/.claude/plans/you-excel-at-making-concurrent-rabbit.md`; these files turn it into self-contained per-phase briefs so a fresh session can pick up where the last one left off without re-reading the whole plan.

## Read order

1. [00-overview.md](./00-overview.md) — context, locked decisions, architecture, file layout, design system, conventions. **Always read first.**
2. [01-bootstrap.md](./01-bootstrap.md) — `apps/web/` skeleton, `mxr web` command, embedded SPA serving, auth bootstrap.
3. [02-mailbox-reader.md](./02-mailbox-reader.md) — virtualized mailbox list, thread reader, optimistic mutations.
4. [03-compose.md](./03-compose.md) — full-page compose with CodeMirror+vim default and Tiptap alt.
5. [04-search.md](./04-search.md) — top-bar search, results page, saved searches, lexical/semantic toggle.
6. [05-command-palette.md](./05-command-palette.md) — Cmd-K overlay with cmdk, scoped fuzzy match.
7. [06-analytics.md](./06-analytics.md) — six dashboards including Wrapped story+dashboard modes.
8. [07-rules.md](./07-rules.md) — rules editor with always-visible dry-run.
9. [08-accounts-onboarding.md](./08-accounts-onboarding.md) — first-run wizard, OAuth device flow, account management.
10. [09-polish.md](./09-polish.md) — screener, settings, VIP allowlist, diagnostics, theme picker, browser notifications, e2e suite, perf budget.
11. [10-v1-launch.md](./10-v1-launch.md) — TDD plan that closes out v1 launch blockers (C1–C4, H1–H8). Ordered, decisions locked, ~5 days.

[STATUS.md](./STATUS.md) tracks current progress across phases — update it as work lands.

## Working rules for an unattended session

- This is **autonomous execution**: make decisions and keep moving. Document any non-obvious calls in the relevant phase doc.
- If context approaches 50% utilisation, run `/compact`, then re-load `00-overview.md` plus the in-progress phase doc plus `STATUS.md` to continue.
- Do not import or share UI from `apps/desktop/`. The web app is a fresh codebase. The only cross-cutting surface is the daemon HTTP bridge contract.
- Test against a real running daemon with the FakeProvider when smoke-checking. Don't trust unit tests alone (CLAUDE.md mandate).
- Every TUI action must have a CLI equivalent and now a web equivalent. Web parity is non-negotiable.
- Optimistic UI for archive/trash/star/markRead/markUnread/move/label/snooze. 60-second undo affordance.
- Keep the surface area minimal — no premature abstractions, no unused indirection. Three similar lines beat a premature factory.
