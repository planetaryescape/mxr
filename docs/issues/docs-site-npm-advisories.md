# Docs site: npm audit fails on new advisories

**Status:** resolved 2026-09-28 on `docs/site-refresh` · **Found:** 2026-09-27 on PR #233 (also affected `main`)

The `Docs Build` CI job runs `npm audit --audit-level=moderate` in `site/`
and failed on advisories published after the last green run, so CI never
reached `npm run build`. A frontmatter bug in PR #247 got past CI for that
reason and only the Vercel build caught it.

## Fix

- `astro` 7.1.3 -> 7.3.3 (the critical advisories cover `<= 7.2.7`).
- `sharp` ^0.35.3 -> 0.35.4 (the libheif advisory covers `< 0.35.4`).
- `overrides` pins `ai` to 6.0.286 and `@ai-sdk/vue` to 3.0.286. The
  latest `@scalar/agent-chat` (0.12.37, pulled in by
  `@scalar/api-reference`) still pins the vulnerable `ai` 6.0.33 and
  `@ai-sdk/vue` 3.0.33, so upgrading Scalar alone does not clear them. The
  overrides stay on the same majors.
- `npm audit fix` for `devalue`, `js-yaml`, `smol-toml` and `svgo`.

Result: `npm ci` then `npm audit` reports 0 vulnerabilities, and
`npm run build` passes (169 pages). The API route inventory page still
renders.

## Why the earlier installs stalled

`~/.npmrc` sets `min-release-age=7`. `astro` 7.3.5 was younger than seven
days, so npm could never resolve it and looped on `astro`/`sharp` instead
of failing. A pinned version inside the window fails fast with `ETARGET`
("No matching version found ... with a date before ..."). Pick versions
older than the quarantine window, which is what the versions above are.

## Follow-up

When the overrides' parent (`@scalar/agent-chat`) ships a fixed `ai`,
drop the two overrides.

## 2026-10-03: GHSA-ch52-4w7c-c8xp has no fixed release

`http-cache-semantics` 4.2.0, pulled in by `astro`, is the latest release
and is affected by a cross-user disclosure through a shared HTTP cache.
The docs site is a static build with no shared server cache, so the risk
does not apply. `Docs Build` now runs `scripts/check-npm-audit.mjs`, which
fails on any moderate or higher advisory except entries in
`site/audit-allowlist.json`, each with a reason and an expiry date. This
entry expires 2026-11-03; remove it when a fixed release ships. The web
app's advisories (`dompurify`, `chokidar` via the TanStack router plugin)
were fixed with `npm audit fix`.
