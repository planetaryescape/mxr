# The apps/web Vite lockfile now resolves the patched release

**Status:** candidate fixed; CI pending · **Found:** 2026-10-21 at `828722a66` on `fix/blueprint-tui-fullscreen`

At `332cd6b733093de1d4e8a51d9efddb506d59bd4e`, `apps/web/package-lock.json` resolved Vite 7.3.6, within the affected range `>=7.0.0,<=7.3.6` for [GHSA-698v-4r3x-3vcv](https://github.com/vitejs/launch-editor/security/advisories/GHSA-698v-4r3x-3vcv), [GHSA-r825-w8mm-5pxw](https://github.com/vitejs/launch-editor/security/advisories/GHSA-r825-w8mm-5pxw), and [GHSA-587h-7wmj-h5pm](https://github.com/vitejs/launch-editor/security/advisories/GHSA-587h-7wmj-h5pm). The advisories list 7.3.7 as patched; their exploitation conditions differ, including Windows and WSL requirements.

Commit `72f77c20fb56b4feb3eb452b31218202457e4f17` pins the `apps/web` Vite dev dependency to 7.3.7 and updates its lock entry. A network-only `npm audit --json` from `apps/web` returned one high-severity Vite finding and these three advisories before the update; after the update it exited 0 with zero vulnerabilities across all severities. The normal `npm install --package-lock-only --ignore-scripts --no-audit` resolver changed only the Vite manifest constraint and Vite lock entry.

The web checks have not yet completed on this candidate. CI run `39522071231` failed its web audit against the prior candidate while the typecheck job continued. Run the required web audit, tests, typecheck, lint, and build against this commit before merging. This is separate from the docs-site Vite 8.1.6 fix; the G01 docs build uses `site/` and does not require expanding that patch into `apps/web`.
