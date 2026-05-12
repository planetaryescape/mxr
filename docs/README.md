# mxr — Internal Docs

Contributor- and agent-facing docs. User-facing docs live in [`site/src/content/docs/`](../site/src/content/docs/) and are published to <https://mxr.planetaryescape.dev>.

| Area | Path | Purpose |
|---|---|---|
| Blueprint | [blueprint/README.md](./blueprint/README.md) | What to build: requirements, design, decisions. |
| Implementation | [implementation/README.md](./implementation/README.md) | How to build it: phased build plans. |
| Web app | [web-app/README.md](./web-app/README.md) | Per-phase execution briefs for `apps/web/`. |
| Vision | [vision/README.md](./vision/README.md) | Delight plan + AI-email roadmap. |
| Reference | [reference/](./reference/) | Rust, email standards, tokio, test-quality audits. |
| Articles | [articles/](./articles/) | Essays. |
| Guides | [guides/](./guides/) | Contributor-facing how-tos (e.g. HTTP bridge internals). User-facing guides live in `site/`. |
| Archive | [archive/README.md](./archive/README.md) | Historical session/handoff notes. Not maintained. |

## Conventions

- Root agent context: [`AGENTS.md`](../AGENTS.md). `CLAUDE.md` is a stub pointing here.
- Project README: [`README.md`](../README.md).
- Generated CLI reference under `site/src/content/docs/reference/cli/` is built by `site/scripts/generate-cli-reference.mjs` — do not hand-edit.
