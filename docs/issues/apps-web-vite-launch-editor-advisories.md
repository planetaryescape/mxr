# The apps/web Vite lockfile now resolves the patched release

Status: resolved in v0.6.117 · Found: 2026-10-21 at `828722a66` on `fix/blueprint-tui-fullscreen`

At `332cd6b733093de1d4e8a51d9efddb506d59bd4e`, `apps/web/package-lock.json` resolved Vite 7.3.6, within the affected range `>=7.0.0,<=7.3.6` for [GHSA-698v-4r3x-3vcv](https://github.com/vitejs/launch-editor/security/advisories/GHSA-698v-4r3x-3vcv), [GHSA-r825-w8mm-5pxw](https://github.com/vitejs/launch-editor/security/advisories/GHSA-r825-w8mm-5pxw), and [GHSA-587h-7wmj-h5pm](https://github.com/vitejs/launch-editor/security/advisories/GHSA-587h-7wmj-h5pm). The advisories list 7.3.7 as patched; their exploitation conditions differ, including Windows and WSL requirements.

Commit `72f77c20fb56b4feb3eb452b31218202457e4f17` pins the `apps/web` Vite dev dependency to 7.3.7 and updates its lock entry. A network-only `npm audit --json` from `apps/web` returned one high-severity Vite finding and these three advisories before the update; after the update it exited 0 with zero vulnerabilities across all severities. The normal `npm install --package-lock-only --ignore-scripts --no-audit` resolver changed only the Vite manifest constraint and Vite lock entry.

[Final PR CI](https://github.com/planetaryescape/mxr/actions/runs/39523116233)
passed audit, typecheck, lint, unit tests and both Chromium shards on
`795e90295a7d88b89ab81f88c3cff486466ad5bc`. The tagged release build passed
with this unchanged web dependency tree in v0.6.117, source
`51c747a5992b73a9c4729fdb4e6c325967cd7d4d`. The site retains its separate
Vite 8.1.6 patch.
