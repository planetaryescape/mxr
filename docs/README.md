# mxr — Internal Docs

Contributor- and agent-facing docs. User-facing docs live in [`site/src/content/docs/`](../site/src/content/docs/) and are published to <https://mxr.sh>.

| Area | Path | Purpose |
|---|---|---|
| Blueprint | [blueprint/README.md](./blueprint/README.md) | What to build: requirements, design, decisions. |
| Email modes | [blueprint/22-email-modes.md](./blueprint/22-email-modes.md) | The product model: email as five apps (Messages, To do, Updates, Reading, Archive), Now, handoff, model tiers, and the phases that build it. |
| Research | [research/](./research/) | Evidence behind the modes ([email-modes/](./research/email-modes/)) and the model-fit review ([model-fit.md](./research/model-fit.md)). |
| Local view performance | [mail-view-performance-2026-10-26.md](./research/mail-view-performance-2026-10-26.md) | Real-mailbox latency findings and a read-only benchmark. |
| Docs inventory | [site-modes-inventory.md](./site-modes-inventory.md) | Every site page and repo doc sorted by when the modes change it. |
| Calendar email | [calendar-email/README.md](./calendar-email/README.md) | Current code-truth, synthesis notes, and historical implementation plan for email calendar invites. |
| Implementation journey | [implementation-journey.md](./implementation-journey.md) | Historical phased delivery, superseded plans, bridge/semantic context, future reauth + distro packaging. |
| Web app | [web-app.md](./web-app.md) | Maintainer notes and implemented behavior for `apps/web/`. |
| Vision | [vision.md](./vision.md) | Where the product thesis now lives, plus maintainer notes for the completed delight-plan work. |
| Reference | [reference/](./reference/) | Rust, email standards, tokio, test-quality audits. |
| Articles | [articles/](./articles/) | Essays. |
| Guides | [guides/](./guides/) | Contributor-facing how-tos (e.g. HTTP bridge internals, [writing docs](./guides/writing-docs.md)). User-facing guides live in `site/`. |

## Find the shipped workflows

Use the public guides for current behavior; the blueprint retains design and
future work. Start with [the daily workflow](https://mxr.sh/guides/your-day/),
[To do](https://mxr.sh/guides/todo/),
[reviewing later evidence](https://mxr.sh/guides/todo-resolution/),
[personal categories](https://mxr.sh/guides/personal-categories/), and
[backup and recovery](https://mxr.sh/guides/backup-recovery/).

For docs changes, follow the generation and validation commands in
[Contributing](../CONTRIBUTING.md#docs-and-release-hygiene). Update the guide
and its navigation links together.

## Conventions

- Root agent context: [`AGENTS.md`](../AGENTS.md). Scoped context lives in `.agents/skills/`.
- Project README: [`README.md`](../README.md).
- Documentation principles (how to write good mxr docs): [`guides/writing-docs.md`](./guides/writing-docs.md) — read before editing anything under `site/src/content/docs/`.
- Generated CLI reference under `site/src/content/docs/reference/cli/` is built by `site/scripts/generate-cli-reference.mjs` — do not hand-edit.
