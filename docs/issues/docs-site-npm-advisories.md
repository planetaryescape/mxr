# Docs site: npm audit fails on new advisories

**Status:** open · **Found:** 2026-09-27 on PR #233 (also affects `main`)

The `Docs Build` CI job runs `npm audit --audit-level=moderate` in `site/`
and fails on advisories published after the last green run:

- `astro` <= 7.2.7 (critical); `site/package.json` pins `7.1.3`. 7.3.5 is
  within Starlight 0.41.3's `^7.0.2` peer range.
- `@scalar/api-reference` pulls `@ai-sdk/provider-utils` and `undici` with
  advisories; 1.72.1 is current (the lockfile has 1.69.2).
- `devalue`, `js-yaml`, `sharp`, `smol-toml`, `svgo`: fixed by `npm audit fix`.

`npm install` for the upgrade stalled locally (over 20 minutes, twice), so it
was not landed with #233. Next step: upgrade `astro` to 7.3.5 and
`@scalar/api-reference` to ^1.72.1, run `npm audit fix`, then build the
site and click through the API reference page before merging.

`Docs Build` is not a required check.
