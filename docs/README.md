# mxr — Internal Docs

Contributor- and agent-facing docs. User-facing docs live in [`site/src/content/docs/`](../site/src/content/docs/) and are published to <https://mxr.planetaryescape.dev>.

| Area | Path | Purpose |
|---|---|---|
| Blueprint | [blueprint/README.md](./blueprint/README.md) | What to build: requirements, design, decisions. |
| Implementation journey | [implementation-journey.md](./implementation-journey.md) | Historical phased delivery, superseded plans, bridge/semantic context, future reauth + distro packaging. |
| Web app | [web-app/README.md](./web-app/README.md) | Per-phase execution briefs for `apps/web/`. |
| Vision | [vision.md](./vision.md) | Maintainer notes for the completed delight-plan work. |
| Reference | [reference/](./reference/) | Rust, email standards, tokio, test-quality audits. |
| Articles | [articles/](./articles/) | Essays. |
| Guides | [guides/](./guides/) | Contributor-facing how-tos (e.g. HTTP bridge internals, [writing docs](./guides/writing-docs.md)). User-facing guides live in `site/`. |
| Archive | [archive/README.md](./archive/README.md) | Historical session/handoff notes. Not maintained. |

## Conventions

- Root agent context: [`AGENTS.md`](../AGENTS.md). `CLAUDE.md` is a stub pointing here.
- Project README: [`README.md`](../README.md).
- Documentation principles (how to write good mxr docs): [`guides/writing-docs.md`](./guides/writing-docs.md) — read before editing anything under `site/src/content/docs/`.
- Generated CLI reference under `site/src/content/docs/reference/cli/` is built by `site/scripts/generate-cli-reference.mjs` — do not hand-edit.
